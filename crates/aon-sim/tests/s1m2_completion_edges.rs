use aon_sim::{
    ArtifactBytes, BindPortCommand, Command, CommandEnvelope, CommandRejectionReason, DemandId,
    DemandKind, DriveStrength, EndpointTarget, Energy, EntityId, FIXED_ONE, Fixed, FixedAabb,
    FixedVec2, GateId, GatePort, GatePortRef, GateType, HostileCollider, LogicLevel, MobileId,
    PlaceGateCommand, PlaceMobileSubstrateCommand, PlaceWireCommand, PowerRatio, PowerSourceId,
    Revision, RoutingDomain, Simulation, StageFeatureSet, StepReport, Tick, WireEnd, WireId,
    WireSensePortRef, WorldInputEvent, decode_balance_profile, decode_numeric_profile,
    decode_package, decode_physical_scale_profile,
};
use serde_json::json;

const NUMERIC: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/numeric/v1.json"
));
const PHYSICAL: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/physical-scale/stage0-alpha.json"
));
const BALANCE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/balance/s1-m2-power-probe-alpha.json"
));

const WU: i64 = FIXED_ONE;
const CIRCUIT_PITCH: i64 = 16_384;
const WORLD_PITCH: i64 = 65_536;
const SOURCE: PowerSourceId = PowerSourceId(EntityId(2));

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

const fn wu(x: i64, y: i64) -> FixedVec2 {
    point(x * WU, y * WU)
}

fn scenario_bytes(source: Option<(FixedVec2, u64)>, mobility: bool) -> Vec<u8> {
    let numeric = decode_numeric_profile(NUMERIC).expect("the Numeric Profile decodes");
    let physical =
        decode_physical_scale_profile(PHYSICAL).expect("the Physical Scale Profile decodes");
    let balance = decode_balance_profile(BALANCE).expect("the S1-M2 Balance Profile decodes");
    let sources = source
        .into_iter()
        .map(|(position, generation)| {
            json!({
                "position": { "x": position.x.0, "y": position.y.0 },
                "generationPerTick": generation
            })
        })
        .collect::<Vec<_>>();
    let features = StageFeatureSet {
        signal: true,
        mobility,
        capacity: true,
        sensing: true,
        power: true,
        ..StageFeatureSet::none()
    };

    serde_json::to_vec(&json!({
        "schemaVersion": 3,
        "scenarioId": "s1-m2-completion-edges",
        "semanticsVersion": "aon-semantics-v1",
        "hashAlgorithm": "blake3-v1",
        "initialWorld": {
            "kind": "main-core-power-v1",
            "mainCore": {
                "position": { "x": -16 * WU, "y": -16 * WU },
                "integrity": 1_000,
                "heatEnergy": 0
            },
            "powerSources": sources
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
                "path": "profiles/balance/s1-m2-power-probe-alpha.json",
                "profileId": balance.profile_id,
                "profileHash": balance.canonical_hash().expect("Balance Profile hashes").to_string()
            }
        }
    }))
    .expect("the focused Scenario serializes")
}

fn new_simulation(source: Option<(FixedVec2, u64)>, mobility: bool) -> Simulation {
    let scenario = scenario_bytes(source, mobility);
    let package = decode_package(ArtifactBytes {
        scenario: &scenario,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("the focused package decodes");
    Simulation::new(package).expect("the focused Simulation starts")
}

fn envelopes(simulation: &Simulation, commands: Vec<Command>) -> Vec<CommandEnvelope> {
    commands
        .into_iter()
        .enumerate()
        .map(|(ordinal, command)| CommandEnvelope {
            target_tick: simulation.next_tick(),
            ordinal: u64::try_from(ordinal).expect("test ordinal fits u64"),
            command,
        })
        .collect()
}

fn step(simulation: &mut Simulation, commands: Vec<Command>) -> StepReport {
    let envelopes = envelopes(simulation, commands);
    let report = simulation
        .step(&envelopes)
        .expect("the focused Tick succeeds");
    assert_eq!(
        report.command_acceptances.len(),
        envelopes.len(),
        "all focused commands are accepted: {:?}",
        report.command_rejections
    );
    assert!(report.command_rejections.is_empty());
    report
}

fn step_with_hostiles(
    simulation: &mut Simulation,
    commands: Vec<Command>,
    hostiles: Vec<HostileCollider>,
) -> StepReport {
    let envelopes = envelopes(simulation, commands);
    let frame = WorldInputEvent::HostileFrame {
        target_tick: simulation.next_tick(),
        hostiles,
    };
    let report = simulation
        .step_with_world_inputs(&envelopes, &[frame])
        .expect("the focused hostile Tick succeeds");
    assert_eq!(report.command_acceptances.len(), envelopes.len());
    assert!(report.command_rejections.is_empty());
    report
}

fn gate_port(gate: GateId, port: GatePort) -> EndpointTarget {
    EndpointTarget::GatePort(GatePortRef { gate, port })
}

fn hostile(id: u64, center: FixedVec2) -> HostileCollider {
    HostileCollider {
        id,
        center,
        radius: Fixed::ZERO,
    }
}

#[test]
fn mobile_source_anchor_bridge_requires_exact_in_area_position_and_powers_gate() {
    let mut simulation = new_simulation(Some((point(0, 0), 100)), true);

    let track = step(
        &mut simulation,
        vec![Command::PlaceWire(PlaceWireCommand {
            routing_domain: RoutingDomain::OpenWorld,
            points: vec![point(-4 * WORLD_PITCH, 0), point(4 * WORLD_PITCH, 0)],
            endpoint_a: EndpointTarget::Free,
            endpoint_b: EndpointTarget::Free,
        })],
    );
    assert_eq!(
        track.command_acceptances[0].created_entity,
        Some(EntityId(3))
    );

    let bounds = FixedAabb::new(
        point(-4 * CIRCUIT_PITCH, -4 * CIRCUIT_PITCH),
        point(4 * CIRCUIT_PITCH, 4 * CIRCUIT_PITCH),
    );
    let mobile = step(
        &mut simulation,
        vec![Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
            origin: point(0, 0),
            routing_area: bounds,
            footprint: bounds,
        })],
    );
    let mobile = MobileId(
        mobile.command_acceptances[0]
            .created_entity
            .expect("the Mobile Substrate is created"),
    );
    assert_eq!(mobile, MobileId(EntityId(4)));
    let domain = RoutingDomain::MobileSubstrate(mobile.entity_id());

    let gate = GateId(EntityId(5));
    let gate_origin = point(2 * CIRCUIT_PITCH, CIRCUIT_PITCH);
    let placed_gate = step(
        &mut simulation,
        vec![Command::PlaceGate(PlaceGateCommand {
            gate_type: GateType::Not,
            origin: gate_origin,
            routing_domain: domain,
        })],
    );
    assert_eq!(
        placed_gate.command_acceptances[0].created_entity,
        Some(gate.entity_id())
    );

    let power_port = point(2 * CIRCUIT_PITCH, 0);
    let powered = step(
        &mut simulation,
        vec![Command::PlaceWire(PlaceWireCommand {
            routing_domain: domain,
            points: vec![
                point(0, 0),
                point(0, -CIRCUIT_PITCH),
                point(2 * CIRCUIT_PITCH, -CIRCUIT_PITCH),
                power_port,
            ],
            endpoint_a: EndpointTarget::PowerSourceAnchor(SOURCE),
            endpoint_b: gate_port(gate, GatePort::Power),
        })],
    );
    let idle = powered
        .power
        .as_ref()
        .expect("Power report exists")
        .load(DemandId::new(gate.entity_id(), DemandKind::GateIdle))
        .expect("the Mobile-local Gate contributes GateIdle demand");
    assert_eq!(idle.ratio, PowerRatio::ONE);
    assert_eq!(
        idle.source_route
            .as_ref()
            .expect("the Mobile-local Gate has a Source route")
            .source(),
        SOURCE
    );

    let hash_before_rejection = simulation.state_hash();
    let attempted = envelopes(
        &simulation,
        vec![Command::PlaceWire(PlaceWireCommand {
            routing_domain: domain,
            points: vec![
                point(-CIRCUIT_PITCH, CIRCUIT_PITCH),
                point(-3 * CIRCUIT_PITCH, CIRCUIT_PITCH),
            ],
            endpoint_a: EndpointTarget::PowerSourceAnchor(SOURCE),
            endpoint_b: EndpointTarget::Free,
        })],
    );
    let rejected = simulation
        .step(&attempted)
        .expect("a wrong-position Source anchor is an ordinary rejection");
    assert!(rejected.command_acceptances.is_empty());
    assert_eq!(rejected.command_rejections.len(), 1);
    assert_eq!(
        rejected.command_rejections[0].reason,
        CommandRejectionReason::InvalidEndpoint
    );
    assert!(!rejected.topology_changed);
    assert_ne!(
        simulation.state_hash(),
        hash_before_rejection,
        "only Tick advancement is canonical"
    );

    let mut out_of_area = new_simulation(Some((point(8 * CIRCUIT_PITCH, 0), 100)), true);
    step(
        &mut out_of_area,
        vec![Command::PlaceWire(PlaceWireCommand {
            routing_domain: RoutingDomain::OpenWorld,
            points: vec![point(-4 * WORLD_PITCH, 0), point(4 * WORLD_PITCH, 0)],
            endpoint_a: EndpointTarget::Free,
            endpoint_b: EndpointTarget::Free,
        })],
    );
    let mobile = step(
        &mut out_of_area,
        vec![Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
            origin: point(0, 0),
            routing_area: bounds,
            footprint: bounds,
        })],
    );
    let domain = RoutingDomain::MobileSubstrate(
        mobile.command_acceptances[0]
            .created_entity
            .expect("the out-of-area fixture Mobile is created"),
    );
    let attempted = envelopes(
        &out_of_area,
        vec![Command::PlaceWire(PlaceWireCommand {
            routing_domain: domain,
            points: vec![point(8 * CIRCUIT_PITCH, 0), point(0, 0)],
            endpoint_a: EndpointTarget::PowerSourceAnchor(SOURCE),
            endpoint_b: EndpointTarget::Free,
        })],
    );
    let rejected = out_of_area
        .step(&attempted)
        .expect("an out-of-area Source binding rejects atomically");
    assert!(rejected.command_acceptances.is_empty());
    assert_eq!(
        rejected.command_rejections[0].reason,
        CommandRejectionReason::SubstrateBoundsViolation
    );
    assert!(!rejected.topology_changed);
}

#[test]
fn gate_retention_expires_on_exact_third_under_threshold_tick_and_recovery_cancels_reset() {
    let mut simulation = new_simulation(Some((point(0, 0), 100)), false);

    let bounds = FixedAabb::new(wu(-8, -8), wu(8, 8));
    let substrate = step(
        &mut simulation,
        vec![Command::PlaceFixedSubstrate(
            aon_sim::PlaceFixedSubstrateCommand {
                origin: point(0, 0),
                routing_area: bounds,
                footprint: bounds,
            },
        )],
    );
    let substrate = substrate.command_acceptances[0]
        .created_entity
        .expect("the retention substrate is created");
    assert_eq!(substrate, EntityId(3));
    let domain = RoutingDomain::FixedSubstrate(substrate);

    let gate = GateId(EntityId(4));
    step(
        &mut simulation,
        vec![Command::PlaceGate(PlaceGateCommand {
            gate_type: GateType::Not,
            origin: point(2 * CIRCUIT_PITCH, CIRCUIT_PITCH),
            routing_domain: domain,
        })],
    );
    let wire = WireId(EntityId(5));
    let powered = step(
        &mut simulation,
        vec![Command::PlaceWire(PlaceWireCommand {
            routing_domain: domain,
            points: vec![
                point(0, 0),
                point(0, -CIRCUIT_PITCH),
                point(2 * CIRCUIT_PITCH, -CIRCUIT_PITCH),
                point(2 * CIRCUIT_PITCH, 0),
            ],
            endpoint_a: EndpointTarget::PowerSourceAnchor(SOURCE),
            endpoint_b: gate_port(gate, GatePort::Power),
        })],
    );
    assert_eq!(
        powered.command_acceptances[0].created_entity,
        Some(wire.entity_id())
    );
    assert_eq!(
        powered
            .power
            .as_ref()
            .expect("Power report exists")
            .gate(gate)
            .expect("Gate report exists")
            .ratio,
        PowerRatio::ONE
    );

    let settled = step(&mut simulation, Vec::new());
    assert_eq!(settled.completed_tick, Tick(3));
    let high = simulation
        .gate_signal_state(gate)
        .expect("the retention Gate remains live");
    assert_eq!(high.current_output, LogicLevel::High);
    assert_eq!(high.pending_due_tick, None);

    let first = step(
        &mut simulation,
        vec![Command::BindPort(BindPortCommand {
            wire,
            end: WireEnd::A,
            target: EndpointTarget::Free,
        })],
    );
    assert_eq!(first.completed_tick, Tick(4));
    assert_eq!(
        first
            .power
            .as_ref()
            .expect("Power report exists")
            .gate(gate)
            .expect("Gate report exists")
            .unpowered_ticks,
        1
    );
    let second = step(&mut simulation, Vec::new());
    let state = simulation
        .gate_signal_state(gate)
        .expect("the under-powered Gate remains live");
    assert_eq!(state.unpowered_ticks, 2);
    assert_eq!(state.current_output, LogicLevel::High);
    assert_eq!(
        state.pending_due_tick, None,
        "retention preserves before Tick 3"
    );
    assert_eq!(
        second
            .power
            .as_ref()
            .expect("Power report exists")
            .gate(gate)
            .expect("Gate report exists")
            .unpowered_ticks,
        2
    );

    let third = step(&mut simulation, Vec::new());
    let expired = simulation
        .gate_signal_state(gate)
        .expect("the expired Gate remains live");
    assert_eq!(expired.unpowered_ticks, 3);
    assert_eq!(expired.current_output, LogicLevel::High);
    assert_eq!(expired.desired_output, LogicLevel::Low);
    assert_eq!(expired.pending_level, Some(LogicLevel::Low));
    assert_eq!(expired.pending_switch_energy, Some(Energy(0)));
    let effective_delay = third
        .power
        .as_ref()
        .expect("Power report exists")
        .gate(gate)
        .expect("Gate report exists")
        .effective_delay;
    assert_eq!(
        expired.pending_due_tick,
        Some(Tick(third.completed_tick.0 + effective_delay.0))
    );
    let expiry_generation = expired.pending_generation;
    let cancelled_heat = expired.cancelled_switching_heat;

    let recovered = step(
        &mut simulation,
        vec![Command::BindPort(BindPortCommand {
            wire,
            end: WireEnd::A,
            target: EndpointTarget::PowerSourceAnchor(SOURCE),
        })],
    );
    assert_eq!(
        recovered
            .power
            .as_ref()
            .expect("Power report exists")
            .gate(gate)
            .expect("Gate report exists")
            .ratio,
        PowerRatio::ONE
    );
    let recovered = simulation
        .gate_signal_state(gate)
        .expect("the recovered Gate remains live");
    assert_eq!(recovered.unpowered_ticks, 0);
    assert_eq!(recovered.current_output, LogicLevel::High);
    assert_eq!(recovered.desired_output, LogicLevel::High);
    assert_eq!(recovered.pending_due_tick, None);
    assert_eq!(recovered.pending_level, None);
    assert_eq!(recovered.pending_switch_energy, None);
    assert_eq!(recovered.pending_generation, expiry_generation + 1);
    assert_eq!(recovered.cancelled_switching_heat, cancelled_heat);

    while simulation.next_tick().0 <= third.completed_tick.0 + effective_delay.0 {
        step(&mut simulation, Vec::new());
    }
    let stale_due = simulation
        .gate_signal_state(gate)
        .expect("the recovered Gate remains live after the stale due Tick");
    assert_eq!(stale_due.current_output, LogicLevel::High);
    assert!(stale_due.pending_due_tick.is_none());
}

#[test]
fn c07_count_one_to_three_does_not_retrigger_and_source_less_sense_is_passive_low() {
    let mut simulation = new_simulation(None, false);

    let bounds = FixedAabb::new(wu(-8, -8), wu(8, 8));
    step(
        &mut simulation,
        vec![Command::PlaceFixedSubstrate(
            aon_sim::PlaceFixedSubstrateCommand {
                origin: point(0, 0),
                routing_area: bounds,
                footprint: bounds,
            },
        )],
    );
    let domain = RoutingDomain::FixedSubstrate(EntityId(2));
    let sensed = WireId(EntityId(3));
    let probe = GateId(EntityId(4));
    step(
        &mut simulation,
        vec![
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: domain,
                points: vec![point(-4 * WU, 0), point(4 * WU, 0)],
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Free,
            }),
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Not,
                origin: point(2 * CIRCUIT_PITCH, 2 * CIRCUIT_PITCH),
                routing_domain: domain,
            }),
        ],
    );
    let probe_sink = simulation
        .gate_signal_ports(probe)
        .expect("the passive-LOW probe Gate exists")
        .input_a
        .sink;
    let sense_ports = simulation
        .wire_sense_state(sensed)
        .expect("the source-less Wire has Sense A/B")
        .ports;

    let binding = step(
        &mut simulation,
        vec![Command::PlaceWire(PlaceWireCommand {
            routing_domain: domain,
            points: vec![
                point(-4 * WU, 0),
                point(-8 * WU, 0),
                point(-8 * WU, 2 * CIRCUIT_PITCH),
                point(CIRCUIT_PITCH, 2 * CIRCUIT_PITCH),
            ],
            endpoint_a: EndpointTarget::WireSensePort(WireSensePortRef {
                wire: sensed,
                end: WireEnd::A,
            }),
            endpoint_b: gate_port(probe, GatePort::InputA),
        })],
    );
    assert_eq!(
        binding.command_acceptances[0].created_entity,
        Some(EntityId(5))
    );

    let one = step_with_hostiles(&mut simulation, Vec::new(), vec![hostile(1, point(0, 0))]);
    let sense = simulation
        .wire_sense_state(sensed)
        .expect("the sensed Wire remains live");
    assert!(sense.sampled_presence);
    assert_eq!(sense.intended_level, LogicLevel::High);
    assert_eq!(sense.intended_strength, DriveStrength(0));
    assert!(one.driver_changes.is_empty(), "Sense delay defers the HIGH");

    let three = step_with_hostiles(
        &mut simulation,
        Vec::new(),
        vec![
            hostile(1, point(-WU / 2, 0)),
            hostile(2, point(0, 0)),
            hostile(3, point(WU / 2, 0)),
        ],
    );
    let sense = simulation
        .wire_sense_state(sensed)
        .expect("the sensed Wire remains live");
    assert!(sense.sampled_presence);
    assert_eq!(sense.intended_level, LogicLevel::High);
    assert_eq!(sense.intended_strength, DriveStrength(0));
    let changed = three
        .driver_changes
        .iter()
        .filter(|change| change.driver == sense_ports.a || change.driver == sense_ports.b)
        .collect::<Vec<_>>();
    assert_eq!(
        changed.len(),
        2,
        "the original 1-hostile HIGH becomes due once per end"
    );
    for change in changed {
        assert_eq!(change.current.level, LogicLevel::High);
        assert_eq!(change.current.strength, DriveStrength(0));
        assert_eq!(change.current.revision, Revision(1));
    }
    assert_eq!(simulation.sink_level(probe_sink), Some(LogicLevel::Low));

    let still_three = step_with_hostiles(
        &mut simulation,
        Vec::new(),
        vec![
            hostile(1, point(-WU / 2, 0)),
            hostile(2, point(0, 0)),
            hostile(3, point(WU / 2, 0)),
        ],
    );
    assert!(
        still_three
            .driver_changes
            .iter()
            .all(|change| change.driver != sense_ports.a && change.driver != sense_ports.b),
        "count 1 -> 3 did not schedule an additional Sense transition"
    );
    for driver in [sense_ports.a, sense_ports.b] {
        let sample = simulation
            .driver_sample(driver)
            .expect("the source-less Sense Driver remains live");
        assert_eq!(sample.level, LogicLevel::High);
        assert_eq!(sample.strength, DriveStrength(0));
        assert_eq!(sample.revision, Revision(1));
    }

    while simulation
        .sink_driver_sample(probe_sink, sense_ports.a)
        .is_none_or(|sample| sample.revision < Revision(1))
    {
        step_with_hostiles(
            &mut simulation,
            Vec::new(),
            vec![
                hostile(1, point(-WU / 2, 0)),
                hostile(2, point(0, 0)),
                hostile(3, point(WU / 2, 0)),
            ],
        );
    }
    let arrived = simulation
        .sink_driver_sample(probe_sink, sense_ports.a)
        .expect("the delayed source-less HIGH reaches the probe slot");
    assert_eq!(arrived.level, LogicLevel::High);
    assert_eq!(arrived.strength, DriveStrength(0));
    assert_eq!(arrived.revision, Revision(1));
    assert_eq!(
        simulation.sink_level(probe_sink),
        Some(LogicLevel::Low),
        "sub-threshold Sense strength is passive LOW without a health bit"
    );
}
