use aon_sim::{
    ArtifactBytes, Command, CommandEnvelope, DriveStrength, DriverId, EndpointTarget, EntityId,
    FIXED_ONE, Fixed, FixedAabb, FixedVec2, GateId, GatePort, GatePortRef, GateType,
    HashCheckpoint, LogicLevel, PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceWireCommand,
    Replay, ReplayArtifact, RoutingDomain, SetExternalDriverCommand, Simulation, Tick,
    decode_package, encode_replay_artifact,
};
use std::path::Path;

const P: i64 = aon_sim::REFERENCE_CIRCUIT_ROUTING_PITCH.0;
const FINAL_LONG_TICK: u64 = 100_000;

const SCENARIO: &[u8] = include_bytes!("../../../fixtures/scenarios/empty.json");
const NUMERIC: &[u8] = include_bytes!("../../../profiles/numeric/v1.json");
const PHYSICAL: &[u8] = include_bytes!("../../../profiles/physical-scale/stage0-alpha.json");
const BALANCE: &[u8] = include_bytes!("../../../profiles/balance/stage0-alpha.json");

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

const fn pitch(x: i64, y: i64) -> FixedVec2 {
    point(x * P, y * P)
}

fn package() -> aon_sim::SimulationPackage {
    decode_package(ArtifactBytes {
        scenario: SCENARIO,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("reference package is valid")
}

fn envelope(target_tick: u64, ordinal: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        target_tick: Tick(target_tick),
        ordinal,
        command,
    }
}

fn substrate() -> Command {
    let half = 32 * FIXED_ONE;
    let bounds = FixedAabb::new(point(-half, -half), point(half, half));
    Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
        origin: point(0, 0),
        routing_area: bounds,
        footprint: bounds,
    })
}

fn gate(target_tick: u64, ordinal: u64, gate_type: GateType, origin: FixedVec2) -> CommandEnvelope {
    envelope(
        target_tick,
        ordinal,
        Command::PlaceGate(PlaceGateCommand {
            gate_type,
            origin,
            routing_domain: RoutingDomain::FixedSubstrate(EntityId(1)),
        }),
    )
}

fn gate_endpoint(gate: u64, port: GatePort) -> EndpointTarget {
    EndpointTarget::GatePort(GatePortRef {
        gate: GateId(EntityId(gate)),
        port,
    })
}

fn wire(
    target_tick: u64,
    ordinal: u64,
    points: Vec<FixedVec2>,
    gate_a: u64,
    port_a: GatePort,
    gate_b: u64,
    port_b: GatePort,
) -> CommandEnvelope {
    envelope(
        target_tick,
        ordinal,
        Command::PlaceWire(PlaceWireCommand {
            routing_domain: RoutingDomain::FixedSubstrate(EntityId(1)),
            points,
            endpoint_a: gate_endpoint(gate_a, port_a),
            endpoint_b: gate_endpoint(gate_b, port_b),
        }),
    )
}

fn external(
    target_tick: u64,
    ordinal: u64,
    driver: u64,
    level: LogicLevel,
    strength: u64,
) -> CommandEnvelope {
    envelope(
        target_tick,
        ordinal,
        Command::SetExternalDriver(SetExternalDriverCommand {
            driver: DriverId(EntityId(driver)),
            level,
            strength: DriveStrength(strength),
        }),
    )
}

fn run(
    commands: &[CommandEnvelope],
    final_next_tick: u64,
) -> (Simulation, Vec<aon_sim::StateHash>) {
    let mut simulation = Simulation::new(package()).expect("reference simulation starts");
    let mut trace = vec![simulation.state_hash()];
    while simulation.next_tick().0 < final_next_tick {
        let batch = commands
            .iter()
            .filter(|command| command.target_tick == simulation.next_tick())
            .cloned()
            .collect::<Vec<_>>();
        let report = simulation.step(&batch).expect("fixture Tick succeeds");
        assert!(report.command_rejections.is_empty());
        trace.push(report.state_hash);
    }
    (simulation, trace)
}

fn ring_commands() -> Vec<CommandEnvelope> {
    vec![
        envelope(0, 0, substrate()),
        gate(1, 0, GateType::Not, pitch(0, 0)),
        wire(
            2,
            0,
            vec![
                pitch(1, 0),
                pitch(2, 0),
                pitch(2, 2),
                pitch(-2, 2),
                pitch(-2, 0),
                pitch(-1, 0),
            ],
            2,
            GatePort::Output,
            2,
            GatePort::InputA,
        ),
    ]
}

fn latch_commands() -> Vec<CommandEnvelope> {
    vec![
        envelope(0, 0, substrate()),
        gate(1, 0, GateType::Or, pitch(0, -4)),
        gate(1, 1, GateType::Not, pitch(4, -4)),
        gate(1, 2, GateType::Or, pitch(0, 4)),
        gate(1, 3, GateType::Not, pitch(4, 4)),
        wire(
            2,
            0,
            vec![pitch(1, -4), pitch(3, -4)],
            2,
            GatePort::Output,
            3,
            GatePort::InputA,
        ),
        wire(
            2,
            1,
            vec![pitch(1, 4), pitch(3, 4)],
            4,
            GatePort::Output,
            5,
            GatePort::InputA,
        ),
        wire(
            2,
            2,
            vec![
                pitch(5, -4),
                pitch(6, -4),
                pitch(6, 7),
                pitch(-2, 7),
                pitch(-2, 4),
                point(-P, 7 * P / 2),
            ],
            3,
            GatePort::Output,
            4,
            GatePort::InputA,
        ),
        wire(
            2,
            3,
            vec![
                pitch(5, 4),
                pitch(7, 4),
                pitch(7, -6),
                pitch(-2, -6),
                pitch(-2, -4),
                point(-P, -7 * P / 2),
            ],
            5,
            GatePort::Output,
            2,
            GatePort::InputB,
        ),
        // Driver IDs follow the frozen Gate-port allocation order: OR_Q.InputA=1,
        // OR_Qbar.InputB=7.
        external(2, 4, 7, LogicLevel::High, 100),
        external(15, 0, 7, LogicLevel::Low, 0),
        external(20, 0, 1, LogicLevel::High, 100),
        external(33, 0, 1, LogicLevel::Low, 0),
    ]
}

fn artifact(
    commands: Vec<CommandEnvelope>,
    trace: &[aon_sim::StateHash],
    checkpoint_ticks: impl IntoIterator<Item = u64>,
) -> ReplayArtifact {
    let initial = Simulation::new(package()).expect("reference simulation starts");
    let checkpoints = checkpoint_ticks
        .into_iter()
        .map(|next_tick| HashCheckpoint {
            next_tick: Tick(next_tick),
            state_hash: trace[usize::try_from(next_tick).expect("fixture Tick fits usize")],
        })
        .collect();
    ReplayArtifact::new(
        "../scenarios/empty.json",
        Replay::new(initial.replay_header(), commands, checkpoints).expect("Replay is valid"),
    )
    .expect("Scenario locator is valid")
}

fn emit_artifact(label: &str, path: &Path, artifact: &ReplayArtifact, write: bool) {
    let bytes = encode_replay_artifact(artifact).expect("Replay encodes");
    if write {
        std::fs::write(path, bytes).expect("Replay fixture writes");
        println!("wrote {}", path.display());
    } else {
        println!("=== {label} ===");
        println!("{}", String::from_utf8(bytes).expect("Replay is UTF-8"));
    }
}

fn main() {
    let write = std::env::args().any(|argument| argument == "--write");
    let ring_commands = ring_commands();
    let (_, ring_trace) = run(&ring_commands, 21);
    let ring = artifact(ring_commands, &ring_trace, 0..=21);
    emit_artifact(
        "RING",
        Path::new("fixtures/replays/feedback-ring-v1.json"),
        &ring,
        write,
    );

    let latch_commands = latch_commands();
    let (latch_simulation, latch_trace) = run(&latch_commands, FINAL_LONG_TICK);
    let q = latch_simulation
        .gate_signal_state(GateId(EntityId(3)))
        .expect("Q Gate remains live");
    let qbar = latch_simulation
        .gate_signal_state(GateId(EntityId(5)))
        .expect("Qbar Gate remains live");
    assert_eq!(q.current_output, LogicLevel::Low);
    assert_eq!(qbar.current_output, LogicLevel::High);
    assert_eq!(q.pending_due_tick, None);
    assert_eq!(qbar.pending_due_tick, None);
    let latch = artifact(
        latch_commands,
        &latch_trace,
        [0, 15, 20, 33, 46, FINAL_LONG_TICK],
    );
    emit_artifact(
        "LATCH",
        Path::new("fixtures/replays/stage0-100k-v1.json"),
        &latch,
        write,
    );
}
