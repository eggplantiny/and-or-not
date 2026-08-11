#![forbid(unsafe_code)]

use aon_sim::{
    ArtifactBytes, CommandEnvelope, PackageError, RenderSnapshot, Replay, ReplayError, Simulation,
    SimulationError, SimulationPackage, StateHash, Tick, decode_package,
};
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

#[derive(Debug, Error)]
pub enum HostError {
    #[error(transparent)]
    Package(#[from] PackageError),

    #[error(transparent)]
    Simulation(#[from] SimulationError),

    #[error(transparent)]
    Replay(#[from] ReplayError),
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
}

#[derive(Resource)]
struct ReplayCommandSchedule {
    replay: Replay,
}

#[derive(Resource, Default)]
struct HostFault {
    error: Option<HostError>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostTrace {
    checkpoints: Vec<StateHash>,
}

impl HostTrace {
    pub fn checkpoints(&self) -> &[StateHash] {
        &self.checkpoints
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

    let checkpoints = app
        .world()
        .resource::<HostTraceResource>()
        .checkpoints
        .clone();
    Ok(HostTrace { checkpoints })
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

    let checkpoints = app
        .world()
        .resource::<HostTraceResource>()
        .checkpoints
        .clone();
    Ok(HostTrace { checkpoints })
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
    let simulation = Simulation::new(package)?;
    let simulation_hz = simulation.profiles().balance().simulation_hz;
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "A/O/N — Empty World".to_owned(),
            resolution: (960, 540).into(),
            ..default()
        }),
        ..default()
    }));
    configure_host_pacing(&mut app, simulation_hz);
    install_simulation(&mut app, simulation, HostRunMode::Running, true);
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

    match canonical.simulation.step(commands) {
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
