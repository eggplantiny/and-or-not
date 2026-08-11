#![forbid(unsafe_code)]

use aon_sim::{
    ArtifactBytes, PackageError, RenderSnapshot, Simulation, SimulationError, SimulationPackage,
    StateHash, decode_package,
};
use bevy::prelude::*;
use bevy::time::{TimeUpdateStrategy, Virtual};
use bevy::window::{PrimaryWindow, WindowPlugin};
use std::time::Duration;
use thiserror::Error;

const SIMULATION_TICK_DURATION: Duration = Duration::from_millis(50);

const EMPTY_SCENARIO: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/scenarios/empty.json"
));
const EMPTY_NUMERIC_PROFILE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/numeric/bootstrap-empty-v1.json"
));
const EMPTY_PHYSICAL_SCALE_PROFILE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/physical-scale/bootstrap-empty-v1.json"
));
const EMPTY_BALANCE_PROFILE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/balance/bootstrap-empty-v1.json"
));

#[derive(Debug, Error)]
pub enum HostError {
    #[error(transparent)]
    Package(#[from] PackageError),

    #[error(transparent)]
    Simulation(#[from] SimulationError),
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

    pub const fn next_tick(&self) -> u64 {
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

#[derive(Resource, Default)]
struct HostFault {
    error: Option<SimulationError>,
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
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    configure_host_pacing(&mut app);
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

    if let Some(error) = app.world_mut().resource_mut::<HostFault>().error.take() {
        return Err(HostError::Simulation(error));
    }

    let checkpoints = app
        .world()
        .resource::<HostTraceResource>()
        .checkpoints
        .clone();
    Ok(HostTrace { checkpoints })
}

pub fn run_paced_host_harness(
    package: SimulationPackage,
    frame_deltas: &[Duration],
) -> Result<HostTrace, HostError> {
    let simulation = Simulation::new(package)?;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    configure_host_pacing(&mut app);
    install_simulation(&mut app, simulation, HostRunMode::Running, true);

    // Bevy's Real clock uses its first update to establish an origin and
    // intentionally reports a zero delta for that update.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    app.update();
    for frame_delta in frame_deltas {
        app.insert_resource(TimeUpdateStrategy::ManualDuration(*frame_delta));
        app.update();
    }

    if let Some(error) = app.world_mut().resource_mut::<HostFault>().error.take() {
        return Err(HostError::Simulation(error));
    }

    let checkpoints = app
        .world()
        .resource::<HostTraceResource>()
        .checkpoints
        .clone();
    Ok(HostTrace { checkpoints })
}

pub fn run_native() -> Result<(), HostError> {
    let package = embedded_empty_package()?;
    let simulation = Simulation::new(package)?;
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "A/O/N — Empty World".to_owned(),
            resolution: (960, 540).into(),
            ..default()
        }),
        ..default()
    }));
    configure_host_pacing(&mut app);
    install_simulation(&mut app, simulation, HostRunMode::Running, true);
    app.run();
    Ok(())
}

fn configure_host_pacing(app: &mut App) {
    app.insert_resource(Time::<Fixed>::from_duration(SIMULATION_TICK_DURATION));

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

fn advance_canonical_simulation(
    mut canonical: ResMut<CanonicalSimulation>,
    host_state: Res<SimulationHostState>,
    mut trace: ResMut<HostTraceResource>,
    mut fault: ResMut<HostFault>,
) {
    if host_state.mode() != HostRunMode::Running || fault.error.is_some() {
        return;
    }

    match canonical.simulation.step(&[]) {
        Ok(report) => trace.checkpoints.push(report.state_hash),
        Err(error) => fault.error = Some(error),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presenter_projects_empty_snapshot_into_the_primary_window_title() {
        let package = embedded_empty_package().expect("embedded fixtures are valid");
        let simulation = Simulation::new(package).expect("bootstrap simulation is valid");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        configure_host_pacing(&mut app);
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
        assert_eq!(snapshot.next_tick(), expected_tick);
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
