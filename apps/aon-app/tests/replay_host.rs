use aon_app::{
    HostError, embedded_empty_package, run_paced_replay_host_harness, run_replay_host_harness,
};
use aon_headless::{load_package, load_replay, run_replay_file};
use aon_sim::{
    Command, CommandEnvelope, FIXED_ONE, Fixed, FixedVec2, GateType, HashCheckpoint,
    PlaceGateCommand, Replay, ReplayError, RoutingDomain, Simulation, SimulationPackage, StateHash,
    Tick,
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
    let replay = Replay::new(header, commands, checkpoints).expect("recorded replay is valid");

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
    let divergent = Replay::new(*replay.header(), replay.commands().to_vec(), checkpoints)
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
