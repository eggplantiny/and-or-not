use aon_sim::{
    ArtifactBytes, Command, CommandEnvelope, DemandId, DemandKind, DriveStrength, EndpointTarget,
    Energy, EntityId, FIXED_ONE, Fixed, FixedAabb, FixedVec2, GateId, GatePort, GatePortRef,
    GateType, HashCheckpoint, HostileCollider, LogicLevel, MobileId, PlaceFixedSubstrateCommand,
    PlaceGateCommand, PlaceMobileSubstrateCommand, PlaceWireCommand, PowerRatio, PowerSourceId,
    Replay, ReplayArtifact, Revision, RoutingDomain, Simulation, StateHash, StepReport, Tick,
    WireEnd, WireId, WireSensePortRef, WorldInputEvent, decode_balance_profile,
    decode_numeric_profile, decode_package, decode_physical_scale_profile,
    decode_scenario_manifest, encode_replay_artifact, scale_work,
};
use serde_json::json;
use std::path::Path;

const NUMERIC: &[u8] = include_bytes!("../../../profiles/numeric/v1.json");
const PHYSICAL: &[u8] = include_bytes!("../../../profiles/physical-scale/stage0-alpha.json");
const BALANCE: &[u8] = include_bytes!("../../../profiles/balance/s1-m2-power-probe-alpha.json");

const WU: i64 = FIXED_ONE;
const CIRCUIT_PITCH: i64 = 16_384;
const CORE_POSITION: FixedVec2 = wu(-8, -8);
const SUBSTRATE: EntityId = EntityId(3);
const SOURCE: PowerSourceId = PowerSourceId(EntityId(2));

const C07_SCENARIO_ID: &str = "s1-m2-c07-sensing-v1";
const C07_SCENARIO_PATH: &str = "fixtures/scenarios/s1-m2-c07-sensing-v1.json";
const C07_REPLAY_PATH: &str = "fixtures/replays/s1-m2-c07-sensing-v1.json";
const C07_SOURCE_POSITION: FixedVec2 = wu(-4, 0);
const C07_SOURCE_GENERATION: u64 = 16;
const C07_SENSED_WIRE: WireId = WireId(EntityId(4));
const C07_PROBE_A: GateId = GateId(EntityId(5));
const C07_PROBE_B: GateId = GateId(EntityId(6));

const C08_FULL_SCENARIO_ID: &str = "s1-m2-c08-brownout-full-v1";
const C08_FULL_SCENARIO_PATH: &str = "fixtures/scenarios/s1-m2-c08-brownout-full-v1.json";
const C08_FULL_REPLAY_PATH: &str = "fixtures/replays/s1-m2-c08-brownout-full-v1.json";
const C08_HALF_SCENARIO_ID: &str = "s1-m2-c08-brownout-half-v1";
const C08_HALF_SCENARIO_PATH: &str = "fixtures/scenarios/s1-m2-c08-brownout-half-v1.json";
const C08_HALF_REPLAY_PATH: &str = "fixtures/replays/s1-m2-c08-brownout-half-v1.json";
const C08_SOURCE_POSITION: FixedVec2 = wu(0, 0);
const C08_FULL_GENERATION: u64 = 51;
const C08_HALF_GENERATION: u64 = 24;
const C08_TRACK_WIRE: WireId = WireId(EntityId(4));
const C08_MOBILE: MobileId = MobileId(EntityId(5));
const C08_GATE: GateId = GateId(EntityId(6));
const C08_SENSED_WIRE: WireId = WireId(EntityId(8));

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

const fn wu(x: i64, y: i64) -> FixedVec2 {
    point(x * WU, y * WU)
}

const fn gate_port(gate: GateId, port: GatePort) -> EndpointTarget {
    EndpointTarget::GatePort(GatePortRef { gate, port })
}

fn wire(
    routing_domain: RoutingDomain,
    points: Vec<FixedVec2>,
    endpoint_a: EndpointTarget,
    endpoint_b: EndpointTarget,
) -> Command {
    Command::PlaceWire(PlaceWireCommand {
        routing_domain,
        points,
        endpoint_a,
        endpoint_b,
    })
}

#[derive(Clone, Copy)]
struct ScenarioSpec {
    id: &'static str,
    source_position: FixedVec2,
    generation: u64,
    mobility: bool,
}

fn scenario_bytes(spec: ScenarioSpec) -> Vec<u8> {
    let numeric = decode_numeric_profile(NUMERIC).expect("the retained Numeric Profile decodes");
    let physical = decode_physical_scale_profile(PHYSICAL)
        .expect("the retained Physical Scale Profile decodes");
    let balance = decode_balance_profile(BALANCE).expect("the S1-M2 Power Balance Profile decodes");
    let mut bytes = serde_json::to_vec_pretty(&json!({
        "schemaVersion": 3,
        "scenarioId": spec.id,
        "semanticsVersion": "aon-semantics-v1",
        "hashAlgorithm": "blake3-v1",
        "initialWorld": {
            "kind": "main-core-power-v1",
            "mainCore": {
                "position": { "x": CORE_POSITION.x.0, "y": CORE_POSITION.y.0 },
                "integrity": 1000,
                "heatEnergy": 0
            },
            "powerSources": [{
                "position": {
                    "x": spec.source_position.x.0,
                    "y": spec.source_position.y.0
                },
                "generationPerTick": spec.generation
            }]
        },
        "requiredFeatures": {
            "signal": true,
            "mobility": spec.mobility,
            "capacity": true,
            "sensing": true,
            "power": true,
            "relay": false,
            "payload": false,
            "radiation": false
        },
        "profiles": {
            "numeric": {
                "path": "../../profiles/numeric/v1.json",
                "profileId": numeric.profile_id,
                "profileHash": numeric.canonical_hash().expect("Numeric Profile hashes").to_string()
            },
            "physicalScale": {
                "path": "../../profiles/physical-scale/stage0-alpha.json",
                "profileId": physical.profile_id,
                "profileHash": physical.canonical_hash().expect("Physical Profile hashes").to_string()
            },
            "balance": {
                "path": "../../profiles/balance/s1-m2-power-probe-alpha.json",
                "profileId": balance.profile_id,
                "profileHash": balance.canonical_hash().expect("Balance Profile hashes").to_string()
            }
        }
    }))
    .expect("the S1-M2 Scenario JSON encodes");
    bytes.push(b'\n');
    bytes
}

fn package(scenario: &[u8]) -> aon_sim::SimulationPackage {
    decode_package(ArtifactBytes {
        scenario,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("the generated S1-M2 package decodes")
}

struct Recorder {
    simulation: Simulation,
    header: aon_sim::ReplayHeader,
    commands: Vec<CommandEnvelope>,
    world_inputs: Vec<WorldInputEvent>,
    trace: Vec<StateHash>,
}

impl Recorder {
    fn new(scenario: &[u8]) -> Self {
        let simulation = Simulation::new(package(scenario)).expect("the S1-M2 Simulation starts");
        let header = simulation.replay_header();
        let trace = vec![simulation.state_hash()];
        Self {
            simulation,
            header,
            commands: Vec::new(),
            world_inputs: Vec::new(),
            trace,
        }
    }

    fn step(&mut self, commands: Vec<Command>, world_input: Option<WorldInputEvent>) -> StepReport {
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
        let inputs = world_input.iter().cloned().collect::<Vec<_>>();
        let report = self
            .simulation
            .step_with_world_inputs(&envelopes, &inputs)
            .expect("the retained S1-M2 Tick succeeds");
        assert!(
            report.command_rejections.is_empty(),
            "retained commands are accepted at {target_tick}: {:?}",
            report.command_rejections
        );
        self.commands.extend(envelopes);
        self.world_inputs.extend(world_input);
        self.trace.push(report.state_hash);
        report
    }

    fn finish(mut self, scenario_path: &'static str) -> ReplayArtifact {
        // The generator deliberately supplies the occupied hostile frame in reverse ID order.
        // Replay v2 canonicalization owns the retained byte order; execution above used the
        // already-normalized complete frame required by Simulation::step_with_world_inputs.
        for input in &mut self.world_inputs {
            let WorldInputEvent::HostileFrame { hostiles, .. } = input;
            if hostiles.len() == 3 {
                hostiles.reverse();
            }
        }
        let checkpoints = self
            .trace
            .iter()
            .copied()
            .enumerate()
            .map(|(next_tick, state_hash)| HashCheckpoint {
                next_tick: Tick(u64::try_from(next_tick).expect("fixture Tick fits u64")),
                state_hash,
            })
            .collect();
        ReplayArtifact::new(
            scenario_path,
            Replay::new_v2(self.header, self.commands, self.world_inputs, checkpoints)
                .expect("the retained S1-M2 Replay normalizes"),
        )
        .expect("the retained Scenario locator is portable")
    }
}

fn substrate() -> Command {
    let bounds = FixedAabb::new(wu(-8, -8), wu(8, 8));
    Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
        origin: wu(0, 0),
        routing_area: bounds,
        footprint: bounds,
    })
}

fn hostile(id: u64, x: i64) -> HostileCollider {
    HostileCollider {
        id,
        center: point(x, 0),
        radius: Fixed::ZERO,
    }
}

fn hostile_frame(target_tick: u64, hostiles: Vec<HostileCollider>) -> WorldInputEvent {
    WorldInputEvent::HostileFrame {
        target_tick: Tick(target_tick),
        hostiles,
    }
}

fn c07_commands() -> Vec<Command> {
    let domain = RoutingDomain::FixedSubstrate(SUBSTRATE);
    vec![
        substrate(),
        wire(
            domain,
            vec![C07_SOURCE_POSITION, wu(4, 0)],
            EndpointTarget::PowerSourceAnchor(SOURCE),
            EndpointTarget::Free,
        ),
        Command::PlaceGate(PlaceGateCommand {
            gate_type: GateType::Not,
            origin: point(-4 * WU + CIRCUIT_PITCH, 4 * WU),
            routing_domain: domain,
        }),
        Command::PlaceGate(PlaceGateCommand {
            gate_type: GateType::Not,
            origin: point(4 * WU + CIRCUIT_PITCH, 4 * WU),
            routing_domain: domain,
        }),
        wire(
            domain,
            vec![
                wu(-4, 0),
                wu(-5, 0),
                wu(-5, 3),
                wu(-6, 3),
                wu(-6, 4),
                wu(-4, 4),
            ],
            EndpointTarget::WireSensePort(WireSensePortRef {
                wire: C07_SENSED_WIRE,
                end: WireEnd::A,
            }),
            gate_port(C07_PROBE_A, GatePort::InputA),
        ),
        wire(
            domain,
            vec![wu(4, 0), wu(5, 0), wu(5, 3), wu(3, 3), wu(3, 4), wu(4, 4)],
            EndpointTarget::WireSensePort(WireSensePortRef {
                wire: C07_SENSED_WIRE,
                end: WireEnd::B,
            }),
            gate_port(C07_PROBE_B, GatePort::InputA),
        ),
    ]
}

fn assert_c07_driver_pair(
    simulation: &Simulation,
    level: LogicLevel,
    revision: Revision,
    emitted_at: Tick,
) {
    let sense = simulation
        .wire_sense_state(C07_SENSED_WIRE)
        .expect("the straight retained Wire exposes Sense A/B");
    for driver in [sense.ports.a, sense.ports.b] {
        let sample = simulation
            .driver_sample(driver)
            .expect("the retained Sense Driver exists");
        assert_eq!(sample.level, level);
        assert_eq!(sample.strength, DriveStrength(400));
        assert_eq!(sample.revision, revision);
        assert_eq!(sample.emitted_at, emitted_at);
    }
}

fn record_c07(scenario: &[u8]) -> ReplayArtifact {
    let mut recorder = Recorder::new(scenario);
    let mut construction = c07_commands().into_iter();
    let substrate = construction.next().expect("C07 has a substrate command");
    let first = recorder.step(vec![substrate], None);
    assert_eq!(first.command_acceptances.len(), 1);
    let second = recorder.step(construction.by_ref().take(3).collect(), None);
    assert_eq!(second.command_acceptances.len(), 3);
    let power = second.power.expect("C07 emits a Power report");
    for kind in [DemandKind::WireLeakage, DemandKind::WireSensing] {
        let load = power
            .load(DemandId::new(C07_SENSED_WIRE.entity_id(), kind))
            .expect("the powered sensed Wire load is reported");
        assert_eq!(load.ratio, PowerRatio::ONE);
        assert_eq!(load.nominal, Energy(8));
        assert_eq!(load.granted, Energy(8));
    }
    let sense = recorder
        .simulation
        .wire_sense_state(C07_SENSED_WIRE)
        .expect("the retained straight Wire is sensed");
    assert!(!sense.sampled_presence);
    assert_eq!(sense.intended_level, LogicLevel::Low);
    assert_eq!(sense.intended_strength, DriveStrength(400));

    let probes = recorder.step(construction.collect(), None);
    assert_eq!(probes.command_acceptances.len(), 2);
    assert_c07_driver_pair(&recorder.simulation, LogicLevel::Low, Revision(1), Tick(2));

    recorder.step(Vec::new(), Some(hostile_frame(3, Vec::new())));
    assert_c07_driver_pair(&recorder.simulation, LogicLevel::Low, Revision(1), Tick(2));

    let occupied = vec![hostile(1, -WU / 2), hostile(2, 0), hostile(3, WU / 2)];
    recorder.step(Vec::new(), Some(hostile_frame(4, occupied)));
    let sense = recorder
        .simulation
        .wire_sense_state(C07_SENSED_WIRE)
        .expect("the retained straight Wire remains sensed");
    assert!(sense.sampled_presence);
    assert_eq!(sense.intended_level, LogicLevel::High);
    assert_c07_driver_pair(&recorder.simulation, LogicLevel::Low, Revision(1), Tick(2));

    recorder.step(Vec::new(), Some(hostile_frame(5, Vec::new())));
    let sense = recorder
        .simulation
        .wire_sense_state(C07_SENSED_WIRE)
        .expect("the retained straight Wire remains sensed");
    assert!(!sense.sampled_presence);
    assert_eq!(sense.intended_level, LogicLevel::Low);
    assert_c07_driver_pair(&recorder.simulation, LogicLevel::High, Revision(2), Tick(5));

    let driver_low = recorder.step(Vec::new(), None);
    assert_c07_driver_pair(&recorder.simulation, LogicLevel::Low, Revision(3), Tick(6));
    assert!(driver_low.signal_changes.is_empty());
    let in_flight = recorder.step(Vec::new(), None);
    assert!(in_flight.signal_changes.is_empty());
    let high = recorder.step(Vec::new(), None);
    assert_eq!(high.signal_changes.len(), 2);
    let ports_a = recorder
        .simulation
        .gate_signal_ports(C07_PROBE_A)
        .expect("probe A remains live");
    let ports_b = recorder
        .simulation
        .gate_signal_ports(C07_PROBE_B)
        .expect("probe B remains live");
    let sense_ports = recorder
        .simulation
        .wire_sense_state(C07_SENSED_WIRE)
        .expect("Sense A/B remain live")
        .ports;
    for (sink, driver) in [
        (ports_a.input_a.sink, sense_ports.a),
        (ports_b.input_a.sink, sense_ports.b),
    ] {
        let sample = recorder
            .simulation
            .sink_driver_sample(sink, driver)
            .expect("the delayed HIGH reached its independent probe");
        assert_eq!(sample.level, LogicLevel::High);
        assert_eq!(sample.strength, DriveStrength(400));
        assert_eq!(sample.revision, Revision(2));
        assert_eq!(recorder.simulation.sink_level(sink), Some(LogicLevel::High));
    }

    let low = recorder.step(Vec::new(), None);
    assert_eq!(low.signal_changes.len(), 2);
    for (sink, driver) in [
        (ports_a.input_a.sink, sense_ports.a),
        (ports_b.input_a.sink, sense_ports.b),
    ] {
        let sample = recorder
            .simulation
            .sink_driver_sample(sink, driver)
            .expect("the delayed LOW reached its independent probe");
        assert_eq!(sample.level, LogicLevel::Low);
        assert_eq!(sample.strength, DriveStrength(400));
        assert_eq!(sample.revision, Revision(3));
        assert_eq!(recorder.simulation.sink_level(sink), Some(LogicLevel::Low));
    }

    recorder.finish("../scenarios/s1-m2-c07-sensing-v1.json")
}

fn c08_commands() -> Vec<Command> {
    let fixed = RoutingDomain::FixedSubstrate(SUBSTRATE);
    let mobile_bounds = FixedAabb::new(wu(-2, -2), wu(2, 2));
    vec![
        substrate(),
        wire(
            RoutingDomain::OpenWorld,
            vec![C08_SOURCE_POSITION, wu(16, 0)],
            EndpointTarget::PowerSourceAnchor(SOURCE),
            EndpointTarget::Free,
        ),
        Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
            origin: wu(4, 0),
            routing_area: mobile_bounds,
            footprint: mobile_bounds,
        }),
        Command::PlaceGate(PlaceGateCommand {
            gate_type: GateType::Not,
            origin: wu(0, 4),
            routing_domain: fixed,
        }),
        wire(
            fixed,
            vec![C08_SOURCE_POSITION, point(0, 4 * WU - CIRCUIT_PITCH)],
            EndpointTarget::PowerSourceAnchor(SOURCE),
            gate_port(C08_GATE, GatePort::Power),
        ),
        wire(
            fixed,
            vec![C08_SOURCE_POSITION, wu(-4, 0)],
            EndpointTarget::PowerSourceAnchor(SOURCE),
            EndpointTarget::Free,
        ),
    ]
}

fn record_c08(
    scenario: &[u8],
    generation: u64,
    expected_ratio: PowerRatio,
    expected_delay: Tick,
    expected_strength: DriveStrength,
    expected_movement: Fixed,
    scenario_path: &'static str,
) -> ReplayArtifact {
    let mut recorder = Recorder::new(scenario);
    let mut construction = c08_commands().into_iter();
    let first = recorder.step(construction.by_ref().take(2).collect(), None);
    assert_eq!(first.command_acceptances.len(), 2);
    let second = recorder.step(construction.by_ref().take(2).collect(), None);
    assert_eq!(second.command_acceptances.len(), 2);
    let evidence = recorder.step(construction.collect(), None);
    assert_eq!(evidence.command_acceptances.len(), 2);
    let power = evidence.power.as_ref().expect("C08 emits a Power report");
    assert_eq!(power.regions.len(), 1);
    let region = &power.regions[0];
    assert_eq!(region.generation, Energy(generation));
    assert_eq!(region.total_nominal_demand, Energy(51));
    assert_eq!(region.ratio, expected_ratio);
    assert_eq!(power.loads.len(), 9);
    assert!(power.loads.iter().all(|load| load.ratio == expected_ratio));
    assert!(
        power
            .loads
            .iter()
            .all(|load| load.transmission_loss == Energy(0))
    );
    for demand in [
        DemandId::new(C08_GATE.entity_id(), DemandKind::GateIdle),
        DemandId::new(C08_GATE.entity_id(), DemandKind::GateSwitch),
        DemandId::new(C08_SENSED_WIRE.entity_id(), DemandKind::WireSensing),
        DemandId::new(C08_MOBILE.entity_id(), DemandKind::Movement),
    ] {
        assert_eq!(
            power
                .load(demand)
                .expect("every C08 behavior has a real runtime load")
                .ratio,
            expected_ratio
        );
    }
    assert_eq!(
        scale_work(Energy(8), expected_ratio),
        Ok(if expected_ratio == PowerRatio::ONE {
            Energy(8)
        } else {
            Energy(4)
        })
    );
    let movement = &evidence.mobile_movements[0];
    assert_eq!(movement.mobile, C08_MOBILE);
    assert_eq!(movement.granted_budget, expected_movement);
    assert_eq!(movement.consumed_budget, expected_movement);
    let gate = recorder
        .simulation
        .gate_signal_state(C08_GATE)
        .expect("the C08 Gate remains live");
    assert_eq!(gate.current_output, LogicLevel::Low);
    assert_eq!(gate.desired_output, LogicLevel::High);
    assert_eq!(gate.pending_due_tick, Some(expected_delay));
    assert_eq!(gate.pending_level, Some(LogicLevel::High));
    let sense = recorder
        .simulation
        .wire_sense_state(C08_SENSED_WIRE)
        .expect("the C08 sensed Wire remains live");
    assert_eq!(sense.intended_strength, expected_strength);
    assert_eq!(
        recorder
            .simulation
            .power_source_state(SOURCE)
            .expect("the Scenario Source remains live")
            .generation_per_tick(),
        Energy(generation)
    );
    let aon_sim::TrackPosition::Edge { edge, .. } = movement.start else {
        panic!("movement starts on the retained Track Wire");
    };
    assert_eq!(edge, C08_TRACK_WIRE);

    let after_one = recorder.step(Vec::new(), None);
    assert_eq!(
        after_one.mobile_movements[0].granted_budget,
        expected_movement
    );
    let sense = recorder
        .simulation
        .wire_sense_state(C08_SENSED_WIRE)
        .expect("the C08 sensed Wire remains live");
    for driver in [sense.ports.a, sense.ports.b] {
        let sample = recorder
            .simulation
            .driver_sample(driver)
            .expect("the C08 Sense Driver remains live");
        assert_eq!(sample.level, LogicLevel::Low);
        assert_eq!(sample.strength, expected_strength);
        assert_eq!(sample.revision, Revision(1));
    }

    let after_two = recorder.step(Vec::new(), None);
    assert_eq!(
        after_two.mobile_movements[0].granted_budget,
        expected_movement
    );
    let gate = recorder
        .simulation
        .gate_signal_state(C08_GATE)
        .expect("the C08 Gate remains live");
    assert_eq!(gate.current_output, LogicLevel::High);
    assert!(gate.pending_due_tick.is_none());
    let output = recorder
        .simulation
        .driver_sample(gate.ports.output)
        .expect("the C08 Gate output remains live");
    assert_eq!(output.level, LogicLevel::High);
    assert_eq!(output.strength, expected_strength);

    recorder.finish(scenario_path)
}

fn write_or_print(path: &Path, bytes: &[u8], write: bool) {
    if write {
        std::fs::write(path, bytes).expect("the retained fixture writes");
        println!("wrote {}", path.display());
    } else {
        println!("{}", path.display());
        println!("{}", String::from_utf8_lossy(bytes));
    }
}

fn emit_pair(
    scenario_path: &'static str,
    replay_path: &'static str,
    scenario: &[u8],
    artifact: &ReplayArtifact,
    write: bool,
) {
    let replay = encode_replay_artifact(artifact).expect("the retained Replay encodes");
    let scenario_hash = decode_scenario_manifest(scenario)
        .expect("the generated Scenario strictly decodes")
        .canonical_hash()
        .expect("the generated Scenario hashes");
    println!(
        "{} scenarioHash={} initialHash={} finalHash={}",
        artifact.replay().header().world_generator_version,
        scenario_hash,
        artifact.replay().header().initial_state_hash,
        artifact
            .replay()
            .checkpoints()
            .last()
            .expect("retained Replay has a final checkpoint")
            .state_hash
    );
    write_or_print(Path::new(scenario_path), scenario, write);
    write_or_print(Path::new(replay_path), &replay, write);
}

fn main() {
    let write = std::env::args().any(|argument| argument == "--write");

    let c07_scenario = scenario_bytes(ScenarioSpec {
        id: C07_SCENARIO_ID,
        source_position: C07_SOURCE_POSITION,
        generation: C07_SOURCE_GENERATION,
        mobility: false,
    });
    let c07 = record_c07(&c07_scenario);
    emit_pair(
        C07_SCENARIO_PATH,
        C07_REPLAY_PATH,
        &c07_scenario,
        &c07,
        write,
    );

    let c08_full_scenario = scenario_bytes(ScenarioSpec {
        id: C08_FULL_SCENARIO_ID,
        source_position: C08_SOURCE_POSITION,
        generation: C08_FULL_GENERATION,
        mobility: true,
    });
    let c08_full = record_c08(
        &c08_full_scenario,
        C08_FULL_GENERATION,
        PowerRatio::ONE,
        Tick(3),
        DriveStrength(400),
        Fixed(WU),
        "../scenarios/s1-m2-c08-brownout-full-v1.json",
    );
    emit_pair(
        C08_FULL_SCENARIO_PATH,
        C08_FULL_REPLAY_PATH,
        &c08_full_scenario,
        &c08_full,
        write,
    );

    let c08_half_scenario = scenario_bytes(ScenarioSpec {
        id: C08_HALF_SCENARIO_ID,
        source_position: C08_SOURCE_POSITION,
        generation: C08_HALF_GENERATION,
        mobility: true,
    });
    let c08_half = record_c08(
        &c08_half_scenario,
        C08_HALF_GENERATION,
        PowerRatio::new(Fixed(WU / 2)).expect("one half is a valid PowerRatio"),
        Tick(4),
        DriveStrength(200),
        Fixed(WU / 2),
        "../scenarios/s1-m2-c08-brownout-half-v1.json",
    );
    emit_pair(
        C08_HALF_SCENARIO_PATH,
        C08_HALF_REPLAY_PATH,
        &c08_half_scenario,
        &c08_half,
        write,
    );
}
