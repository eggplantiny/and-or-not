use aon_sim::{
    ArtifactBytes, Command, CommandEnvelope, DriveStrength, DriverId, EndpointTarget, EntityId,
    Fixed, FixedAabb, FixedVec2, GateId, GatePort, GatePortRef, GateType, HashCheckpoint,
    JunctionId, LogicLevel, MobileId, MobilePort, MobilePortRef, PlaceGateCommand,
    PlaceJunctionCommand, PlaceMobileSubstrateCommand, PlaceWireCommand, RemoveEntityCommand,
    RenderSnapshot, Replay, ReplayArtifact, RoutingDomain, SetExternalDriverCommand, Simulation,
    StateHash, Tick, WireId, decode_package, encode_replay_artifact,
};
use std::path::Path;

const SCENARIO: &[u8] = include_bytes!("../../../fixtures/scenarios/empty.json");
const NUMERIC: &[u8] = include_bytes!("../../../profiles/numeric/v1.json");
const PHYSICAL: &[u8] = include_bytes!("../../../profiles/physical-scale/stage0-alpha.json");
const BALANCE: &[u8] = include_bytes!("../../../profiles/balance/stage0-alpha.json");

const WORLD_PITCH: i64 = 65_536;
const CIRCUIT_PITCH: i64 = 16_384;
const JUNCTION_X: i64 = 32 * WORLD_PITCH;
const B_X: i64 = 64 * WORLD_PITCH;
const ASSERTED: DriveStrength = DriveStrength(100);
const RELEASED: DriveStrength = DriveStrength(0);
const READY_TICK: u64 = 24;
const PULSE_TICK: u64 = 70;
const FIRST_STOP_TICK: u64 = 81;
const RELEASE_TICK: u64 = 97;
const FINAL_TICK: u64 = 162;

const JUNCTION: JunctionId = JunctionId(EntityId(1));
const EDGE_B: WireId = WireId(EntityId(3));
const MOBILE: MobileId = MobileId(EntityId(4));
const Q: GateId = GateId(EntityId(6));
const QBAR: GateId = GateId(EntityId(8));
const RESET_DRIVER: DriverId = DriverId(EntityId(1));
const SET_DRIVER: DriverId = DriverId(EntityId(7));
const Q_INPUT_WIRE: EntityId = EntityId(10);
const Q_FEEDBACK_WIRE: EntityId = EntityId(14);
const QBAR_FEEDBACK_WIRE: EntityId = EntityId(15);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Design {
    RetainedState,
    CurrentInputOnly,
}

impl Design {
    const fn fixture_path(self) -> &'static str {
        match self {
            Self::RetainedState => "fixtures/replays/mobility-retained-stop-v1.json",
            Self::CurrentInputOnly => "fixtures/replays/mobility-current-input-stop-v1.json",
        }
    }
}

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

const fn circuit_point(x: i64, y: i64) -> FixedVec2 {
    point(x * CIRCUIT_PITCH, y * CIRCUIT_PITCH)
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

fn gate(gate_type: GateType, origin: FixedVec2, domain: RoutingDomain) -> Command {
    Command::PlaceGate(PlaceGateCommand {
        gate_type,
        origin,
        routing_domain: domain,
    })
}

fn gate_port(gate: GateId, port: GatePort) -> EndpointTarget {
    EndpointTarget::GatePort(GatePortRef { gate, port })
}

fn wire(
    domain: RoutingDomain,
    points: Vec<FixedVec2>,
    endpoint_a: EndpointTarget,
    endpoint_b: EndpointTarget,
) -> Command {
    Command::PlaceWire(PlaceWireCommand {
        routing_domain: domain,
        points,
        endpoint_a,
        endpoint_b,
    })
}

fn external(driver: DriverId, level: LogicLevel, strength: DriveStrength) -> Command {
    Command::SetExternalDriver(SetExternalDriverCommand {
        driver,
        level,
        strength,
    })
}

struct Recorder {
    simulation: Simulation,
    commands: Vec<CommandEnvelope>,
    trace: Vec<StateHash>,
}

impl Recorder {
    fn new() -> Self {
        let simulation = Simulation::new(package()).expect("reference simulation starts");
        let trace = vec![simulation.state_hash()];
        Self {
            simulation,
            commands: Vec::new(),
            trace,
        }
    }

    fn step(&mut self, commands: Vec<Command>) {
        let target_tick = self.simulation.next_tick();
        let envelopes = commands
            .into_iter()
            .enumerate()
            .map(|(ordinal, command)| CommandEnvelope {
                target_tick,
                ordinal: u64::try_from(ordinal).expect("fixture ordinal fits u64"),
                command,
            })
            .collect::<Vec<_>>();
        let report = self
            .simulation
            .step(&envelopes)
            .expect("retained mobility fixture Tick succeeds");
        assert!(
            report.command_rejections.is_empty(),
            "retained mobility fixture rejects no commands: {:?}",
            report.command_rejections
        );
        self.commands.extend(envelopes);
        self.trace.push(report.state_hash);
    }

    fn empty(&mut self) {
        self.step(Vec::new());
    }

    fn next_tick(&self) -> u64 {
        self.simulation.next_tick().0
    }

    fn checkpoint(&self, next_tick: u64) -> HashCheckpoint {
        HashCheckpoint {
            next_tick: Tick(next_tick),
            state_hash: self.trace[usize::try_from(next_tick).expect("fixture Tick fits usize")],
        }
    }
}

fn mobile(simulation: &Simulation) -> aon_sim::MobileRenderRecord {
    let mut snapshot = RenderSnapshot::default();
    simulation.write_render_snapshot(&mut snapshot);
    snapshot
        .mobiles()
        .iter()
        .copied()
        .find(|record| record.id == MOBILE)
        .expect("retained Mobile remains live")
}

fn gate_is_quiescent(simulation: &Simulation, gate: GateId, level: LogicLevel) -> bool {
    simulation.gate_signal_state(gate).is_some_and(|state| {
        state.current_output == level
            && state.desired_output == level
            && state.pending_due_tick.is_none()
            && state.pending_level.is_none()
    })
}

fn record_fixture(design: Design) -> (ReplayArtifact, Vec<u64>) {
    let mut recorder = Recorder::new();

    recorder.step(vec![Command::PlaceJunction(PlaceJunctionCommand {
        routing_domain: RoutingDomain::OpenWorld,
        position: point(JUNCTION_X, 0),
    })]);
    recorder.step(vec![
        wire(
            RoutingDomain::OpenWorld,
            vec![point(0, 0), point(JUNCTION_X, 0)],
            EndpointTarget::Free,
            EndpointTarget::Junction(JUNCTION),
        ),
        wire(
            RoutingDomain::OpenWorld,
            vec![point(JUNCTION_X, 0), point(B_X, 0)],
            EndpointTarget::Junction(JUNCTION),
            EndpointTarget::Free,
        ),
    ]);

    let local_bounds = FixedAabb::new(circuit_point(-12, -12), circuit_point(12, 12));
    recorder.step(vec![Command::PlaceMobileSubstrate(
        PlaceMobileSubstrateCommand {
            origin: point(0, 0),
            routing_area: local_bounds,
            footprint: local_bounds,
        },
    )]);
    let domain = RoutingDomain::MobileSubstrate(MOBILE.entity_id());

    recorder.step(vec![
        gate(GateType::Or, circuit_point(0, -4), domain),
        gate(GateType::Not, circuit_point(4, -4), domain),
        gate(GateType::Or, circuit_point(0, 4), domain),
        gate(GateType::Not, circuit_point(4, 4), domain),
        Command::PlaceJunction(PlaceJunctionCommand {
            routing_domain: domain,
            position: circuit_point(2, 4),
        }),
    ]);

    let or_q = GateId(EntityId(5));
    let or_qbar = GateId(EntityId(7));
    let fanout = JunctionId(EntityId(9));
    // Both variants share the exact construction and startup prefix. The current-input-only
    // variant removes every live feedback edge before the product-comparison ready checkpoint.
    recorder.step(vec![
        wire(
            domain,
            vec![circuit_point(1, -4), circuit_point(3, -4)],
            gate_port(or_q, GatePort::Output),
            gate_port(Q, GatePort::InputA),
        ),
        wire(
            domain,
            vec![circuit_point(1, 4), circuit_point(2, 4)],
            gate_port(or_qbar, GatePort::Output),
            EndpointTarget::Junction(fanout),
        ),
        wire(
            domain,
            vec![circuit_point(2, 4), circuit_point(3, 4)],
            EndpointTarget::Junction(fanout),
            gate_port(QBAR, GatePort::InputA),
        ),
        wire(
            domain,
            vec![circuit_point(2, 4), circuit_point(2, 10)],
            EndpointTarget::Junction(fanout),
            EndpointTarget::MobilePort(MobilePortRef {
                mobile: MOBILE,
                port: MobilePort::Stop,
            }),
        ),
        wire(
            domain,
            vec![
                circuit_point(5, -4),
                circuit_point(6, -4),
                circuit_point(6, 7),
                circuit_point(-2, 7),
                circuit_point(-2, 4),
                point(-CIRCUIT_PITCH, 7 * CIRCUIT_PITCH / 2),
            ],
            gate_port(Q, GatePort::Output),
            gate_port(or_qbar, GatePort::InputA),
        ),
        wire(
            domain,
            vec![
                circuit_point(5, 4),
                circuit_point(7, 4),
                circuit_point(7, -6),
                circuit_point(-2, -6),
                circuit_point(-2, -4),
                point(-CIRCUIT_PITCH, -7 * CIRCUIT_PITCH / 2),
            ],
            gate_port(QBAR, GatePort::Output),
            gate_port(or_q, GatePort::InputB),
        ),
        external(RESET_DRIVER, LogicLevel::High, ASSERTED),
    ]);

    for _ in 0..32 {
        if gate_is_quiescent(&recorder.simulation, Q, LogicLevel::Low)
            && gate_is_quiescent(&recorder.simulation, QBAR, LogicLevel::High)
            && mobile(&recorder.simulation).stop == LogicLevel::Low
        {
            break;
        }
        recorder.empty();
    }
    assert!(gate_is_quiescent(&recorder.simulation, Q, LogicLevel::Low));
    assert!(gate_is_quiescent(
        &recorder.simulation,
        QBAR,
        LogicLevel::High
    ));
    let release_startup = match design {
        Design::RetainedState => vec![external(RESET_DRIVER, LogicLevel::Low, RELEASED)],
        Design::CurrentInputOnly => vec![
            Command::RemoveEntity(RemoveEntityCommand {
                target: Q_INPUT_WIRE,
            }),
            Command::RemoveEntity(RemoveEntityCommand {
                target: Q_FEEDBACK_WIRE,
            }),
            Command::RemoveEntity(RemoveEntityCommand {
                target: QBAR_FEEDBACK_WIRE,
            }),
            // Acyclic reference design: SET drives OR(7), whose output directly fans out to
            // STOP and NOT(8). NOT(8) then feeds NOT(6), so Q/Qbar remain observable without
            // a live feedback edge or retained state at the ready checkpoint.
            wire(
                domain,
                vec![
                    circuit_point(5, 4),
                    circuit_point(7, 4),
                    circuit_point(7, -7),
                    circuit_point(2, -7),
                    circuit_point(2, -4),
                    circuit_point(3, -4),
                ],
                gate_port(QBAR, GatePort::Output),
                gate_port(Q, GatePort::InputA),
            ),
            external(RESET_DRIVER, LogicLevel::Low, RELEASED),
        ],
    };
    recorder.step(release_startup);
    while recorder.next_tick() < READY_TICK {
        recorder.empty();
    }
    let ready_tick = recorder.next_tick();
    assert_eq!(ready_tick, READY_TICK);
    assert!(gate_is_quiescent(&recorder.simulation, Q, LogicLevel::Low));
    assert!(gate_is_quiescent(
        &recorder.simulation,
        QBAR,
        LogicLevel::High
    ));
    assert_eq!(mobile(&recorder.simulation).stop, LogicLevel::Low);

    let b_position = aon_sim::TrackPosition::Edge {
        edge: EDGE_B,
        offset: Fixed(B_X - JUNCTION_X),
        heading: aon_sim::Heading::Forward,
    };
    for _ in 0..96 {
        if mobile(&recorder.simulation).track_position == b_position {
            break;
        }
        recorder.empty();
    }
    assert_eq!(mobile(&recorder.simulation).track_position, b_position);
    let pulse_tick = recorder.next_tick();
    assert_eq!(pulse_tick, PULSE_TICK);

    recorder.step(vec![external(SET_DRIVER, LogicLevel::High, ASSERTED)]);
    for _ in 0..32 {
        if gate_is_quiescent(&recorder.simulation, Q, LogicLevel::High)
            && gate_is_quiescent(&recorder.simulation, QBAR, LogicLevel::Low)
            && mobile(&recorder.simulation).stop == LogicLevel::High
        {
            break;
        }
        recorder.empty();
    }
    assert!(gate_is_quiescent(&recorder.simulation, Q, LogicLevel::High));
    assert!(gate_is_quiescent(
        &recorder.simulation,
        QBAR,
        LogicLevel::Low
    ));
    assert_eq!(mobile(&recorder.simulation).stop, LogicLevel::High);
    while recorder.next_tick() < FIRST_STOP_TICK {
        let stopped = mobile(&recorder.simulation);
        assert_eq!(stopped.stop, LogicLevel::High);
        recorder.empty();
    }
    let first_stop_tick = recorder.next_tick();
    assert_eq!(first_stop_tick, FIRST_STOP_TICK);
    let stopped_at = mobile(&recorder.simulation).track_position;

    for _ in 0..16 {
        recorder.empty();
        assert_eq!(mobile(&recorder.simulation).track_position, stopped_at);
    }
    let release_tick = recorder.next_tick();
    assert_eq!(release_tick, RELEASE_TICK);
    recorder.step(vec![external(SET_DRIVER, LogicLevel::Low, RELEASED)]);
    let released_tick = recorder.next_tick();
    assert_eq!(released_tick, RELEASE_TICK + 1);
    let mut resumed = false;
    for _ in 0..64 {
        recorder.empty();
        let mobile = mobile(&recorder.simulation);
        match design {
            Design::RetainedState => {
                assert_eq!(mobile.track_position, stopped_at);
                assert_eq!(mobile.stop, LogicLevel::High);
                assert!(gate_is_quiescent(&recorder.simulation, Q, LogicLevel::High));
                assert!(gate_is_quiescent(
                    &recorder.simulation,
                    QBAR,
                    LogicLevel::Low
                ));
            }
            Design::CurrentInputOnly => {
                if mobile.stop == LogicLevel::High {
                    assert_eq!(mobile.track_position, stopped_at);
                } else if mobile.track_position != stopped_at {
                    resumed = true;
                }
            }
        }
    }
    let final_tick = recorder.next_tick();
    assert_eq!(final_tick, FINAL_TICK);
    if design == Design::CurrentInputOnly {
        assert!(
            resumed,
            "current-input-only Mobile resumes after SET release"
        );
        assert_eq!(mobile(&recorder.simulation).stop, LogicLevel::Low);
        assert!(gate_is_quiescent(&recorder.simulation, Q, LogicLevel::Low));
        assert!(gate_is_quiescent(
            &recorder.simulation,
            QBAR,
            LogicLevel::High
        ));
    }

    let checkpoint_ticks = vec![
        0,
        ready_tick,
        pulse_tick,
        first_stop_tick,
        release_tick,
        released_tick,
        final_tick,
    ];
    let checkpoints = checkpoint_ticks
        .iter()
        .map(|&tick| recorder.checkpoint(tick))
        .collect();
    let initial = Simulation::new(package()).expect("reference simulation starts");
    let replay = Replay::new_v2(
        initial.replay_header(),
        recorder.commands,
        Vec::new(),
        checkpoints,
    )
    .expect("retained mobility Replay is valid");
    let artifact =
        ReplayArtifact::new("../scenarios/empty.json", replay).expect("Scenario locator is valid");
    (artifact, checkpoint_ticks)
}

fn main() {
    let write = std::env::args().any(|argument| argument == "--write");
    for design in [Design::RetainedState, Design::CurrentInputOnly] {
        let (artifact, checkpoint_ticks) = record_fixture(design);
        let bytes = encode_replay_artifact(&artifact).expect("mobility Replay encodes");
        let path = Path::new(design.fixture_path());
        if write {
            std::fs::write(path, bytes).expect("mobility Replay fixture writes");
            println!(
                "wrote {} with checkpoints {checkpoint_ticks:?}",
                path.display()
            );
        } else {
            println!("{} checkpoints {checkpoint_ticks:?}", path.display());
            println!("{}", String::from_utf8(bytes).expect("Replay is UTF-8"));
        }
    }
}
