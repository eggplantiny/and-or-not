use aon_headless::{HeadlessError, run_replay};
use aon_sim::{
    BalanceProfile, Command, CommandEnvelope, EnemyInitialState, EntityId, Fixed, FixedVec2,
    HashCheckpoint, HeatEnergy, InitialWorld, Integrity, NumericProfile, PhysicalScaleProfile,
    ProfileBundle, RemoveEntityCommand, Replay, ReplayError, RunEndCause, RunStatus, Simulation,
    SimulationContract, SimulationPackage, StageFeatureSet, StateHash, Tick, WorldInputEvent,
};

fn fatal_package() -> SimulationPackage {
    let mut profiles = ProfileBundle {
        numeric: NumericProfile::reference_v1("numeric-s1m4-terminal-headless"),
        physical_scale: PhysicalScaleProfile::stage0_alpha("physical-s1m4-terminal-headless"),
        balance: BalanceProfile::construction_contact_damage_alpha(
            "balance-s1m4-terminal-headless",
        ),
    };
    profiles
        .balance
        .contact_damage_probe
        .as_mut()
        .expect("Balance v5 contact/damage probe exists")
        .enemy_attack_energy_per_tick = 100;
    let contract = SimulationContract::from_profiles(&profiles).expect("profiles are valid");

    SimulationPackage::new(
        "s1m4-terminal-headless",
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
    assert_eq!(
        terminal.run_status,
        RunStatus::Ended {
            completed_tick: Tick(0),
            cause: RunEndCause::MainCoreDestroyed,
        }
    );

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
fn terminal_checkpoint_divergence_precedes_a_later_boundary_error() {
    let package = fatal_package();
    let divergent_hash = StateHash::from_hex(&"0".repeat(64)).expect("zero hash is canonical");
    let replay = terminal_replay_with_checkpoint(
        &package,
        Tick(2),
        Vec::new(),
        Vec::new(),
        Some(divergent_hash),
    );

    let error = run_replay(package, &replay)
        .expect_err("terminal checkpoint must be verified before the later boundary");
    assert!(matches!(
        error,
        HeadlessError::Replay(ReplayError::CheckpointDivergence {
            next_tick: Tick(1),
            expected,
            actual,
        }) if expected == divergent_hash && actual != divergent_hash
    ));
}

#[test]
fn exact_terminal_boundary_returns_the_fatal_report_and_hash() {
    let package = fatal_package();
    let replay = terminal_replay(&package, Tick(1), Vec::new(), Vec::new());

    let trace = run_replay(package, &replay).expect("finalNextTick equal to terminal is valid");
    assert_eq!(trace.completed_ticks(), 1);
    assert_eq!(trace.reports().len(), 1);
    assert_eq!(trace.checkpoints().len(), 2);
    assert_eq!(trace.final_hash(), trace.reports()[0].state_hash);
    assert_eq!(
        trace.reports()[0].run_status,
        RunStatus::Ended {
            completed_tick: Tick(0),
            cause: RunEndCause::MainCoreDestroyed,
        }
    );
}

#[test]
fn later_checkpoint_command_or_world_input_uses_the_same_typed_boundary_error() {
    let package = fatal_package();
    let cases = [
        (Vec::new(), Vec::new()),
        (
            vec![CommandEnvelope {
                target_tick: Tick(1),
                ordinal: 0,
                command: Command::RemoveEntity(RemoveEntityCommand {
                    target: EntityId(u64::MAX),
                }),
            }],
            Vec::new(),
        ),
        (
            Vec::new(),
            vec![WorldInputEvent::HostileFrame {
                target_tick: Tick(1),
                hostiles: Vec::new(),
            }],
        ),
    ];

    for (commands, world_inputs) in cases {
        let replay = terminal_replay(&package, Tick(2), commands, world_inputs);
        let error = run_replay(package.clone(), &replay)
            .expect_err("Replay requests execution beyond the terminal boundary");
        assert!(matches!(
            error,
            HeadlessError::Replay(ReplayError::RunEndedBeforeReplayBoundary {
                terminal_next_tick: Tick(1),
                requested_next_tick: Tick(2),
            })
        ));
    }
}
