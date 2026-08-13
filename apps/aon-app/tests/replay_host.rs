use aon_app::{
    HostError, embedded_empty_package, run_paced_replay_host_harness, run_replay_host_harness,
};
use aon_headless::{load_package, load_replay, run_replay, run_replay_file};
use aon_sim::{
    Command, CommandEnvelope, EndpointTarget, FIXED_ONE, Fixed, FixedAabb, FixedVec2, GateType,
    HashCheckpoint, JunctionId, PlaceGateCommand, PlaceJunctionCommand,
    PlaceMobileSubstrateCommand, PlaceWireCommand, Replay, ReplayError, RoutingDomain, Simulation,
    SimulationPackage, StateHash, Tick,
};
use std::path::PathBuf;
use std::time::Duration;

fn recorded_command_replay(final_next_tick: u64) -> (SimulationPackage, Replay, Vec<StateHash>) {
    assert!(final_next_tick > 3);
    let package = embedded_empty_package().expect("embedded package is valid");
    let mut recorder = Simulation::new(package.clone()).expect("simulation bootstraps");
    let header = recorder.replay_header();
    let commands = vec![
        place_not(Tick(0), 0, FixedVec2::new(Fixed::ZERO, Fixed::ZERO)),
        place_not(
            Tick(3),
            0,
            FixedVec2::new(Fixed(8 * FIXED_ONE), Fixed::ZERO),
        ),
    ];
    let mut checkpoints = vec![HashCheckpoint {
        next_tick: Tick(0),
        state_hash: recorder.state_hash(),
    }];

    for tick in 0..final_next_tick {
        let tick = Tick(tick);
        let batch = commands
            .iter()
            .filter(|command| command.target_tick == tick)
            .cloned()
            .collect::<Vec<_>>();
        let report = recorder.step(&batch).expect("recording succeeds");
        checkpoints.push(HashCheckpoint {
            next_tick: report.next_tick,
            state_hash: report.state_hash,
        });
    }

    let expected_trace = checkpoints
        .iter()
        .map(|checkpoint| checkpoint.state_hash)
        .collect();
    let replay = Replay::new_v2(header, commands, Vec::new(), checkpoints)
        .expect("recorded replay is valid");

    (package, replay, expected_trace)
}

fn place_not(target_tick: Tick, ordinal: u64, origin: FixedVec2) -> CommandEnvelope {
    CommandEnvelope {
        target_tick,
        ordinal,
        command: Command::PlaceGate(PlaceGateCommand {
            gate_type: GateType::Not,
            origin,
            routing_domain: RoutingDomain::OpenWorld,
        }),
    }
}

fn recorded_mobility_replay() -> (SimulationPackage, Replay, Vec<StateHash>) {
    let package = embedded_empty_package().expect("embedded package is valid");
    let mut recorder = Simulation::new(package.clone()).expect("simulation bootstraps");
    let header = recorder.replay_header();
    let pitch = recorder.profiles().physical_scale.world_routing_pitch.0;
    let point = |x| FixedVec2::new(Fixed(x * pitch), Fixed::ZERO);
    let bounds = FixedAabb::new(
        FixedVec2::new(Fixed(-pitch), Fixed(-pitch)),
        FixedVec2::new(Fixed(pitch), Fixed(pitch)),
    );
    let junction = JunctionId(aon_sim::EntityId(1));
    let commands = vec![
        CommandEnvelope {
            target_tick: Tick(0),
            ordinal: 0,
            command: Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: point(2),
            }),
        },
        CommandEnvelope {
            target_tick: Tick(1),
            ordinal: 0,
            command: Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![point(0), point(2)],
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Junction(junction),
            }),
        },
        CommandEnvelope {
            target_tick: Tick(1),
            ordinal: 1,
            command: Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![point(2), point(4)],
                endpoint_a: EndpointTarget::Junction(junction),
                endpoint_b: EndpointTarget::Free,
            }),
        },
        CommandEnvelope {
            target_tick: Tick(2),
            ordinal: 0,
            command: Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
                origin: point(1),
                routing_area: bounds,
                footprint: bounds,
            }),
        },
        CommandEnvelope {
            target_tick: Tick(2),
            ordinal: 1,
            command: Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
                origin: point(3),
                routing_area: bounds,
                footprint: bounds,
            }),
        },
    ];
    let mut checkpoints = vec![HashCheckpoint {
        next_tick: Tick(0),
        state_hash: recorder.state_hash(),
    }];
    for tick in 0..8 {
        let batch = commands
            .iter()
            .filter(|command| command.target_tick == Tick(tick))
            .cloned()
            .collect::<Vec<_>>();
        let report = recorder.step(&batch).expect("mobility recording succeeds");
        assert!(
            report.command_rejections.is_empty(),
            "mobility fixture rejected Tick {tick}: {:?}",
            report.command_rejections
        );
        checkpoints.push(HashCheckpoint {
            next_tick: report.next_tick,
            state_hash: report.state_hash,
        });
    }
    let trace = checkpoints
        .iter()
        .map(|checkpoint| checkpoint.state_hash)
        .collect::<Vec<_>>();
    let replay = Replay::new_v2(header, commands, Vec::new(), checkpoints)
        .expect("mobility replay is valid");
    (package, replay, trace)
}

fn retained_replay(name: &str) -> (PathBuf, SimulationPackage, Replay) {
    let replay_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/replays")
        .join(name);
    let artifact = load_replay(&replay_path).expect("retained Replay decodes");
    let scenario_path = replay_path
        .parent()
        .expect("retained Replay has a parent")
        .join(artifact.scenario_path());
    let package = load_package(scenario_path).expect("retained Replay Scenario loads");
    (replay_path, package, artifact.replay().clone())
}

fn assert_same_trace(expected: &[StateHash], actual: &[StateHash]) {
    assert_eq!(actual.len(), expected.len(), "trace length differs");
    if let Some((index, (expected, actual))) = expected
        .iter()
        .zip(actual)
        .enumerate()
        .find(|(_, (expected, actual))| expected != actual)
    {
        panic!("trace diverged at nextTick {index}: expected {expected}, actual {actual}");
    }
}

fn assert_declared_checkpoints(replay: &Replay, trace: &[StateHash]) {
    for checkpoint in replay.checkpoints() {
        let index = usize::try_from(checkpoint.next_tick.0).expect("checkpoint fits usize");
        assert_eq!(
            trace[index], checkpoint.state_hash,
            "declared checkpoint diverged at nextTick {}",
            checkpoint.next_tick
        );
    }
}

#[test]
fn retained_feedback_replay_matches_headless_and_bevy_independent_of_presentation() {
    let (replay_path, package, replay) = retained_replay("feedback-ring-v1.json");
    let headless = run_replay_file(replay_path).expect("retained feedback Replay runs headlessly");
    assert_eq!(headless.completed_ticks(), replay.final_next_tick().0);
    assert_declared_checkpoints(&replay, headless.checkpoints());

    for presentation_updates in [0, 1, 7] {
        let trace =
            run_replay_host_harness(package.clone(), replay.clone(), presentation_updates, true)
                .expect("replay host succeeds");
        assert_same_trace(headless.checkpoints(), trace.checkpoints());
    }

    let without_presenter = run_replay_host_harness(package, replay, 7, false)
        .expect("replay host succeeds without presenter");
    assert_same_trace(headless.checkpoints(), without_presenter.checkpoints());
}

#[test]
fn retained_feedback_replay_preserves_trace_across_frame_partitioning() {
    let (replay_path, package, replay) = retained_replay("feedback-ring-v1.json");
    let headless = run_replay_file(replay_path).expect("retained feedback Replay runs headlessly");

    let one_long_frame = run_paced_replay_host_harness(
        package.clone(),
        replay.clone(),
        &[Duration::from_millis(1_050)],
    )
    .expect("one long frame preserves all Tick debt");
    assert_same_trace(headless.checkpoints(), one_long_frame.checkpoints());

    let twenty_one_fixed_frames =
        run_paced_replay_host_harness(package, replay, &[Duration::from_millis(50); 21])
            .expect("one frame per Tick preserves the Replay trace");
    assert_same_trace(
        headless.checkpoints(),
        twenty_one_fixed_frames.checkpoints(),
    );
}

#[test]
fn replay_checkpoint_divergence_is_a_host_error() {
    let (package, replay, _) = recorded_command_replay(4);
    let mut checkpoints = replay.checkpoints().to_vec();
    checkpoints[1].state_hash = checkpoints[0].state_hash;
    let divergent = Replay::new_v2(
        *replay.header(),
        replay.commands().to_vec(),
        replay.world_inputs().to_vec(),
        checkpoints,
    )
    .expect("shape remains valid");

    let error = run_replay_host_harness(package, divergent, 0, false)
        .expect_err("checkpoint mismatch must fail the host");
    assert!(matches!(
        error,
        HostError::Replay(ReplayError::CheckpointDivergence {
            next_tick: Tick(1),
            ..
        })
    ));
}

#[test]
fn mobility_replay_matches_headless_bevy_presenter_and_frame_partitions() {
    let (package, replay, expected) = recorded_mobility_replay();
    let headless = run_replay(package.clone(), &replay).expect("mobility replay runs headlessly");
    assert_same_trace(&expected, headless.checkpoints());

    for presentation_updates in [0, 1, 7] {
        let bevy =
            run_replay_host_harness(package.clone(), replay.clone(), presentation_updates, true)
                .expect("mobility replay runs with presenter");
        assert_same_trace(&expected, bevy.checkpoints());
    }
    let without_presenter = run_replay_host_harness(package.clone(), replay.clone(), 7, false)
        .expect("mobility replay runs without presenter");
    assert_same_trace(&expected, without_presenter.checkpoints());

    let one_long_frame = run_paced_replay_host_harness(
        package.clone(),
        replay.clone(),
        &[Duration::from_millis(450)],
    )
    .expect("long frame preserves mobility Tick debt");
    assert_same_trace(&expected, one_long_frame.checkpoints());
    let split_frames =
        run_paced_replay_host_harness(package, replay, &[Duration::from_millis(50); 8])
            .expect("split frames preserve mobility trace");
    assert_same_trace(&expected, split_frames.checkpoints());
}

#[test]
fn retained_stage0_100k_replay_matches_headless_and_bevy_complete_trace() {
    let (replay_path, package, replay) = retained_replay("stage0-100k-v1.json");
    let headless = run_replay_file(replay_path).expect("retained 100k Replay runs headlessly");
    assert_eq!(headless.completed_ticks(), 100_000);
    assert_eq!(headless.checkpoints().len(), 100_001);
    assert_declared_checkpoints(&replay, headless.checkpoints());
    assert_eq!(
        headless.final_hash(),
        replay
            .checkpoints()
            .last()
            .expect("retained Replay has a final golden")
            .state_hash
    );

    let bevy = run_replay_host_harness(package, replay, 0, false)
        .expect("retained 100k Replay runs through Bevy FixedUpdate");
    assert_same_trace(headless.checkpoints(), bevy.checkpoints());
}

#[test]
fn retained_s1m1_capacity_replay_matches_headless_and_bevy_fixed_update() {
    let (replay_path, package, replay) = retained_replay("s1-m1-capacity-accounting-v1.json");
    let headless = run_replay_file(replay_path).expect("retained capacity Replay runs headlessly");
    assert_eq!(headless.scenario_id(), "s1-m1-capacity-accounting-v1");
    assert_eq!(headless.completed_ticks(), 3);
    assert_eq!(headless.checkpoints().len(), 4);
    assert_declared_checkpoints(&replay, headless.checkpoints());

    let expected_accounting = [
        (
            Tick(0),
            Tick(1),
            10_u64 * FIXED_ONE as u64,
            1_000_u64 * FIXED_ONE as u64,
        ),
        (
            Tick(1),
            Tick(2),
            10_u64 * FIXED_ONE as u64,
            1_000_u64 * FIXED_ONE as u64,
        ),
        (
            Tick(2),
            Tick(3),
            12_u64 * FIXED_ONE as u64,
            1_000_u64 * FIXED_ONE as u64,
        ),
    ];

    let assert_reports = |reports: &[aon_sim::StepReport]| {
        assert_eq!(reports.len(), expected_accounting.len());
        for (report, (completed, next, used, supported)) in reports.iter().zip(expected_accounting)
        {
            assert_eq!((report.completed_tick, report.next_tick), (completed, next));
            assert!(report.command_rejections.is_empty());
            let accounting = report
                .network_accounting
                .expect("capacity Replay reports Phase 4 accounting");
            assert_eq!(
                (accounting.used().0, accounting.supported().0),
                (used, supported)
            );
            let checkpoint = usize::try_from(next.0).expect("retained Tick fits usize");
            assert_eq!(report.state_hash, headless.checkpoints()[checkpoint]);
        }
    };

    for presentation_updates in [0, 1, 7] {
        let bevy =
            run_replay_host_harness(package.clone(), replay.clone(), presentation_updates, true)
                .expect("retained capacity Replay runs with the presenter");
        assert_same_trace(headless.checkpoints(), bevy.checkpoints());
        assert_reports(bevy.reports());
    }

    let without_presenter = run_replay_host_harness(package.clone(), replay.clone(), 7, false)
        .expect("retained capacity Replay runs without the presenter");
    assert_same_trace(headless.checkpoints(), without_presenter.checkpoints());
    assert_reports(without_presenter.reports());

    let one_frame = run_paced_replay_host_harness(package, replay, &[Duration::from_millis(150)])
        .expect("one frame preserves all capacity Replay Ticks");
    assert_same_trace(headless.checkpoints(), one_frame.checkpoints());
    assert_reports(one_frame.reports());
}
