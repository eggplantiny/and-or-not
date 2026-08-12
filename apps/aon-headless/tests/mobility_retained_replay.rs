use aon_headless::run_replay_file;
use aon_sim::{
    ArtifactBytes, Command, DriveStrength, DriverId, EntityId, Fixed, GateId, Heading, LogicLevel,
    MobileId, RenderSnapshot, Simulation, StateHash, Tick, TrackPosition, WireId, decode_package,
    decode_replay_artifact, encode_replay_artifact,
};
use std::path::PathBuf;

const REPLAY_BYTES: &[u8] =
    include_bytes!("../../../fixtures/replays/mobility-retained-stop-v1.json");
const CURRENT_INPUT_REPLAY_BYTES: &[u8] =
    include_bytes!("../../../fixtures/replays/mobility-current-input-stop-v1.json");
const SCENARIO: &[u8] = include_bytes!("../../../fixtures/scenarios/empty.json");
const NUMERIC: &[u8] = include_bytes!("../../../profiles/numeric/v1.json");
const PHYSICAL: &[u8] = include_bytes!("../../../profiles/physical-scale/stage0-alpha.json");
const BALANCE: &[u8] = include_bytes!("../../../profiles/balance/stage0-alpha.json");

const WORLD_PITCH: i64 = 65_536;
const JUNCTION_X: i64 = 32 * WORLD_PITCH;
const B_X: i64 = 64 * WORLD_PITCH;
const PULSE_TICK: Tick = Tick(70);
const FIRST_STOP_TICK: Tick = Tick(81);
const RELEASE_TICK: Tick = Tick(97);
const FINAL_TICK: Tick = Tick(162);

const MOBILE: MobileId = MobileId(EntityId(4));
const Q: GateId = GateId(EntityId(6));
const QBAR: GateId = GateId(EntityId(8));
const SET_DRIVER: DriverId = DriverId(EntityId(7));

fn gate_level(simulation: &Simulation, gate: GateId) -> LogicLevel {
    simulation
        .gate_signal_state(gate)
        .expect("product-comparison Gate is live")
        .current_output
}

fn package() -> aon_sim::SimulationPackage {
    decode_package(ArtifactBytes {
        scenario: SCENARIO,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("reference package decodes")
}

fn mobile(simulation: &Simulation) -> aon_sim::MobileRenderRecord {
    let mut snapshot = RenderSnapshot::default();
    simulation.write_render_snapshot(&mut snapshot);
    snapshot
        .mobiles()
        .iter()
        .copied()
        .find(|record| record.id == MOBILE)
        .expect("retained Mobile is live")
}

fn assert_gate_quiescent(simulation: &Simulation, gate: GateId, level: LogicLevel) {
    let state = simulation
        .gate_signal_state(gate)
        .expect("retained feedback Gate is live");
    assert_eq!(state.current_output, level);
    assert_eq!(state.desired_output, level);
    assert_eq!(state.pending_due_tick, None);
    assert_eq!(state.pending_level, None);
}

#[test]
fn retained_mobility_stop_replay_is_canonical_and_executes_headlessly() {
    let artifact =
        decode_replay_artifact(REPLAY_BYTES).expect("retained mobility Replay strictly decodes");
    assert_eq!(
        encode_replay_artifact(&artifact).expect("retained mobility Replay canonically encodes"),
        REPLAY_BYTES
    );
    assert_eq!(artifact.scenario_path(), "../scenarios/empty.json");
    assert_eq!(artifact.replay().commands().len(), 19);
    assert_eq!(artifact.replay().final_next_tick(), FINAL_TICK);
    assert_eq!(
        artifact
            .replay()
            .checkpoints()
            .iter()
            .map(|checkpoint| checkpoint.next_tick)
            .collect::<Vec<_>>(),
        [
            Tick(0),
            Tick(24),
            PULSE_TICK,
            FIRST_STOP_TICK,
            RELEASE_TICK,
            Tick(98),
            FINAL_TICK,
        ]
    );
    assert_eq!(
        artifact
            .replay()
            .checkpoints()
            .last()
            .expect("retained mobility Replay has a final checkpoint")
            .state_hash,
        StateHash::from_hex("4ce711b67ec13274603422e6c55c105373b43dfde50fe6df263beaed31d4538f")
            .expect("retained mobility final golden is canonical")
    );

    let pulse_commands = artifact
        .replay()
        .commands()
        .iter()
        .filter_map(|envelope| match &envelope.command {
            Command::SetExternalDriver(command) if command.driver == SET_DRIVER => {
                Some((envelope.target_tick, command.level, command.strength))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        pulse_commands,
        [
            (PULSE_TICK, LogicLevel::High, DriveStrength(100)),
            (RELEASE_TICK, LogicLevel::Low, DriveStrength(0)),
        ],
        "the checked-in Replay contains an explicit finite SET pulse"
    );

    let replay_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/replays/mobility-retained-stop-v1.json");
    let headless = run_replay_file(replay_path).expect("retained mobility Replay runs headlessly");
    assert_eq!(headless.completed_ticks(), FINAL_TICK.0);
    assert_eq!(
        headless.final_hash(),
        artifact
            .replay()
            .checkpoints()
            .last()
            .expect("final checkpoint exists")
            .state_hash
    );

    let mut simulation = Simulation::new(package()).expect("reference simulation starts");
    artifact
        .replay()
        .validate_against(&simulation)
        .expect("retained mobility Replay matches the package");
    let mut trace = vec![simulation.state_hash()];
    let b_position = TrackPosition::Edge {
        edge: WireId(EntityId(3)),
        offset: Fixed(B_X - JUNCTION_X),
        heading: Heading::Forward,
    };
    let mut stopped_at = None;

    while simulation.next_tick() < artifact.replay().final_next_tick() {
        if simulation.next_tick() == PULSE_TICK {
            assert_eq!(
                mobile(&simulation).track_position,
                b_position,
                "the Mobile reaches B before the SET pulse"
            );
            assert_gate_quiescent(&simulation, Q, LogicLevel::Low);
            assert_gate_quiescent(&simulation, QBAR, LogicLevel::High);
            assert_eq!(mobile(&simulation).stop, LogicLevel::Low);
        }
        if simulation.next_tick() == RELEASE_TICK {
            let retained = mobile(&simulation);
            assert_eq!(retained.stop, LogicLevel::High);
            assert_eq!(
                (retained.left, retained.right),
                (LogicLevel::Low, LogicLevel::Low)
            );
            assert_eq!(Some(retained.track_position), stopped_at);
            assert_gate_quiescent(&simulation, Q, LogicLevel::High);
            assert_gate_quiescent(&simulation, QBAR, LogicLevel::Low);
        }

        let target_tick = simulation.next_tick();
        let commands = artifact
            .replay()
            .commands_for_tick(target_tick)
            .cloned()
            .collect::<Vec<_>>();
        let report = simulation
            .step(&commands)
            .expect("retained mobility Replay Tick succeeds");
        assert!(report.command_rejections.is_empty());
        trace.push(report.state_hash);

        if simulation.next_tick() == FIRST_STOP_TICK {
            let retained = mobile(&simulation);
            assert_eq!(retained.stop, LogicLevel::High);
            assert_gate_quiescent(&simulation, Q, LogicLevel::High);
            assert_gate_quiescent(&simulation, QBAR, LogicLevel::Low);
            stopped_at = Some(retained.track_position);
        }
        if target_tick >= FIRST_STOP_TICK {
            let movement = report
                .mobile_movements
                .first()
                .expect("retained Replay reports its Mobile movement");
            assert_eq!(movement.controls.stop, LogicLevel::High);
            assert_eq!(movement.granted_budget, Fixed(0));
            assert_eq!(movement.consumed_budget, Fixed(0));
            assert_eq!(
                movement.start,
                stopped_at.expect("STOP position was observed")
            );
            assert_eq!(
                movement.end,
                stopped_at.expect("STOP position was observed")
            );
            assert_eq!(mobile(&simulation).track_position, movement.end);
        }
        if target_tick >= RELEASE_TICK {
            assert_gate_quiescent(&simulation, Q, LogicLevel::High);
            assert_gate_quiescent(&simulation, QBAR, LogicLevel::Low);
            assert_eq!(mobile(&simulation).stop, LogicLevel::High);
        }
    }

    artifact
        .replay()
        .verify_trace(&trace)
        .expect("manual headless trace matches every retained checkpoint");
    assert_eq!(trace, headless.checkpoints());
    let released_input = simulation
        .driver_sample(SET_DRIVER)
        .expect("released SET input remains observable");
    assert_eq!(released_input.level, LogicLevel::Low);
    assert_eq!(released_input.strength, DriveStrength(0));
    assert_gate_quiescent(&simulation, Q, LogicLevel::High);
    assert_gate_quiescent(&simulation, QBAR, LogicLevel::Low);
    assert_eq!(mobile(&simulation).stop, LogicLevel::High);
    assert_eq!(mobile(&simulation).track_position, stopped_at.unwrap());
}

#[test]
fn current_input_only_replay_is_canonical_and_resumes_after_the_matched_set_release() {
    let retained =
        decode_replay_artifact(REPLAY_BYTES).expect("retained-state Replay strictly decodes");
    let current = decode_replay_artifact(CURRENT_INPUT_REPLAY_BYTES)
        .expect("current-input-only Replay strictly decodes");
    assert_eq!(
        encode_replay_artifact(&current).expect("current-input-only Replay canonically encodes"),
        CURRENT_INPUT_REPLAY_BYTES
    );
    assert_eq!(current.scenario_path(), retained.scenario_path());
    assert_eq!(current.replay().commands().len(), 23);
    assert_eq!(current.replay().final_next_tick(), FINAL_TICK);
    assert_eq!(
        current
            .replay()
            .checkpoints()
            .iter()
            .map(|checkpoint| checkpoint.next_tick)
            .collect::<Vec<_>>(),
        [
            Tick(0),
            Tick(24),
            PULSE_TICK,
            FIRST_STOP_TICK,
            RELEASE_TICK,
            Tick(98),
            FINAL_TICK,
        ]
    );
    assert_eq!(
        current
            .replay()
            .checkpoints()
            .last()
            .expect("current-input-only Replay has a final checkpoint")
            .state_hash,
        StateHash::from_hex("d3915d1586e7e4b109858c2d702e68373c94b34456ecdeb78997686ba3fb244d")
            .expect("current-input-only final golden is canonical")
    );

    let pulse_commands = current
        .replay()
        .commands()
        .iter()
        .filter_map(|envelope| match &envelope.command {
            Command::SetExternalDriver(command) if command.driver == SET_DRIVER => {
                Some((envelope.target_tick, command.level, command.strength))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        pulse_commands,
        [
            (PULSE_TICK, LogicLevel::High, DriveStrength(100)),
            (RELEASE_TICK, LogicLevel::Low, DriveStrength(0)),
        ],
        "both product variants use the same finite SET pulse"
    );

    let replay_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/replays/mobility-current-input-stop-v1.json");
    let headless = run_replay_file(replay_path).expect("current-input-only Replay runs headlessly");
    assert_eq!(headless.completed_ticks(), FINAL_TICK.0);
    assert_eq!(
        headless.final_hash(),
        current
            .replay()
            .checkpoints()
            .last()
            .expect("final current-input checkpoint exists")
            .state_hash
    );

    let mut retained_simulation = Simulation::new(package()).expect("retained simulation starts");
    let mut current_simulation = Simulation::new(package()).expect("control simulation starts");
    retained
        .replay()
        .validate_against(&retained_simulation)
        .expect("retained Replay matches the package");
    current
        .replay()
        .validate_against(&current_simulation)
        .expect("current-input-only Replay matches the package");
    let mut current_trace = vec![current_simulation.state_hash()];
    let b_position = TrackPosition::Edge {
        edge: WireId(EntityId(3)),
        offset: Fixed(B_X - JUNCTION_X),
        heading: Heading::Forward,
    };
    let mut retained_stopped_at = None;
    let mut current_stopped_at = None;

    while current_simulation.next_tick() < FINAL_TICK {
        let next_tick = current_simulation.next_tick();
        assert_eq!(retained_simulation.next_tick(), next_tick);
        if next_tick == Tick(24) {
            let retained_mobile = mobile(&retained_simulation);
            let current_mobile = mobile(&current_simulation);
            assert_eq!(
                current_mobile.track_position,
                retained_mobile.track_position
            );
            assert_eq!(current_mobile.stop, LogicLevel::Low);
            assert_eq!(gate_level(&current_simulation, Q), LogicLevel::Low);
            assert_eq!(gate_level(&current_simulation, QBAR), LogicLevel::High);

            let mut snapshot = RenderSnapshot::default();
            current_simulation.write_render_snapshot(&mut snapshot);
            let live_wires = snapshot
                .wires()
                .iter()
                .map(|wire| wire.id)
                .collect::<Vec<_>>();
            for removed_feedback in [10, 14, 15] {
                assert!(
                    !live_wires.contains(&WireId(EntityId(removed_feedback))),
                    "the ready current-input design has no retained feedback wire {removed_feedback}"
                );
            }
            assert!(live_wires.contains(&WireId(EntityId(16))));
        }
        if next_tick == PULSE_TICK {
            assert_eq!(mobile(&retained_simulation).track_position, b_position);
            assert_eq!(mobile(&current_simulation).track_position, b_position);
        }

        let retained_commands = retained
            .replay()
            .commands_for_tick(next_tick)
            .cloned()
            .collect::<Vec<_>>();
        let current_commands = current
            .replay()
            .commands_for_tick(next_tick)
            .cloned()
            .collect::<Vec<_>>();
        retained_simulation
            .step(&retained_commands)
            .expect("retained product Tick succeeds");
        let current_report = current_simulation
            .step(&current_commands)
            .expect("current-input product Tick succeeds");
        current_trace.push(current_report.state_hash);

        match current_simulation.next_tick() {
            Tick(71) => {
                let input = current_simulation
                    .driver_sample(SET_DRIVER)
                    .expect("SET input is observable");
                assert_eq!(input.level, LogicLevel::High);
                assert_eq!(gate_level(&current_simulation, Q), LogicLevel::Low);
                assert_eq!(gate_level(&current_simulation, QBAR), LogicLevel::High);
                assert_eq!(mobile(&current_simulation).stop, LogicLevel::Low);
            }
            FIRST_STOP_TICK => {
                assert_eq!(gate_level(&current_simulation, Q), LogicLevel::High);
                assert_eq!(gate_level(&current_simulation, QBAR), LogicLevel::Low);
                assert_eq!(mobile(&current_simulation).stop, LogicLevel::High);
                retained_stopped_at = Some(mobile(&retained_simulation).track_position);
                current_stopped_at = Some(mobile(&current_simulation).track_position);
                assert_eq!(current_stopped_at, retained_stopped_at);
            }
            Tick(98) => {
                let retained_input = retained_simulation
                    .driver_sample(SET_DRIVER)
                    .expect("retained SET input is observable");
                let current_input = current_simulation
                    .driver_sample(SET_DRIVER)
                    .expect("current-only SET input is observable");
                assert_eq!(current_input, retained_input);
                assert_eq!(current_input.level, LogicLevel::Low);
                assert_eq!(current_input.strength, DriveStrength(0));
                assert_eq!(gate_level(&current_simulation, Q), LogicLevel::High);
                assert_eq!(gate_level(&current_simulation, QBAR), LogicLevel::Low);
                assert_eq!(mobile(&current_simulation).stop, LogicLevel::High);
            }
            FINAL_TICK => {
                assert_eq!(gate_level(&current_simulation, Q), LogicLevel::Low);
                assert_eq!(gate_level(&current_simulation, QBAR), LogicLevel::High);
                assert_eq!(mobile(&current_simulation).stop, LogicLevel::Low);
                assert_eq!(gate_level(&retained_simulation, Q), LogicLevel::High);
                assert_eq!(gate_level(&retained_simulation, QBAR), LogicLevel::Low);
                assert_eq!(mobile(&retained_simulation).stop, LogicLevel::High);
            }
            _ => {}
        }
    }

    current
        .replay()
        .verify_trace(&current_trace)
        .expect("manual current-input trace matches every checkpoint");
    assert_eq!(current_trace, headless.checkpoints());
    assert_eq!(
        mobile(&retained_simulation).track_position,
        retained_stopped_at.expect("retained STOP position was observed")
    );
    assert_ne!(
        mobile(&current_simulation).track_position,
        current_stopped_at.expect("current-only STOP position was observed"),
        "current-input-only Mobile resumes after SET returns LOW"
    );
    assert_ne!(
        mobile(&current_simulation).track_position,
        mobile(&retained_simulation).track_position,
        "same current SET=LOW produces distinct World behavior only when State is retained"
    );
}
