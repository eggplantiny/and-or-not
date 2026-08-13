#![forbid(unsafe_code)]

pub mod cell_buffer;
pub mod editor;
pub mod host_action;
pub mod inspector;
pub mod laboratory;
pub mod native_editor;
pub mod native_laboratory;
pub mod native_probe;
pub mod pacing;
pub mod presenter;
pub mod probe;

use aon_sim::{
    ArtifactBytes, CommandEnvelope, DriverId, EntityId, GateId, LogicLevel, MobileId, PackageError,
    PhysicalScaleProfile, RenderSnapshot, Replay, ReplayError, SignalProbeTarget, Simulation,
    SimulationError, SimulationPackage, StateHash, StepReport, Tick, decode_package,
    decode_replay_artifact,
};
use bevy::input::{ButtonState, keyboard::KeyboardInput};
use bevy::prelude::*;
use bevy::time::{TimeUpdateStrategy, Virtual};
use bevy::window::{PrimaryWindow, WindowPlugin};
use std::time::Duration;
use thiserror::Error;

const EMPTY_SCENARIO: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/scenarios/empty.json"
));
const EMPTY_NUMERIC_PROFILE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/numeric/v1.json"
));
const EMPTY_PHYSICAL_SCALE_PROFILE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/physical-scale/stage0-alpha.json"
));
const EMPTY_BALANCE_PROFILE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/balance/stage0-alpha.json"
));
const STAGE0_RETAINED_STATE_REPLAY: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/replays/mobility-retained-stop-v1.json"
));
const STAGE0_CURRENT_INPUT_ONLY_REPLAY: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/replays/mobility-current-input-stop-v1.json"
));
const STAGE0_PRODUCT_PROBE_TARGETS_TICK: Tick = Tick(5);
const STAGE0_PRODUCT_PROBE_READY_TICK: Tick = Tick(24);
const STAGE0_PRODUCT_PROBE_MOBILE: MobileId = MobileId(EntityId(4));
const STAGE0_PRODUCT_PROBE_Q: GateId = GateId(EntityId(6));
const STAGE0_PRODUCT_PROBE_QBAR: GateId = GateId(EntityId(8));
const STAGE0_PRODUCT_PROBE_SET: DriverId = DriverId(EntityId(7));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage0ProductDesign {
    CurrentInputOnly,
    RetainedState,
}

impl Stage0ProductDesign {
    const fn replay_bytes(self) -> &'static [u8] {
        match self {
            Self::CurrentInputOnly => STAGE0_CURRENT_INPUT_ONLY_REPLAY,
            Self::RetainedState => STAGE0_RETAINED_STATE_REPLAY,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::CurrentInputOnly => "CURRENT INPUT ONLY",
            Self::RetainedState => "RETAINED STATE",
        }
    }

    const fn hotkey(self) -> &'static str {
        match self {
            Self::CurrentInputOnly => "F5",
            Self::RetainedState => "F6",
        }
    }
}

#[derive(Debug, Error)]
pub enum HostError {
    #[error(transparent)]
    Package(#[from] PackageError),

    #[error(transparent)]
    Simulation(#[from] SimulationError),

    #[error(transparent)]
    Replay(#[from] ReplayError),

    #[error(transparent)]
    NativeProbe(#[from] native_probe::NativeProbeError),

    #[error(transparent)]
    Laboratory(#[from] laboratory::LaboratoryError),

    #[error("embedded Stage 0 product probe invariant failed: {0}")]
    ProductProbeInvariant(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostRunMode {
    Paused,
    Running,
}

#[derive(Resource)]
pub struct CanonicalSimulation {
    simulation: Simulation,
}

impl CanonicalSimulation {
    pub fn state_hash(&self) -> StateHash {
        self.simulation.state_hash()
    }

    pub const fn next_tick(&self) -> Tick {
        self.simulation.next_tick()
    }
}

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct LatestRenderSnapshot {
    snapshot: RenderSnapshot,
}

impl LatestRenderSnapshot {
    pub fn get(&self) -> &RenderSnapshot {
        &self.snapshot
    }
}

#[derive(Resource)]
pub struct SimulationHostState {
    mode: HostRunMode,
}

impl SimulationHostState {
    pub const fn mode(&self) -> HostRunMode {
        self.mode
    }

    pub const fn set_mode(&mut self, mode: HostRunMode) {
        self.mode = mode;
    }
}

#[derive(Resource)]
struct HostTraceResource {
    checkpoints: Vec<StateHash>,
    reports: Vec<StepReport>,
}

#[derive(Resource)]
struct ReplayCommandSchedule {
    replay: Replay,
}

#[derive(Resource)]
struct NativePresentationConfig {
    physical: PhysicalScaleProfile,
    viewport: presenter::Viewport,
}

#[derive(Resource)]
struct Stage0ProductProbeControl {
    package: SimulationPackage,
    active_design: Stage0ProductDesign,
    switch_error: Option<String>,
}

#[derive(Resource, Default)]
struct HostFault {
    error: Option<HostError>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostTrace {
    checkpoints: Vec<StateHash>,
    reports: Vec<StepReport>,
}

impl HostTrace {
    pub fn checkpoints(&self) -> &[StateHash] {
        &self.checkpoints
    }

    pub fn reports(&self) -> &[StepReport] {
        &self.reports
    }

    pub fn final_hash(&self) -> StateHash {
        self.checkpoints
            .last()
            .copied()
            .expect("a host trace always includes the initial state")
    }
}

pub struct SimulationHostPlugin {
    presenter_enabled: bool,
}

impl SimulationHostPlugin {
    pub const fn with_presenter() -> Self {
        Self {
            presenter_enabled: true,
        }
    }

    pub const fn without_presenter() -> Self {
        Self {
            presenter_enabled: false,
        }
    }
}

impl Plugin for SimulationHostPlugin {
    fn build(&self, app: &mut App) {
        app.init_schedule(FixedUpdate);
        app.init_schedule(Update);
        app.add_systems(FixedUpdate, advance_canonical_simulation);
        if self.presenter_enabled {
            app.add_systems(
                Update,
                (refresh_render_snapshot, present_empty_world_title).chain(),
            );
        }
    }
}

pub fn embedded_empty_package() -> Result<SimulationPackage, PackageError> {
    decode_package(ArtifactBytes {
        scenario: EMPTY_SCENARIO,
        numeric_profile: EMPTY_NUMERIC_PROFILE,
        physical_scale_profile: EMPTY_PHYSICAL_SCALE_PROFILE,
        balance_profile: EMPTY_BALANCE_PROFILE,
    })
}

fn stage0_product_probe_session(
    package: SimulationPackage,
    design: Stage0ProductDesign,
) -> Result<laboratory::LaboratorySession, HostError> {
    let artifact = decode_replay_artifact(design.replay_bytes())?;
    let (scenario_path, replay) = artifact.into_parts();
    if scenario_path != "../scenarios/empty.json"
        || !replay
            .checkpoints()
            .iter()
            .any(|checkpoint| checkpoint.next_tick == STAGE0_PRODUCT_PROBE_READY_TICK)
    {
        return Err(HostError::ProductProbeInvariant(
            "A/B Replay locator or ready checkpoint changed",
        ));
    }

    let mut laboratory = laboratory::LaboratorySession::from_replay(package, replay)?;
    while laboratory.next_tick() < STAGE0_PRODUCT_PROBE_TARGETS_TICK {
        laboratory.step_once()?;
    }
    if laboratory.next_tick() != STAGE0_PRODUCT_PROBE_TARGETS_TICK {
        return Err(HostError::ProductProbeInvariant(
            "Replay did not create probe targets at the frozen Tick",
        ));
    }

    let stop = laboratory
        .latest_snapshot()
        .mobiles()
        .iter()
        .find(|mobile| mobile.id == STAGE0_PRODUCT_PROBE_MOBILE)
        .map(|mobile| mobile.ports.stop)
        .ok_or(HostError::ProductProbeInvariant(
            "ready checkpoint has no Mobile",
        ))?;
    for target in [
        SignalProbeTarget::Driver(STAGE0_PRODUCT_PROBE_SET),
        SignalProbeTarget::GateOutput(STAGE0_PRODUCT_PROBE_Q),
        SignalProbeTarget::GateOutput(STAGE0_PRODUCT_PROBE_QBAR),
        SignalProbeTarget::Sink(stop),
    ] {
        laboratory.add_probe(target)?;
    }
    while laboratory.next_tick() < STAGE0_PRODUCT_PROBE_READY_TICK {
        laboratory.step_once()?;
    }
    if laboratory.next_tick() != STAGE0_PRODUCT_PROBE_READY_TICK {
        return Err(HostError::ProductProbeInvariant(
            "Replay did not stop at the frozen ready Tick",
        ));
    }
    for (target, expected) in [
        (
            SignalProbeTarget::Driver(STAGE0_PRODUCT_PROBE_SET),
            LogicLevel::Low,
        ),
        (
            SignalProbeTarget::GateOutput(STAGE0_PRODUCT_PROBE_Q),
            LogicLevel::Low,
        ),
        (
            SignalProbeTarget::GateOutput(STAGE0_PRODUCT_PROBE_QBAR),
            LogicLevel::High,
        ),
        (SignalProbeTarget::Sink(stop), LogicLevel::Low),
    ] {
        let sample = laboratory
            .probes()
            .traces()
            .find_map(|(_, trace)| (trace.target() == target).then(|| trace.latest()).flatten())
            .ok_or(HostError::ProductProbeInvariant(
                "ready checkpoint probe history is empty",
            ))?;
        if sample.next_tick != STAGE0_PRODUCT_PROBE_READY_TICK || sample.logic_level() != expected {
            return Err(HostError::ProductProbeInvariant(
                "ready checkpoint probe sample changed",
            ));
        }
    }
    laboratory.set_selection(Some(presenter::PickTarget::Entity(
        STAGE0_PRODUCT_PROBE_MOBILE.entity_id(),
    )));
    Ok(laboratory)
}

pub fn run_host_harness(
    package: SimulationPackage,
    ticks: u64,
    presentation_updates: u32,
    presenter_enabled: bool,
) -> Result<HostTrace, HostError> {
    let simulation = Simulation::new(package)?;
    let simulation_hz = simulation.profiles().balance().simulation_hz;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    configure_host_pacing(&mut app, simulation_hz);
    install_simulation(
        &mut app,
        simulation,
        HostRunMode::Running,
        presenter_enabled,
    );

    run_presentation_updates(&mut app, presentation_updates);
    for _ in 0..ticks {
        app.world_mut().run_schedule(FixedUpdate);
        run_presentation_updates(&mut app, presentation_updates);
    }

    if let Some(error) = take_host_fault(&mut app) {
        return Err(error);
    }

    let trace = app.world().resource::<HostTraceResource>();
    Ok(HostTrace {
        checkpoints: trace.checkpoints.clone(),
        reports: trace.reports.clone(),
    })
}

pub fn run_replay_host_harness(
    package: SimulationPackage,
    replay: Replay,
    presentation_updates: u32,
    presenter_enabled: bool,
) -> Result<HostTrace, HostError> {
    let simulation = Simulation::new(package)?;
    replay.validate_against(&simulation)?;
    let simulation_hz = simulation.profiles().balance().simulation_hz;
    let final_next_tick = replay.final_next_tick();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    configure_host_pacing(&mut app, simulation_hz);
    install_replay_simulation(&mut app, simulation, replay, presenter_enabled);

    run_presentation_updates(&mut app, presentation_updates);
    for _ in 0..final_next_tick.0 {
        app.world_mut().run_schedule(FixedUpdate);
        if host_has_fault(&app) {
            break;
        }
        run_presentation_updates(&mut app, presentation_updates);
    }

    finish_replay_host(&mut app)
}

pub fn run_paced_host_harness(
    package: SimulationPackage,
    frame_deltas: &[Duration],
) -> Result<HostTrace, HostError> {
    let simulation = Simulation::new(package)?;
    let simulation_hz = simulation.profiles().balance().simulation_hz;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    configure_host_pacing(&mut app, simulation_hz);
    install_simulation(&mut app, simulation, HostRunMode::Running, true);

    // Bevy's Real clock uses its first update to establish an origin and
    // intentionally reports a zero delta for that update.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    app.update();
    for frame_delta in frame_deltas {
        app.insert_resource(TimeUpdateStrategy::ManualDuration(*frame_delta));
        app.update();
    }

    if let Some(error) = take_host_fault(&mut app) {
        return Err(error);
    }

    let trace = app.world().resource::<HostTraceResource>();
    Ok(HostTrace {
        checkpoints: trace.checkpoints.clone(),
        reports: trace.reports.clone(),
    })
}

pub fn run_paced_replay_host_harness(
    package: SimulationPackage,
    replay: Replay,
    frame_deltas: &[Duration],
) -> Result<HostTrace, HostError> {
    let simulation = Simulation::new(package)?;
    replay.validate_against(&simulation)?;
    let simulation_hz = simulation.profiles().balance().simulation_hz;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    configure_host_pacing(&mut app, simulation_hz);
    install_replay_simulation(&mut app, simulation, replay, true);

    // Bevy's Real clock uses its first update to establish an origin and
    // intentionally reports a zero delta for that update.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    app.update();
    for frame_delta in frame_deltas {
        app.insert_resource(TimeUpdateStrategy::ManualDuration(*frame_delta));
        app.update();
        if host_has_fault(&app) {
            break;
        }
    }

    finish_replay_host(&mut app)
}

pub fn run_native() -> Result<(), HostError> {
    let package = embedded_empty_package()?;
    let simulation_hz = package.profiles().balance().simulation_hz;
    let physical = package.profiles().physical_scale.clone();
    let laboratory = laboratory::LaboratorySession::new(package)?;
    let viewport = presenter::Viewport::new(cell_buffer::CellPoint::new(-30, -12), 61, 25);
    run_native_laboratory(
        laboratory,
        physical,
        simulation_hz,
        viewport,
        "A/O/N — Empty World",
        (960, 540),
        None,
    )
}

/// Opens the Stage 0 direct-play A/B emergence probe at its ready checkpoint.
///
/// F5 loads the current-input-only control and F6 loads the retained-state design. Each switch
/// creates a fresh paused read-only Replay session at the same ready Tick with SET, Q, Qbar, and
/// STOP probes attached. Space runs the matched input pulse; C enters the selected Mobile's Circuit
/// View.
pub fn run_native_stage0_product_probe() -> Result<(), HostError> {
    let package = embedded_empty_package()?;
    let simulation_hz = package.profiles().balance().simulation_hz;
    let physical = package.profiles().physical_scale.clone();
    let initial_design = Stage0ProductDesign::RetainedState;
    let laboratory = stage0_product_probe_session(package.clone(), initial_design)?;
    let viewport = presenter::Viewport::new(cell_buffer::CellPoint::new(-4, -12), 73, 25);
    run_native_laboratory(
        laboratory,
        physical,
        simulation_hz,
        viewport,
        "A/O/N — Stage 0 A/B Product Probe",
        (1280, 720),
        Some(Stage0ProductProbeControl {
            package,
            active_design: initial_design,
            switch_error: None,
        }),
    )
}

fn run_native_laboratory(
    laboratory: laboratory::LaboratorySession,
    physical: PhysicalScaleProfile,
    simulation_hz: u32,
    viewport: presenter::Viewport,
    title: &str,
    resolution: (u32, u32),
    stage0_product_probe: Option<Stage0ProductProbeControl>,
) -> Result<(), HostError> {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: title.to_owned(),
            resolution: resolution.into(),
            ..default()
        }),
        ..default()
    }));
    configure_host_pacing(&mut app, simulation_hz);
    native_laboratory::install_native_laboratory(&mut app, laboratory, physical.clone(), viewport);
    app.insert_resource(NativePresentationConfig { physical, viewport });
    if let Some(control) = stage0_product_probe {
        app.insert_resource(control);
        app.add_systems(
            PreUpdate,
            switch_stage0_product_design
                .in_set(native_laboratory::NativePreUpdateSet::ProductSwitch),
        );
    }
    native_probe::install_native_probe_renderer(&mut app)?;
    app.add_systems(Update, update_native_probe_document);
    app.run();
    Ok(())
}

fn configure_host_pacing(app: &mut App, simulation_hz: u32) {
    app.insert_resource(Time::<Fixed>::from_hz(f64::from(simulation_hz)));

    // Bevy clamps Virtual time to 250 ms by default. That clamp would silently
    // delete canonical tick debt after a long frame, which A/O/N forbids.
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .set_max_delta(Duration::MAX);
}

fn install_simulation(
    app: &mut App,
    simulation: Simulation,
    mode: HostRunMode,
    presenter_enabled: bool,
) {
    let initial_hash = simulation.state_hash();
    let mut snapshot = RenderSnapshot::default();
    simulation.write_render_snapshot(&mut snapshot);

    app.insert_resource(CanonicalSimulation { simulation });
    app.insert_resource(LatestRenderSnapshot { snapshot });
    app.insert_resource(SimulationHostState { mode });
    app.insert_resource(HostTraceResource {
        checkpoints: vec![initial_hash],
        reports: Vec::new(),
    });
    app.init_resource::<HostFault>();

    if presenter_enabled {
        app.add_plugins(SimulationHostPlugin::with_presenter());
    } else {
        app.add_plugins(SimulationHostPlugin::without_presenter());
    }
}

fn install_replay_simulation(
    app: &mut App,
    simulation: Simulation,
    replay: Replay,
    presenter_enabled: bool,
) {
    install_simulation(app, simulation, HostRunMode::Running, presenter_enabled);
    app.insert_resource(ReplayCommandSchedule { replay });
}

fn advance_canonical_simulation(
    mut canonical: ResMut<CanonicalSimulation>,
    host_state: Res<SimulationHostState>,
    replay_schedule: Option<Res<ReplayCommandSchedule>>,
    mut trace: ResMut<HostTraceResource>,
    mut fault: ResMut<HostFault>,
) {
    if host_state.mode() != HostRunMode::Running || fault.error.is_some() {
        return;
    }

    let next_tick = canonical.simulation.next_tick();
    let commands = if let Some(schedule) = replay_schedule.as_ref() {
        if next_tick >= schedule.replay.final_next_tick() {
            return;
        }
        replay_commands_for_tick(&schedule.replay, next_tick)
    } else {
        &[]
    };
    let world_inputs = replay_schedule
        .as_ref()
        .map(|schedule| {
            schedule
                .replay
                .world_inputs_for_tick(next_tick)
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let step = if replay_schedule.is_some() {
        canonical
            .simulation
            .step_with_world_inputs(commands, &world_inputs)
    } else {
        canonical.simulation.step(commands)
    };
    match step {
        Ok(report) => {
            trace.checkpoints.push(report.state_hash);
            if let Some(schedule) = replay_schedule.as_ref()
                && let Some(expected) = replay_checkpoint(&schedule.replay, report.next_tick)
                && expected != report.state_hash
            {
                fault.error = Some(HostError::Replay(ReplayError::CheckpointDivergence {
                    next_tick: report.next_tick,
                    expected,
                    actual: report.state_hash,
                }));
            }
            trace.reports.push(report);
        }
        Err(error) => fault.error = Some(HostError::Simulation(error)),
    }
}

fn replay_commands_for_tick(replay: &Replay, tick: Tick) -> &[CommandEnvelope] {
    let commands = replay.commands();
    let start = commands.partition_point(|command| command.target_tick < tick);
    let end = commands.partition_point(|command| command.target_tick <= tick);
    &commands[start..end]
}

fn replay_checkpoint(replay: &Replay, next_tick: Tick) -> Option<StateHash> {
    replay
        .checkpoints()
        .binary_search_by_key(&next_tick, |checkpoint| checkpoint.next_tick)
        .ok()
        .map(|index| replay.checkpoints()[index].state_hash)
}

fn refresh_render_snapshot(
    canonical: Res<CanonicalSimulation>,
    mut latest: ResMut<LatestRenderSnapshot>,
) {
    canonical
        .simulation
        .write_render_snapshot(&mut latest.snapshot);
}

fn present_empty_world_title(
    latest: Res<LatestRenderSnapshot>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    let hash = latest.snapshot.state_hash().to_string();
    window.title = format!(
        "A/O/N — Empty World | tick {} | hash {}",
        latest.snapshot.next_tick(),
        &hash[..12]
    );
}

fn switch_stage0_product_design(
    mut inputs: MessageReader<KeyboardInput>,
    mut laboratory: ResMut<native_laboratory::NativeLaboratory>,
    mut actions: ResMut<native_laboratory::NativeHostActionQueue>,
    mut status: ResMut<native_laboratory::NativeLaboratoryStatus>,
    mut editor: ResMut<native_editor::NativeEditorState>,
    mut control: ResMut<Stage0ProductProbeControl>,
) {
    for input in inputs.read() {
        if input.state != ButtonState::Pressed || input.repeat {
            continue;
        }
        let requested = match input.key_code {
            KeyCode::F5 => Stage0ProductDesign::CurrentInputOnly,
            KeyCode::F6 => Stage0ProductDesign::RetainedState,
            _ => continue,
        };
        match stage0_product_probe_session(control.package.clone(), requested) {
            Ok(session) => {
                laboratory.replace_session(session);
                actions.clear();
                *status = native_laboratory::NativeLaboratoryStatus::default();
                editor.clear_transient();
                control.active_design = requested;
                control.switch_error = None;
            }
            Err(error) => control.switch_error = Some(error.to_string()),
        }
    }
}

fn update_native_probe_document(
    laboratory: Res<native_laboratory::NativeLaboratory>,
    native_status: Res<native_laboratory::NativeLaboratoryStatus>,
    editor: Res<native_editor::NativeEditorState>,
    config: Res<NativePresentationConfig>,
    product_probe: Option<Res<Stage0ProductProbeControl>>,
    mut document: ResMut<native_probe::NativeProbeDocument>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let session = laboratory.session();
    let snapshot = session.latest_snapshot();
    let is_product_probe = product_probe.is_some();
    let mode = if session.is_faulted() || native_status.execution_error().is_some() {
        "FAULTED"
    } else {
        match session.pacer().mode() {
            pacing::HostRunMode::Paused => "PAUSED",
            pacing::HostRunMode::Running => "RUNNING",
        }
    };
    let rate = match session.pacer().rate() {
        pacing::HostRate::Quarter => "1/4x",
        pacing::HostRate::One => "1x",
        pacing::HostRate::Four => "4x",
    };
    let hash = snapshot.state_hash().to_string();
    let view = match session.view() {
        presenter::ViewMode::Network => "Network".to_owned(),
        presenter::ViewMode::Circuit { substrate } => format!("Circuit({})", substrate.0),
    };
    let status = if let Some(product_probe) = product_probe.as_ref() {
        cell_buffer::TextPanel::new(
            "Stage 0 A/B Product Probe",
            [
                format!(
                    "design={} {} (switch resets to nextTick 24)",
                    product_probe.active_design.hotkey(),
                    product_probe.active_design.label()
                ),
                format!("nextTick={} {mode} {rate} {view}", snapshot.next_tick().0),
                format!(
                    "hash={} primitives={}",
                    &hash[..16],
                    snapshot.primitive_count()
                ),
                "F5 current-input-only  F6 retained-state".to_owned(),
                "Space run/pause  . step  1/2/3 rate  R raw Tick 0".to_owned(),
                "C/N circuit/network (F5/F6 restore Mobile 4 selection)".to_owned(),
                format!(
                    "lastSteps={} actionErrors={} executionError={} switchError={}",
                    native_status.steps_in_last_pulse(),
                    native_status.action_rejections().len(),
                    if native_status.execution_error().is_some() {
                        "YES"
                    } else {
                        "NO"
                    },
                    product_probe.switch_error.as_deref().unwrap_or("-")
                ),
            ],
        )
    } else {
        cell_buffer::TextPanel::new(
            "A/O/N Laboratory",
            [
                format!("scenario={}", snapshot.scenario_id()),
                format!(
                    "nextTick={} topologyRevision={}",
                    snapshot.next_tick().0,
                    snapshot.topology_revision().0
                ),
                format!(
                    "state={mode} rate={rate} primitives={}",
                    snapshot.primitive_count()
                ),
                format!("hash={}", &hash[..16]),
                format!("view={view}  Space pause/resume  . step  1/2/3 rate  R reset"),
                format!(
                    "cursor=({}, {}) editor={}",
                    editor.cursor().x,
                    editor.cursor().y,
                    editor.feedback()
                ),
                "edit: arrows/numpad move Enter pick A/O/I gate J junction F fixed M mobile W wire Del"
                    .to_owned(),
                "bind: B bind U free  drive: Z/H/X  probe: P add K remove  C/N view  Esc cancel"
                    .to_owned(),
                format!(
                    "lastPulseSteps={} actionRejections={} error={}",
                    native_status.steps_in_last_pulse(),
                    native_status.action_rejections().len(),
                    native_status.execution_error().unwrap_or("-")
                ),
            ],
        )
    };
    let waveform = session
        .probes()
        .waveform_panel(if is_product_probe { 16 } else { 32 });
    let inspector_host_state = if session.is_faulted() || native_status.execution_error().is_some()
    {
        inspector::InspectorHostState::Faulted
    } else {
        match session.pacer().mode() {
            pacing::HostRunMode::Paused => inspector::InspectorHostState::Paused,
            pacing::HostRunMode::Running => inspector::InspectorHostState::Running,
        }
    };
    let inspector_rate = match session.pacer().rate() {
        pacing::HostRate::Quarter => inspector::InspectorRate::Quarter,
        pacing::HostRate::One => inspector::InspectorRate::One,
        pacing::HostRate::Four => inspector::InspectorRate::Four,
    };
    let selection = session
        .selection()
        .map(|target| inspector::InspectorSelection {
            target: match target {
                presenter::PickTarget::Entity(entity) => inspector::InspectorTarget::Entity(entity),
                presenter::PickTarget::GatePort(port) => inspector::InspectorTarget::GatePort(port),
                presenter::PickTarget::MobilePort(port) => {
                    inspector::InspectorTarget::MobilePort(port)
                }
                presenter::PickTarget::WireEnd { wire, end } => {
                    inspector::InspectorTarget::WireEnd { wire, end }
                }
            },
            latest_command: None,
        });
    let selected_arrival = session.reports().iter().rev().find_map(|report| {
        (!report.signal_arrivals.is_empty()).then_some(inspector::ArrivalSelection {
            completed_tick: report.completed_tick,
            observation_index: 0,
        })
    });
    let inspector = if let Some(product_probe) = product_probe.as_ref() {
        stage0_product_probe_inspector(session, product_probe.active_design)
    } else {
        inspector::inspector_panel(inspector::InspectorInput {
            snapshot,
            retained_reports: session.reports(),
            host_state: inspector_host_state,
            rate: inspector_rate,
            selection,
            selected_arrival,
        })
    };

    match presenter::project_snapshot(snapshot, &config.physical, session.view(), config.viewport) {
        Ok(presentation) => {
            let mut buffer = presentation.buffer().clone();
            if let Some(selection) = session.selection() {
                for point in presentation.pick_points(selection) {
                    if let Some(visual) = buffer.visual(point) {
                        buffer.write(cell_buffer::CellWrite::new(
                            point,
                            cell_buffer::CellLayer::Selection,
                            cell_buffer::CellVisual::new(
                                visual.glyph,
                                cell_buffer::CellTone::Highlight,
                                visual.source,
                            ),
                        ));
                    }
                }
            }
            if let Some(anchor) = editor.wire_anchor_cell() {
                buffer.write(cell_buffer::CellWrite::new(
                    anchor,
                    cell_buffer::CellLayer::GhostAndDebug,
                    cell_buffer::CellVisual::new('?', cell_buffer::CellTone::Ghost, None),
                ));
            }
            buffer.write(cell_buffer::CellWrite::new(
                editor.cursor(),
                cell_buffer::CellLayer::GhostAndDebug,
                cell_buffer::CellVisual::new('+', cell_buffer::CellTone::Ghost, None),
            ));
            if is_product_probe {
                document.replace_cell_buffer_with_stacked_panels(
                    &format!("{view} View"),
                    &buffer,
                    &[status, waveform, inspector],
                    2,
                    1,
                );
            } else {
                document.replace_cell_buffer_with_panels(
                    &format!("{view} View"),
                    &buffer,
                    &[status, waveform, inspector],
                    2,
                );
            }
        }
        Err(error) => document.replace(cell_buffer::compose_panels(
            &[
                cell_buffer::TextPanel::new(format!("{view} View"), [format!("error: {error}")]),
                status,
                waveform,
                inspector,
            ],
            2,
        )),
    }

    if let Ok(mut window) = windows.single_mut() {
        window.title = product_probe.as_ref().map_or_else(
            || {
                format!(
                    "A/O/N Laboratory — {mode} {rate} | tick {} | hash {}",
                    snapshot.next_tick(),
                    &hash[..12]
                )
            },
            |product_probe| {
                format!(
                    "A/O/N Stage 0 A/B — {} {} | tick {} | hash {}",
                    product_probe.active_design.hotkey(),
                    product_probe.active_design.label(),
                    snapshot.next_tick(),
                    &hash[..12]
                )
            },
        );
    }
}

fn stage0_product_probe_inspector(
    session: &laboratory::LaboratorySession,
    design: Stage0ProductDesign,
) -> cell_buffer::TextPanel {
    let snapshot = session.latest_snapshot();
    let mobile = snapshot
        .mobiles()
        .iter()
        .find(|mobile| mobile.id == STAGE0_PRODUCT_PROBE_MOBILE);
    let latest_glyph = |target| {
        session
            .probes()
            .traces()
            .find_map(|(_, trace)| {
                (trace.target() == target)
                    .then(|| trace.latest().map(|sample| sample.logic_glyph()))
                    .flatten()
            })
            .unwrap_or('-')
    };
    let stop_target = mobile.map(|record| SignalProbeTarget::Sink(record.ports.stop));
    let stop = stop_target.map_or('-', latest_glyph);
    let (world, controls) = mobile.map_or_else(
        || ("removed".to_owned(), "-/-/-".to_owned()),
        |record| {
            (
                format!(
                    "({}, {})",
                    record.world_position.x.0, record.world_position.y.0
                ),
                format!(
                    "{}/{}/{}",
                    logic_level_glyph(record.stop),
                    logic_level_glyph(record.left),
                    logic_level_glyph(record.right)
                ),
            )
        },
    );
    let (architecture, outcome) = match design {
        Stage0ProductDesign::CurrentInputOnly => (
            "feed-forward control: no retained circuit state",
            "nextTick 162: SET=0 Q=0 STOP=0; Mobile resumes",
        ),
        Stage0ProductDesign::RetainedState => (
            "feedback design: circuit state survives release",
            "nextTick 98..162: SET=0 Q=1 STOP=1; held",
        ),
    };
    cell_buffer::TextPanel::new(
        "A/B Emergence Inspector",
        [
            format!("active={} {}", design.hotkey(), design.label()),
            format!(
                "SET={} Q={} Qbar={} STOP={stop}",
                latest_glyph(SignalProbeTarget::Driver(STAGE0_PRODUCT_PROBE_SET)),
                latest_glyph(SignalProbeTarget::GateOutput(STAGE0_PRODUCT_PROBE_Q)),
                latest_glyph(SignalProbeTarget::GateOutput(STAGE0_PRODUCT_PROBE_QBAR)),
            ),
            "matched SET: LOW through 70; HIGH 71..97; LOW 98+".to_owned(),
            "nextTick 81: both designs Q=1 Qbar=0 STOP=1".to_owned(),
            architecture.to_owned(),
            outcome.to_owned(),
            format!("Mobile 4 world={world}"),
            format!("Mobile controls STOP/LEFT/RIGHT={controls}"),
            "F5/F6 compare from Tick 24; C Circuit; N Network".to_owned(),
        ],
    )
}

const fn logic_level_glyph(level: LogicLevel) -> char {
    match level {
        LogicLevel::Low => '0',
        LogicLevel::High => '1',
        LogicLevel::X => 'X',
    }
}

fn run_presentation_updates(app: &mut App, count: u32) {
    for _ in 0..count {
        app.world_mut().run_schedule(Update);
    }
}

fn take_host_fault(app: &mut App) -> Option<HostError> {
    app.world_mut().resource_mut::<HostFault>().error.take()
}

fn host_has_fault(app: &App) -> bool {
    app.world().resource::<HostFault>().error.is_some()
}

fn finish_replay_host(app: &mut App) -> Result<HostTrace, HostError> {
    if let Some(error) = take_host_fault(app) {
        return Err(error);
    }

    let trace = HostTrace {
        checkpoints: app
            .world()
            .resource::<HostTraceResource>()
            .checkpoints
            .clone(),
        reports: app.world().resource::<HostTraceResource>().reports.clone(),
    };
    app.world()
        .resource::<ReplayCommandSchedule>()
        .replay
        .verify_trace(trace.checkpoints())?;
    Ok(trace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::keyboard::{Key, NativeKey};

    const PRODUCT_DESIGNS: [Stage0ProductDesign; 2] = [
        Stage0ProductDesign::CurrentInputOnly,
        Stage0ProductDesign::RetainedState,
    ];

    fn advance_product_probe_to(laboratory: &mut laboratory::LaboratorySession, next_tick: u64) {
        while laboratory.next_tick() < Tick(next_tick) {
            laboratory
                .step_once()
                .expect("critical Replay Tick advances");
        }
        assert_eq!(laboratory.next_tick(), Tick(next_tick));
    }

    fn product_probe_level(
        laboratory: &laboratory::LaboratorySession,
        target: SignalProbeTarget,
    ) -> LogicLevel {
        laboratory
            .probes()
            .traces()
            .find_map(|(_, trace)| {
                (trace.target() == target)
                    .then(|| trace.latest().map(|sample| sample.logic_level()))
                    .flatten()
            })
            .expect("the product probe target has a latest sample")
    }

    #[test]
    fn both_stage0_product_designs_open_at_the_matched_ready_checkpoint() {
        for design in PRODUCT_DESIGNS {
            let package = embedded_empty_package().expect("embedded fixtures are valid");
            let laboratory = stage0_product_probe_session(package, design)
                .expect("product probe fixture is executable");

            assert_eq!(
                laboratory.session_mode(),
                laboratory::LaboratorySessionMode::ReplayPlayback
            );
            assert_eq!(laboratory.next_tick(), STAGE0_PRODUCT_PROBE_READY_TICK);
            assert_eq!(laboratory.replay_final_next_tick(), Some(Tick(162)));
            assert_eq!(
                laboratory.selection(),
                Some(presenter::PickTarget::Entity(
                    STAGE0_PRODUCT_PROBE_MOBILE.entity_id()
                ))
            );

            let snapshot = laboratory.latest_snapshot();
            let mobile = snapshot
                .mobiles()
                .iter()
                .find(|mobile| mobile.id == STAGE0_PRODUCT_PROBE_MOBILE)
                .expect("ready checkpoint contains the Mobile");
            assert_eq!(mobile.stop, LogicLevel::Low);
            assert_eq!(
                product_probe_level(
                    &laboratory,
                    SignalProbeTarget::GateOutput(STAGE0_PRODUCT_PROBE_Q)
                ),
                LogicLevel::Low
            );
            assert_eq!(
                product_probe_level(
                    &laboratory,
                    SignalProbeTarget::GateOutput(STAGE0_PRODUCT_PROBE_QBAR)
                ),
                LogicLevel::High
            );

            let targets = laboratory
                .probes()
                .traces()
                .map(|(_, trace)| trace.target())
                .collect::<Vec<_>>();
            assert_eq!(
                targets,
                vec![
                    SignalProbeTarget::Driver(STAGE0_PRODUCT_PROBE_SET),
                    SignalProbeTarget::GateOutput(STAGE0_PRODUCT_PROBE_Q),
                    SignalProbeTarget::GateOutput(STAGE0_PRODUCT_PROBE_QBAR),
                    SignalProbeTarget::Sink(mobile.ports.stop),
                ]
            );
            assert!(laboratory.probes().traces().all(|(_, trace)| {
                trace.history().len() == 19
                    && trace
                        .latest()
                        .is_some_and(|sample| sample.next_tick == STAGE0_PRODUCT_PROBE_READY_TICK)
            }));
        }
    }

    #[test]
    fn stage0_ab_designs_share_the_exact_set_timeline_but_diverge_after_release() {
        let expected_set = [
            (24, LogicLevel::Low),
            (70, LogicLevel::Low),
            (71, LogicLevel::High),
            (81, LogicLevel::High),
            (97, LogicLevel::High),
            (98, LogicLevel::Low),
            (162, LogicLevel::Low),
        ];
        let mut timelines = Vec::new();

        for design in PRODUCT_DESIGNS {
            let package = embedded_empty_package().expect("embedded fixtures are valid");
            let mut laboratory = stage0_product_probe_session(package, design)
                .expect("product probe fixture is executable");
            let stop = laboratory
                .latest_snapshot()
                .mobiles()
                .iter()
                .find(|mobile| mobile.id == STAGE0_PRODUCT_PROBE_MOBILE)
                .expect("ready checkpoint contains the Mobile")
                .ports
                .stop;
            let mut timeline = Vec::new();
            for (next_tick, expected) in expected_set {
                advance_product_probe_to(&mut laboratory, next_tick);
                let actual = product_probe_level(
                    &laboratory,
                    SignalProbeTarget::Driver(STAGE0_PRODUCT_PROBE_SET),
                );
                assert_eq!(actual, expected, "{} at Tick {next_tick}", design.label());
                timeline.push(actual);
            }
            let final_q = product_probe_level(
                &laboratory,
                SignalProbeTarget::GateOutput(STAGE0_PRODUCT_PROBE_Q),
            );
            let final_stop = product_probe_level(&laboratory, SignalProbeTarget::Sink(stop));
            match design {
                Stage0ProductDesign::CurrentInputOnly => {
                    assert_eq!((final_q, final_stop), (LogicLevel::Low, LogicLevel::Low));
                }
                Stage0ProductDesign::RetainedState => {
                    assert_eq!((final_q, final_stop), (LogicLevel::High, LogicLevel::High));
                }
            }
            timelines.push(timeline);
        }
        assert_eq!(timelines[0], timelines[1]);
    }

    #[test]
    fn both_stage0_ab_documents_fit_and_label_the_default_window_at_critical_ticks() {
        let viewport = presenter::Viewport::new(cell_buffer::CellPoint::new(-4, -12), 73, 25);
        let metrics = native_probe::embedded_font_metrics().expect("embedded font metrics");
        for design in PRODUCT_DESIGNS {
            let critical = match design {
                Stage0ProductDesign::CurrentInputOnly => [
                    (24, "SET=0 Q=0 Qbar=1 STOP=0"),
                    (71, "SET=1 Q=0 Qbar=1 STOP=0"),
                    (81, "SET=1 Q=1 Qbar=0 STOP=1"),
                    (98, "SET=0 Q=1 Qbar=0 STOP=1"),
                    (162, "SET=0 Q=0 Qbar=1 STOP=0"),
                ],
                Stage0ProductDesign::RetainedState => [
                    (24, "SET=0 Q=0 Qbar=1 STOP=0"),
                    (71, "SET=1 Q=0 Qbar=1 STOP=0"),
                    (81, "SET=1 Q=1 Qbar=0 STOP=1"),
                    (98, "SET=0 Q=1 Qbar=0 STOP=1"),
                    (162, "SET=0 Q=1 Qbar=0 STOP=1"),
                ],
            };
            for (next_tick, levels) in critical {
                let package = embedded_empty_package().expect("embedded fixtures are valid");
                let physical = package.profiles().physical_scale.clone();
                let mut laboratory = stage0_product_probe_session(package.clone(), design)
                    .expect("product probe fixture is executable");
                advance_product_probe_to(&mut laboratory, next_tick);
                let mut app = App::new();
                app.add_plugins(MinimalPlugins);
                app.insert_resource(native_laboratory::NativeLaboratory::new(laboratory));
                app.insert_resource(native_laboratory::NativeLaboratoryStatus::default());
                app.insert_resource(native_editor::NativeEditorState::new(
                    physical.clone(),
                    viewport,
                ));
                app.insert_resource(NativePresentationConfig { physical, viewport });
                app.insert_resource(Stage0ProductProbeControl {
                    package,
                    active_design: design,
                    switch_error: None,
                });
                app.init_resource::<native_probe::NativeProbeDocument>();
                app.world_mut().spawn((Window::default(), PrimaryWindow));
                app.add_systems(Update, update_native_probe_document);
                app.world_mut().run_schedule(Update);

                let document = app.world().resource::<native_probe::NativeProbeDocument>();
                let rendered_width =
                    24.0 + document.column_count() as f32 * metrics.cell_advance_px();
                let rendered_height = 24.0 + document.row_count() as f32 * metrics.line_height_px();
                assert!(
                    rendered_width <= 1280.0,
                    "{} Tick {next_tick} width {rendered_width} exceeds the window",
                    design.label()
                );
                assert!(
                    rendered_height <= 720.0,
                    "{} Tick {next_tick} height {rendered_height} exceeds the window",
                    design.label()
                );
                assert!(document.text().contains("[Waveform last 16"));
                assert!(document.text().contains("[A/B Emergence Inspector"));
                assert!(document.text().contains("F5 current-input-only"));
                assert!(document.text().contains("F6 retained-state"));
                assert!(document.text().contains(design.label()));
                for label in ["P0 driver:7", "P1 gate:6:out", "P2 gate:8:out", "P3 sink:"] {
                    assert!(document.text().contains(label));
                }
                assert!(document.text().contains(levels));
            }
        }
    }

    #[test]
    fn f5_and_f6_replace_the_session_and_reset_to_the_matched_ready_tick() {
        let package = embedded_empty_package().expect("embedded fixtures are valid");
        let physical = package.profiles().physical_scale.clone();
        let viewport = presenter::Viewport::new(cell_buffer::CellPoint::new(-4, -12), 73, 25);
        let mut laboratory =
            stage0_product_probe_session(package.clone(), Stage0ProductDesign::RetainedState)
                .expect("retained product probe is executable");
        advance_product_probe_to(&mut laboratory, 81);
        let mut dirty_editor = native_editor::NativeEditorState::new(physical.clone(), viewport);
        dirty_editor
            .apply_control(&laboratory, native_editor::NativeEditorControl::WireAnchor)
            .expect("test editor creates a transient wire anchor");
        assert!(dirty_editor.wire_anchor_cell().is_some());

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        native_laboratory::install_native_laboratory(&mut app, laboratory, physical, viewport);
        app.insert_resource(dirty_editor);
        app.insert_resource(Stage0ProductProbeControl {
            package,
            active_design: Stage0ProductDesign::RetainedState,
            switch_error: None,
        });
        app.add_systems(
            PreUpdate,
            switch_stage0_product_design
                .in_set(native_laboratory::NativePreUpdateSet::ProductSwitch),
        );
        let window = app.world_mut().spawn_empty().id();
        {
            let mut actions = app
                .world_mut()
                .resource_mut::<native_laboratory::NativeHostActionQueue>();
            actions.push(host_action::HostAction::SetView(
                presenter::ViewMode::Circuit {
                    substrate: STAGE0_PRODUCT_PROBE_MOBILE.entity_id(),
                },
            ));
            actions.push(host_action::HostAction::ClearSelection);
            actions.push(host_action::HostAction::Resume);
            actions.push(host_action::HostAction::SingleStep);
        }
        app.world_mut().run_schedule(FixedUpdate);
        {
            let laboratory = app
                .world()
                .resource::<native_laboratory::NativeLaboratory>();
            assert_eq!(
                laboratory.session().pacer().mode(),
                pacing::HostRunMode::Running
            );
            assert_eq!(
                laboratory.session().view(),
                presenter::ViewMode::Circuit {
                    substrate: STAGE0_PRODUCT_PROBE_MOBILE.entity_id()
                }
            );
            assert_eq!(laboratory.session().selection(), None);
        }
        assert_eq!(
            app.world()
                .resource::<native_laboratory::NativeLaboratoryStatus>()
                .action_rejections()
                .len(),
            1
        );

        for (key_code, expected_design) in [
            (KeyCode::F6, Stage0ProductDesign::RetainedState),
            (KeyCode::F5, Stage0ProductDesign::CurrentInputOnly),
        ] {
            for same_frame_key in [key_code, KeyCode::Space, KeyCode::Period] {
                app.world_mut().write_message(KeyboardInput {
                    key_code: same_frame_key,
                    logical_key: Key::Unidentified(NativeKey::Unidentified),
                    state: ButtonState::Pressed,
                    text: None,
                    repeat: false,
                    window,
                });
            }
            app.world_mut().run_schedule(PreUpdate);

            let control = app.world().resource::<Stage0ProductProbeControl>();
            let laboratory = app
                .world()
                .resource::<native_laboratory::NativeLaboratory>();
            assert_eq!(control.active_design, expected_design);
            assert_eq!(control.switch_error, None);
            assert_eq!(
                laboratory.session().pacer().mode(),
                pacing::HostRunMode::Paused
            );
            assert_eq!(laboratory.session().pacer().rate(), pacing::HostRate::One);
            assert_eq!(
                laboratory.session().next_tick(),
                STAGE0_PRODUCT_PROBE_READY_TICK
            );
            assert_eq!(laboratory.session().view(), presenter::ViewMode::Network);
            assert_eq!(
                laboratory.session().selection(),
                Some(presenter::PickTarget::Entity(
                    STAGE0_PRODUCT_PROBE_MOBILE.entity_id()
                ))
            );
            assert_eq!(laboratory.session().probes().traces().count(), 4);
            assert!(laboratory.session().probes().traces().all(|(_, trace)| {
                trace.history().len() == 19
                    && trace
                        .latest()
                        .is_some_and(|sample| sample.next_tick == STAGE0_PRODUCT_PROBE_READY_TICK)
            }));
            assert!(
                app.world()
                    .resource::<native_laboratory::NativeHostActionQueue>()
                    .queued()
                    .is_empty()
            );
            let status = app
                .world()
                .resource::<native_laboratory::NativeLaboratoryStatus>();
            assert!(status.action_rejections().is_empty());
            assert_eq!(status.execution_error(), None);
            assert_eq!(status.steps_in_last_pulse(), 0);
            let editor = app.world().resource::<native_editor::NativeEditorState>();
            assert_eq!(editor.wire_anchor_cell(), None);
            assert_eq!(editor.feedback(), "session reset");
        }
    }

    #[test]
    fn presenter_projects_empty_snapshot_into_the_primary_window_title() {
        let package = embedded_empty_package().expect("embedded fixtures are valid");
        let simulation = Simulation::new(package).expect("bootstrap simulation is valid");
        let simulation_hz = simulation.profiles().balance().simulation_hz;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        configure_host_pacing(&mut app, simulation_hz);
        install_simulation(&mut app, simulation, HostRunMode::Running, true);
        let window_entity = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();

        app.world_mut().run_schedule(Update);

        assert_primary_window_title(&app, window_entity, 0);

        app.world_mut().run_schedule(FixedUpdate);
        app.world_mut().run_schedule(Update);

        assert_primary_window_title(&app, window_entity, 1);
    }

    fn assert_primary_window_title(app: &App, window_entity: Entity, expected_tick: u64) {
        let snapshot = app.world().resource::<LatestRenderSnapshot>().get();
        assert_eq!(snapshot.next_tick(), Tick(expected_tick));
        let hash = snapshot.state_hash().to_string();
        let expected_title = format!(
            "A/O/N — Empty World | tick {} | hash {}",
            snapshot.next_tick(),
            &hash[..12]
        );
        let window = app
            .world()
            .entity(window_entity)
            .get::<Window>()
            .expect("the synthetic primary window exists");
        assert_eq!(window.title, expected_title);
    }
}
