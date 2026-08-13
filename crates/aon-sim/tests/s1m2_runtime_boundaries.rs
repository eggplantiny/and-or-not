use aon_sim::{
    ArtifactBytes, Command, CommandEnvelope, DemandId, DemandKind, DriveStrength, EndpointTarget,
    EntityId, FIXED_ONE, Fixed, FixedAabb, FixedVec2, GateId, GatePort, GatePortRef, GateType,
    LogicLevel, PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceWireCommand, PowerRatio,
    PowerSourceId, RoutingDomain, SetExternalDriverCommand, Simulation, StageFeatureSet, Tick,
    decode_balance_profile, decode_numeric_profile, decode_package, decode_physical_scale_profile,
    scale_drive,
};
use serde_json::{Value, json};

const NUMERIC: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/numeric/v1.json"
));
const PHYSICAL: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/physical-scale/stage0-alpha.json"
));
const BASE_BALANCE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/balance/s1-m2-power-probe-alpha.json"
));

const WU: i64 = FIXED_ONE;
const CIRCUIT_PITCH: i64 = 16_384;
const SOURCE: PowerSourceId = PowerSourceId(EntityId(2));
const SUBSTRATE: EntityId = EntityId(3);
const GATE: GateId = GateId(EntityId(4));

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

const fn wu(x: i64, y: i64) -> FixedVec2 {
    point(x * WU, y * WU)
}

fn balance_bytes() -> Vec<u8> {
    let mut balance: Value =
        serde_json::from_slice(BASE_BALANCE).expect("the retained S1-M2 Balance is JSON");
    balance["profileId"] = "balance-s1-m2-runtime-boundaries".into();
    balance["gateBaseDelay"] = 4.into();
    balance["fanoutFreeLoad"] = 1_000.into();
    serde_json::to_vec(&balance).expect("the focused Balance serializes")
}

fn scenario_bytes(balance: &[u8]) -> Vec<u8> {
    let numeric = decode_numeric_profile(NUMERIC).expect("the Numeric Profile decodes");
    let physical =
        decode_physical_scale_profile(PHYSICAL).expect("the Physical Scale Profile decodes");
    let balance = decode_balance_profile(balance).expect("the focused Balance Profile decodes");
    let features = StageFeatureSet {
        signal: true,
        capacity: true,
        sensing: true,
        power: true,
        ..StageFeatureSet::none()
    };

    serde_json::to_vec(&json!({
        "schemaVersion": 3,
        "scenarioId": "s1-m2-runtime-boundaries",
        "semanticsVersion": "aon-semantics-v1",
        "hashAlgorithm": "blake3-v1",
        "initialWorld": {
            "kind": "main-core-power-v1",
            "mainCore": {
                "position": { "x": -16 * WU, "y": -16 * WU },
                "integrity": 1_000,
                "heatEnergy": 0
            },
            "powerSources": [{
                "position": { "x": 0, "y": 0 },
                "generationPerTick": 10
            }]
        },
        "requiredFeatures": {
            "signal": features.signal,
            "mobility": features.mobility,
            "capacity": features.capacity,
            "sensing": features.sensing,
            "power": features.power,
            "relay": features.relay,
            "payload": features.payload,
            "radiation": features.radiation
        },
        "profiles": {
            "numeric": {
                "path": "profiles/numeric/v1.json",
                "profileId": numeric.profile_id,
                "profileHash": numeric.canonical_hash().expect("Numeric Profile hashes").to_string()
            },
            "physicalScale": {
                "path": "profiles/physical-scale/stage0-alpha.json",
                "profileId": physical.profile_id,
                "profileHash": physical.canonical_hash().expect("Physical Profile hashes").to_string()
            },
            "balance": {
                "path": "profiles/balance/s1-m2-runtime-boundaries.json",
                "profileId": balance.profile_id,
                "profileHash": balance.canonical_hash().expect("Balance Profile hashes").to_string()
            }
        }
    }))
    .expect("the focused Scenario serializes")
}

fn simulation() -> Simulation {
    let balance = balance_bytes();
    let scenario = scenario_bytes(&balance);
    let package = decode_package(ArtifactBytes {
        scenario: &scenario,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: &balance,
    })
    .expect("the focused S1-M2 package decodes");
    Simulation::new(package).expect("the focused S1-M2 Simulation starts")
}

fn step(simulation: &mut Simulation, commands: Vec<Command>) -> aon_sim::StepReport {
    let target_tick = simulation.next_tick();
    let envelopes = commands
        .into_iter()
        .enumerate()
        .map(|(ordinal, command)| CommandEnvelope {
            target_tick,
            ordinal: u64::try_from(ordinal).expect("test ordinal fits u64"),
            command,
        })
        .collect::<Vec<_>>();
    let report = simulation.step(&envelopes).expect("focused Tick succeeds");
    assert_eq!(
        report.command_acceptances.len(),
        envelopes.len(),
        "every focused command is accepted at {target_tick}: {:?}",
        report.command_rejections
    );
    assert!(report.command_rejections.is_empty());
    report
}

fn substrate() -> Command {
    let bounds = FixedAabb::new(wu(-128, -128), wu(128, 128));
    Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
        origin: wu(0, 0),
        routing_area: bounds,
        footprint: bounds,
    })
}

fn gate() -> Command {
    Command::PlaceGate(PlaceGateCommand {
        gate_type: GateType::Not,
        origin: wu(0, 2),
        routing_domain: RoutingDomain::FixedSubstrate(SUBSTRATE),
    })
}

fn source_wire(end: FixedVec2, endpoint_b: EndpointTarget) -> Command {
    Command::PlaceWire(PlaceWireCommand {
        routing_domain: RoutingDomain::FixedSubstrate(SUBSTRATE),
        points: vec![wu(0, 0), end],
        endpoint_a: EndpointTarget::PowerSourceAnchor(SOURCE),
        endpoint_b,
    })
}

fn power_gate(simulation: &mut Simulation) {
    let substrate_report = step(simulation, vec![substrate()]);
    assert_eq!(
        substrate_report.command_acceptances[0].created_entity,
        Some(SUBSTRATE)
    );
    let gate_report = step(simulation, vec![gate()]);
    assert_eq!(
        gate_report.command_acceptances[0].created_entity,
        Some(GATE.entity_id())
    );
    let power_port = point(0, 2 * WU - CIRCUIT_PITCH);
    let power_report = step(
        simulation,
        vec![source_wire(
            power_port,
            EndpointTarget::GatePort(GatePortRef {
                gate: GATE,
                port: GatePort::Power,
            }),
        )],
    );
    assert_eq!(
        power_report.command_acceptances[0].created_entity,
        Some(EntityId(5))
    );
}

fn gate_ratio(report: &aon_sim::StepReport) -> PowerRatio {
    report
        .power
        .as_ref()
        .expect("Power report exists")
        .load(DemandId::new(GATE.entity_id(), DemandKind::GateIdle))
        .expect("GateIdle load exists")
        .ratio
}

fn output_sample(simulation: &Simulation) -> aon_sim::DriverSample {
    let output = simulation
        .gate_signal_ports(GATE)
        .expect("focused Gate remains live")
        .output;
    simulation
        .driver_sample(output)
        .expect("focused Gate output remains live")
}

#[test]
fn pending_logic_due_is_frozen_while_rho_strength_responds_at_t_plus_one_and_same_due_merges() {
    let mut simulation = simulation();
    power_gate(&mut simulation);
    let scheduled = simulation
        .gate_signal_state(GATE)
        .expect("powered Gate schedules its initial NOT transition");
    assert_eq!(scheduled.current_output, LogicLevel::Low);
    assert_eq!(scheduled.desired_output, LogicLevel::High);
    assert_eq!(scheduled.pending_due_tick, Some(Tick(6)));
    assert_eq!(scheduled.pending_level, Some(LogicLevel::High));
    let frozen_generation = scheduled.pending_generation;

    let first_drop = step(
        &mut simulation,
        vec![source_wire(wu(4, 0), EndpointTarget::Free)],
    );
    let first_ratio = gate_ratio(&first_drop);
    assert!(first_ratio < PowerRatio::ONE);
    let first_strength = scale_drive(DriveStrength(400), first_ratio)
        .expect("first rho scales the nominal Gate drive");
    assert_eq!(output_sample(&simulation).strength, DriveStrength(400));
    let after_first_drop = simulation
        .gate_signal_state(GATE)
        .expect("pending Gate remains live");
    assert_eq!(after_first_drop.pending_due_tick, Some(Tick(6)));
    assert_eq!(after_first_drop.pending_generation, frozen_generation);

    let first_response = step(&mut simulation, Vec::new());
    assert_eq!(first_response.completed_tick, Tick(4));
    assert_eq!(output_sample(&simulation).level, LogicLevel::Low);
    assert_eq!(output_sample(&simulation).strength, first_strength);
    assert_eq!(
        simulation
            .gate_signal_state(GATE)
            .expect("pending Gate remains live")
            .pending_due_tick,
        Some(Tick(6))
    );

    let second_drop = step(
        &mut simulation,
        vec![source_wire(wu(-8, 0), EndpointTarget::Free)],
    );
    assert_eq!(second_drop.completed_tick, Tick(5));
    let second_ratio = gate_ratio(&second_drop);
    assert!(second_ratio < first_ratio);
    assert!(
        second_ratio >= PowerRatio::new(Fixed(FIXED_ONE / 5)).expect("one fifth is a valid ratio")
    );
    let second_strength = scale_drive(DriveStrength(400), second_ratio)
        .expect("second rho scales the nominal Gate drive");
    assert!(second_strength < first_strength);
    assert_eq!(output_sample(&simulation).strength, first_strength);
    let before_same_due = simulation
        .gate_signal_state(GATE)
        .expect("pending Gate remains live");
    assert_eq!(before_same_due.pending_due_tick, Some(Tick(6)));
    assert_eq!(before_same_due.pending_generation, frozen_generation);

    let merged = step(&mut simulation, Vec::new());
    assert_eq!(merged.completed_tick, Tick(6));
    let output = simulation
        .gate_signal_ports(GATE)
        .expect("focused Gate remains live")
        .output;
    let changes = merged
        .driver_changes
        .iter()
        .filter(|change| change.driver == output)
        .collect::<Vec<_>>();
    assert_eq!(changes.len(), 1, "same-due level and strength merge once");
    assert_eq!(changes[0].previous.level, LogicLevel::Low);
    assert_eq!(changes[0].previous.strength, first_strength);
    assert_eq!(changes[0].current.level, LogicLevel::High);
    assert_eq!(changes[0].current.strength, second_strength);
    let settled = simulation
        .gate_signal_state(GATE)
        .expect("focused Gate remains live");
    assert_eq!(settled.current_output, LogicLevel::High);
    assert_eq!(settled.pending_due_tick, None);
    assert_eq!(settled.pending_level, None);
}

#[test]
fn under_threshold_desired_reversion_cancels_conflicting_pending_without_rollback() {
    let mut simulation = simulation();
    power_gate(&mut simulation);
    let before = simulation
        .gate_signal_state(GATE)
        .expect("powered Gate has a pending transition");
    assert_eq!(before.current_output, LogicLevel::Low);
    assert_eq!(before.desired_output, LogicLevel::High);
    assert_eq!(before.pending_due_tick, Some(Tick(6)));
    let reserved_energy = before
        .pending_switch_energy
        .expect("ordinary pending transition reserves switching Energy");
    let generation = before.pending_generation;
    let hash_before = simulation.state_hash();
    let input = simulation
        .gate_signal_ports(GATE)
        .expect("focused Gate remains live")
        .input_a
        .external_driver;

    let cancelled = step(
        &mut simulation,
        vec![
            source_wire(wu(64, 0), EndpointTarget::Free),
            Command::SetExternalDriver(SetExternalDriverCommand {
                driver: input,
                level: LogicLevel::High,
                strength: DriveStrength(400),
            }),
        ],
    );
    assert_eq!(cancelled.completed_tick, Tick(3));
    let ratio = gate_ratio(&cancelled);
    assert!(
        ratio < PowerRatio::new(Fixed(FIXED_ONE / 5)).expect("one fifth is a valid ratio"),
        "the cancellation case must be below logicOperateThreshold, got {ratio:?}"
    );
    let after = simulation
        .gate_signal_state(GATE)
        .expect("focused Gate remains live");
    assert_eq!(after.current_output, LogicLevel::Low);
    assert_eq!(after.desired_output, LogicLevel::Low);
    assert_eq!(after.pending_due_tick, None);
    assert_eq!(after.pending_level, None);
    assert_eq!(after.pending_switch_energy, None);
    assert_eq!(after.pending_generation, generation + 1);
    assert_eq!(
        after.cancelled_switching_heat.0,
        before.cancelled_switching_heat.0 + reserved_energy.0
    );
    assert_ne!(simulation.state_hash(), hash_before);
    assert_eq!(simulation.next_tick(), Tick(4));

    step(&mut simulation, Vec::new());
    step(&mut simulation, Vec::new());
    let stale = step(&mut simulation, Vec::new());
    assert_eq!(stale.completed_tick, Tick(6));
    assert_eq!(stale.signal_counters.stale_driver_transitions, 1);
    assert_eq!(output_sample(&simulation).level, LogicLevel::Low);
    let final_gate = simulation
        .gate_signal_state(GATE)
        .expect("focused Gate remains live");
    assert_eq!(final_gate.current_output, LogicLevel::Low);
    assert_eq!(final_gate.desired_output, LogicLevel::Low);
    assert_eq!(final_gate.pending_due_tick, None);
}
