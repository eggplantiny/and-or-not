use aon_sim::{
    ArtifactBytes, Command, CommandAcceptance, CommandEnvelope, CommandRejection,
    CommandRejectionReason, EndpointTarget, EntityId, Fixed, FixedAabb, FixedVec2, GateId,
    GatePort, GatePortRef, GateType, JunctionId, PlaceFixedSubstrateCommand, PlaceGateCommand,
    PlaceJunctionCommand, PlaceWireCommand, RoutingDomain, Simulation, SimulationPackage,
    decode_package,
};

const SCENARIO: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/scenarios/empty.json"
));
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
    "/../../profiles/balance/stage0-alpha.json"
));

const QUANTUM: i64 = 1_024;
const CIRCUIT_PITCH: i64 = 16_384;
const WORLD_PITCH: i64 = 65_536;

fn simulation() -> Simulation {
    let package: SimulationPackage = decode_package(ArtifactBytes {
        scenario: SCENARIO,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("the reference S0 package is valid");
    Simulation::new(package).expect("the reference S0 simulation starts")
}

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

fn envelope(simulation: &Simulation, ordinal: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        target_tick: simulation.next_tick(),
        ordinal,
        command,
    }
}

fn substrate_command() -> Command {
    let bounds = FixedAabb::new(
        point(-4 * WORLD_PITCH, -4 * WORLD_PITCH),
        point(4 * WORLD_PITCH, 4 * WORLD_PITCH),
    );
    Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
        origin: point(0, 0),
        routing_area: bounds,
        footprint: bounds,
    })
}

fn gate_command(routing_domain: RoutingDomain, gate_type: GateType, origin: FixedVec2) -> Command {
    Command::PlaceGate(PlaceGateCommand {
        gate_type,
        origin,
        routing_domain,
    })
}

fn wire_command(
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

fn expect_created(simulation: &mut Simulation, command: Command, expected_id: u64) -> EntityId {
    let target_tick = simulation.next_tick();
    let report = simulation
        .step(&[CommandEnvelope {
            target_tick,
            ordinal: 0,
            command,
        }])
        .expect("the fixture placement is valid");
    let expected = EntityId(expected_id);
    assert_eq!(
        report.command_acceptances,
        vec![CommandAcceptance {
            target_tick,
            ordinal: 0,
            created_entity: Some(expected),
        }]
    );
    assert!(report.command_rejections.is_empty());
    expected
}

fn expect_rejected(simulation: &mut Simulation, command: Command, reason: CommandRejectionReason) {
    let target_tick = simulation.next_tick();
    let report = simulation
        .step(&[CommandEnvelope {
            target_tick,
            ordinal: 0,
            command,
        }])
        .expect("an invalid endpoint is a command rejection, not a run error");
    assert!(report.command_acceptances.is_empty());
    assert_eq!(
        report.command_rejections,
        vec![CommandRejection {
            target_tick,
            ordinal: 0,
            reason,
        }]
    );
    assert!(!report.topology_changed);
}

fn simulation_with_gate(gate_type: GateType) -> (Simulation, RoutingDomain, GateId) {
    let mut simulation = simulation();
    let substrate = expect_created(&mut simulation, substrate_command(), 1);
    let domain = RoutingDomain::FixedSubstrate(substrate);
    let gate = GateId(expect_created(
        &mut simulation,
        gate_command(domain, gate_type, point(0, 0)),
        2,
    ));
    (simulation, domain, gate)
}

fn run_shared_gate_port_fanout(reverse_input: bool) -> Simulation {
    let (mut simulation, domain, gate) = simulation_with_gate(GateType::Not);
    let shared_port = EndpointTarget::GatePort(GatePortRef {
        gate,
        port: GatePort::InputA,
    });
    let anchor = point(-CIRCUIT_PITCH, 0);
    let mut commands = vec![
        envelope(
            &simulation,
            10,
            wire_command(
                domain,
                vec![anchor, point(-2 * CIRCUIT_PITCH, 0)],
                shared_port,
                EndpointTarget::Free,
            ),
        ),
        envelope(
            &simulation,
            20,
            wire_command(
                domain,
                vec![anchor, point(-2 * CIRCUIT_PITCH, CIRCUIT_PITCH)],
                shared_port,
                EndpointTarget::Free,
            ),
        ),
    ];
    if reverse_input {
        commands.reverse();
    }

    let target_tick = simulation.next_tick();
    let report = simulation
        .step(&commands)
        .expect("distinct Wires may fan out from one Gate port");
    assert_eq!(
        report.command_acceptances,
        vec![
            CommandAcceptance {
                target_tick,
                ordinal: 10,
                created_entity: Some(EntityId(3)),
            },
            CommandAcceptance {
                target_tick,
                ordinal: 20,
                created_entity: Some(EntityId(4)),
            },
        ]
    );
    assert!(report.command_rejections.is_empty());
    assert!(report.topology_changed);
    simulation
}

#[test]
fn multiple_wires_can_share_one_gate_port_deterministically_without_positive_overlap() {
    let forward = run_shared_gate_port_fanout(false);
    let reversed = run_shared_gate_port_fanout(true);

    assert_eq!(forward.next_tick(), reversed.next_tick());
    assert_eq!(forward.topology_revision(), reversed.topology_revision());
    assert_eq!(forward.state_hash(), reversed.state_hash());
}

#[test]
fn gate_port_binding_is_valid_on_wire_end_b() {
    let (mut simulation, domain, gate) = simulation_with_gate(GateType::Not);
    let created = expect_created(
        &mut simulation,
        wire_command(
            domain,
            vec![point(-2 * CIRCUIT_PITCH, 0), point(-CIRCUIT_PITCH, 0)],
            EndpointTarget::Free,
            EndpointTarget::GatePort(GatePortRef {
                gate,
                port: GatePort::InputA,
            }),
        ),
        3,
    );

    assert_eq!(created, EntityId(3));
}

#[test]
fn binary_off_pitch_gate_anchor_is_valid_on_reversed_wire_end_b() {
    let (mut simulation, domain, gate) = simulation_with_gate(GateType::And);
    let input_a = point(-CIRCUIT_PITCH, -8 * QUANTUM);
    assert_eq!(input_a.y.0.rem_euclid(QUANTUM), 0);
    assert_ne!(input_a.y.0.rem_euclid(CIRCUIT_PITCH), 0);

    expect_created(
        &mut simulation,
        wire_command(
            domain,
            vec![point(-2 * CIRCUIT_PITCH, -8 * QUANTUM), input_a],
            EndpointTarget::Free,
            EndpointTarget::GatePort(GatePortRef {
                gate,
                port: GatePort::InputA,
            }),
        ),
        3,
    );
}

#[test]
fn same_anchor_contact_from_another_routing_domain_rejects_before_endpoint_binding() {
    let (mut simulation, _domain, gate) = simulation_with_gate(GateType::Not);
    expect_rejected(
        &mut simulation,
        wire_command(
            RoutingDomain::OpenWorld,
            vec![point(-CIRCUIT_PITCH, 0), point(-2 * CIRCUIT_PITCH, 0)],
            EndpointTarget::GatePort(GatePortRef {
                gate,
                port: GatePort::InputA,
            }),
            EndpointTarget::Free,
        ),
        CommandRejectionReason::InvalidPortBinding,
    );
}

#[test]
fn noncontacting_gate_reference_from_another_domain_is_an_invalid_endpoint() {
    let (mut simulation, _domain, gate) = simulation_with_gate(GateType::Not);
    expect_rejected(
        &mut simulation,
        wire_command(
            RoutingDomain::OpenWorld,
            vec![point(2 * WORLD_PITCH, 0), point(3 * WORLD_PITCH, 0)],
            EndpointTarget::GatePort(GatePortRef {
                gate,
                port: GatePort::InputA,
            }),
            EndpointTarget::Free,
        ),
        CommandRejectionReason::InvalidEndpoint,
    );
}

#[test]
fn same_anchor_coordinate_with_a_non_gate_gate_id_is_an_invalid_endpoint() {
    let (mut simulation, domain, _gate) = simulation_with_gate(GateType::Not);
    let anchor = point(-CIRCUIT_PITCH, 0);
    let junction = JunctionId(expect_created(
        &mut simulation,
        Command::PlaceJunction(PlaceJunctionCommand {
            routing_domain: domain,
            position: anchor,
        }),
        3,
    ));

    expect_rejected(
        &mut simulation,
        wire_command(
            domain,
            vec![anchor, point(-2 * CIRCUIT_PITCH, 0)],
            EndpointTarget::GatePort(GatePortRef {
                gate: GateId(junction.entity_id()),
                port: GatePort::InputA,
            }),
            EndpointTarget::Free,
        ),
        CommandRejectionReason::InvalidEndpoint,
    );
}
