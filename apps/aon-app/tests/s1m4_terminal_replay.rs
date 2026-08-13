use aon_app::laboratory::{
    LaboratoryError, LaboratoryFault, LaboratorySession, LaboratorySessionMode,
};
use aon_app::{HostError, run_replay_host_harness};
use aon_sim::{
    BalanceProfile, Command, CommandEnvelope, EnemyInitialState, EntityId, Fixed, FixedVec2,
    HashCheckpoint, HeatEnergy, InitialWorld, Integrity, NumericProfile, PhysicalScaleProfile,
    ProfileBundle, RemoveEntityCommand, Replay, ReplayError, RunEndCause, RunStatus, Simulation,
    SimulationContract, SimulationError, SimulationPackage, StageFeatureSet, StateHash, Tick,
    WorldInputEvent,
};

fn fatal_package() -> SimulationPackage {
    let mut profiles = ProfileBundle {
        numeric: NumericProfile::reference_v1("numeric-s1m4-terminal-app"),
        physical_scale: PhysicalScaleProfile::stage0_alpha("physical-s1m4-terminal-app"),
        balance: BalanceProfile::construction_contact_damage_alpha("balance-s1m4-terminal-app"),
    };
    profiles
        .balance
        .contact_damage_probe
        .as_mut()
        .expect("Balance v5 contact/damage probe exists")
        .enemy_attack_energy_per_tick = 100;
    let contract = SimulationContract::from_profiles(&profiles).expect("profiles are valid");

    SimulationPackage::new(
        "s1m4-terminal-app",
        InitialWorld::MainCorePowerEnemyV1 {
            main_core_position: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
            main_core_integrity: Integrity(100),
            main_core_heat_energy: HeatEnergy(0),
            power_sources: Vec::new(),
            enemies: vec![EnemyInitialState::new(
                FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                Fixed(aon_sim::FIXED_ONE),
                Integrity(10),
                HeatEnergy(0),
            )],
        },
        StageFeatureSet {
            signal: true,
            mobility: true,
            capacity: true,
            sensing: true,
            power: true,
            construction: true,
            contact: true,
            damage: true,
            ..StageFeatureSet::none()
        },
        contract,
        profiles,
    )
}

fn terminal_replay(
    package: &SimulationPackage,
    final_next_tick: Tick,
    commands: Vec<CommandEnvelope>,
    world_inputs: Vec<WorldInputEvent>,
) -> Replay {
    terminal_replay_with_checkpoint(package, final_next_tick, commands, world_inputs, None)
}

fn terminal_replay_with_checkpoint(
    package: &SimulationPackage,
    final_next_tick: Tick,
    commands: Vec<CommandEnvelope>,
    world_inputs: Vec<WorldInputEvent>,
    terminal_checkpoint: Option<StateHash>,
) -> Replay {
    let mut recorder = Simulation::new(package.clone()).expect("fatal fixture starts");
    let header = recorder.replay_header();
    let initial_hash = recorder.state_hash();
    let terminal = recorder.step(&[]).expect("fatal Tick completes");
    assert!(matches!(terminal.run_status, RunStatus::Ended { .. }));

    let mut checkpoints = vec![
        HashCheckpoint {
            next_tick: Tick(0),
            state_hash: initial_hash,
        },
        HashCheckpoint {
            next_tick: Tick(1),
            state_hash: terminal_checkpoint.unwrap_or(terminal.state_hash),
        },
    ];
    if final_next_tick > Tick(1) {
        checkpoints.push(HashCheckpoint {
            next_tick: final_next_tick,
            state_hash: terminal.state_hash,
        });
    }
    Replay::new_v2(header, commands, world_inputs, checkpoints).expect("Replay shape is valid")
}

#[test]
fn bevy_and_laboratory_verify_the_terminal_checkpoint_before_the_later_boundary() {
    let package = fatal_package();
    let divergent_hash = StateHash::from_hex(&"0".repeat(64)).expect("zero hash is canonical");
    let replay = terminal_replay_with_checkpoint(
        &package,
        Tick(2),
        Vec::new(),
        Vec::new(),
        Some(divergent_hash),
    );

    let bevy_error = run_replay_host_harness(package.clone(), replay.clone(), 0, false)
        .expect_err("Bevy verifies the terminal checkpoint before the later boundary");
    assert!(matches!(
        bevy_error,
        HostError::Replay(ReplayError::CheckpointDivergence {
            next_tick: Tick(1),
            expected,
            actual,
        }) if expected == divergent_hash && actual != divergent_hash
    ));

    let mut laboratory = LaboratorySession::from_replay(package, replay)
        .expect("Laboratory Replay validates at Tick 0");
    let error = laboratory
        .step_once()
        .expect_err("Laboratory verifies the terminal checkpoint before the later boundary");
    assert!(matches!(
        error,
        LaboratoryError::Fatal(LaboratoryFault::Replay(
            ReplayError::CheckpointDivergence {
                next_tick: Tick(1),
                expected,
                actual,
            }
        )) if expected == divergent_hash && actual != divergent_hash
    ));
    assert_eq!(laboratory.next_tick(), Tick(1));
    assert_eq!(laboratory.reports().len(), 1);
    assert_terminal_report(&laboratory.reports()[0]);
}

fn assert_terminal_report(report: &aon_sim::StepReport) {
    assert_eq!(
        report.run_status,
        RunStatus::Ended {
            completed_tick: Tick(0),
            cause: RunEndCause::MainCoreDestroyed,
        }
    );
    assert_eq!(report.next_tick, Tick(1));
}

#[test]
fn bevy_and_laboratory_accept_an_exact_terminal_boundary() {
    let package = fatal_package();
    let replay = terminal_replay(
        &package,
        Tick(1),
        Vec::new(),
        vec![WorldInputEvent::HostileFrame {
            target_tick: Tick(0),
            hostiles: Vec::new(),
        }],
    );

    let bevy = run_replay_host_harness(package.clone(), replay.clone(), 3, true)
        .expect("Bevy accepts finalNextTick equal to terminal");
    assert_eq!(bevy.reports().len(), 1);
    assert_eq!(bevy.checkpoints().len(), 2);
    assert_terminal_report(&bevy.reports()[0]);
    assert_eq!(bevy.final_hash(), bevy.reports()[0].state_hash);

    let mut laboratory = LaboratorySession::from_replay(package, replay)
        .expect("Laboratory Replay validates at Tick 0");
    assert_eq!(
        laboratory.session_mode(),
        LaboratorySessionMode::ReplayPlayback
    );
    let terminal = laboratory
        .step_once()
        .expect("Laboratory accepts finalNextTick equal to terminal");
    assert_terminal_report(&terminal);
    assert_eq!(laboratory.reports(), std::slice::from_ref(&terminal));
    assert_eq!(laboratory.hash_trace().len(), 2);
    assert_eq!(laboratory.state_hash(), terminal.state_hash);
    assert_eq!(laboratory.step_once(), Err(LaboratoryError::ReplayComplete));
}

#[test]
fn bevy_and_laboratory_report_the_same_typed_later_boundary_error() {
    let package = fatal_package();
    let replay = terminal_replay(
        &package,
        Tick(2),
        vec![CommandEnvelope {
            target_tick: Tick(1),
            ordinal: 0,
            command: Command::RemoveEntity(RemoveEntityCommand {
                target: EntityId(u64::MAX),
            }),
        }],
        vec![WorldInputEvent::HostileFrame {
            target_tick: Tick(1),
            hostiles: Vec::new(),
        }],
    );
    let expected = ReplayError::RunEndedBeforeReplayBoundary {
        terminal_next_tick: Tick(1),
        requested_next_tick: Tick(2),
    };

    let bevy_error = run_replay_host_harness(package.clone(), replay.clone(), 0, false)
        .expect_err("Bevy rejects a Replay boundary beyond terminal");
    assert!(matches!(
        bevy_error,
        HostError::Replay(error) if error == expected
    ));

    let terminal_hash = replay.checkpoints()[1].state_hash;
    let mut laboratory = LaboratorySession::from_replay(package, replay)
        .expect("Laboratory Replay validates at Tick 0");
    let error = laboratory
        .step_once()
        .expect_err("Laboratory rejects a Replay boundary beyond terminal");
    assert!(matches!(
        error,
        LaboratoryError::Fatal(LaboratoryFault::Replay(error)) if error == expected
    ));

    assert_eq!(laboratory.next_tick(), Tick(1));
    assert_eq!(laboratory.state_hash(), terminal_hash);
    assert_eq!(laboratory.hash_trace().len(), 2);
    assert_eq!(laboratory.reports().len(), 1);
    assert_terminal_report(&laboratory.reports()[0]);
    assert_eq!(
        laboratory.latest_snapshot().run_status(),
        RunStatus::Ended {
            completed_tick: Tick(0),
            cause: RunEndCause::MainCoreDestroyed,
        }
    );

    let before_tick = laboratory.next_tick();
    let before_hash = laboratory.state_hash();
    let before_report_count = laboratory.reports().len();
    assert!(matches!(
        laboratory.step_once(),
        Err(LaboratoryError::SessionFaulted {
            fault: LaboratoryFault::Replay(error),
        }) if error == expected
    ));
    assert_eq!(laboratory.next_tick(), before_tick);
    assert_eq!(laboratory.state_hash(), before_hash);
    assert_eq!(laboratory.reports().len(), before_report_count);
}

#[test]
fn interactive_laboratory_keeps_simulation_run_ended_semantics() {
    let mut laboratory = LaboratorySession::new(fatal_package()).expect("interactive world starts");
    let terminal = laboratory.step_once().expect("fatal Tick completes");
    assert_terminal_report(&terminal);

    let before_tick = laboratory.next_tick();
    let before_hash = laboratory.state_hash();
    let before_report_count = laboratory.reports().len();
    assert_eq!(
        laboratory.step_once(),
        Err(LaboratoryError::Fatal(LaboratoryFault::Simulation(
            SimulationError::RunEnded,
        )))
    );
    assert_eq!(laboratory.next_tick(), before_tick);
    assert_eq!(laboratory.state_hash(), before_hash);
    assert_eq!(laboratory.reports().len(), before_report_count);
}
