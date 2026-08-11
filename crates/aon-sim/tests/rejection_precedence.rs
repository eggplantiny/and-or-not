use aon_sim::{
    ArtifactBytes, BindPortCommand, Command, CommandEnvelope, CommandRejection,
    CommandRejectionReason, EndpointTarget, EntityId, Fixed, FixedAabb, FixedVec2, GateType,
    JunctionId, PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceJunctionCommand,
    PlaceWireCommand, RemoveEntityCommand, RoutingDomain, Simulation, SimulationPackage, WireEnd,
    WireId, decode_package,
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
const WORLD_PITCH: i64 = 65_536;
const UNKNOWN_ID: EntityId = EntityId(10_000);

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

fn envelope(simulation: &Simulation, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        target_tick: simulation.next_tick(),
        ordinal: 0,
        command,
    }
}

fn expect_rejected(simulation: &mut Simulation, command: Command, reason: CommandRejectionReason) {
    let target_tick = simulation.next_tick();
    let report = simulation
        .step(&[CommandEnvelope {
            target_tick,
            ordinal: 0,
            command,
        }])
        .expect("ordinary command rejection must not fail the simulation step");

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

fn expect_created(simulation: &mut Simulation, command: Command) -> EntityId {
    let report = simulation
        .step(&[envelope(simulation, command)])
        .expect("the fixture placement is valid");
    assert!(report.command_rejections.is_empty());
    report.command_acceptances[0]
        .created_entity
        .expect("fixture placement creates one entity")
}

fn remove(simulation: &mut Simulation, target: EntityId) {
    let report = simulation
        .step(&[envelope(
            simulation,
            Command::RemoveEntity(RemoveEntityCommand { target }),
        )])
        .expect("the fixture entity can be removed");
    assert!(report.command_rejections.is_empty());
}

fn valid_fixed_substrate() -> Command {
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

fn open_wire(
    points: Vec<FixedVec2>,
    endpoint_a: EndpointTarget,
    endpoint_b: EndpointTarget,
) -> Command {
    Command::PlaceWire(PlaceWireCommand {
        routing_domain: RoutingDomain::OpenWorld,
        points,
        endpoint_a,
        endpoint_b,
    })
}

fn removed_open_junction_fixture() -> (Simulation, JunctionId) {
    let mut simulation = simulation();
    let junction = JunctionId(expect_created(
        &mut simulation,
        Command::PlaceJunction(PlaceJunctionCommand {
            routing_domain: RoutingDomain::OpenWorld,
            position: point(0, 0),
        }),
    ));
    remove(&mut simulation, junction.entity_id());
    (simulation, junction)
}

#[test]
fn fixed_substrate_shape_precedes_geometry_quantization() {
    let valid_bounds = FixedAabb::new(
        point(-WORLD_PITCH, -WORLD_PITCH),
        point(WORLD_PITCH, WORLD_PITCH),
    );
    let empty_off_quantum = FixedAabb::new(point(1, 1), point(1, WORLD_PITCH + 1));

    for command in [
        PlaceFixedSubstrateCommand {
            origin: point(1, 0),
            routing_area: empty_off_quantum,
            footprint: valid_bounds,
        },
        PlaceFixedSubstrateCommand {
            origin: point(1, 0),
            routing_area: valid_bounds,
            footprint: empty_off_quantum,
        },
    ] {
        expect_rejected(
            &mut simulation(),
            Command::PlaceFixedSubstrate(command),
            CommandRejectionReason::InvalidGeometryShape,
        );
    }
}

#[test]
fn short_wire_schema_precedes_unknown_endpoint_and_geometry_quantization() {
    let mut simulation = simulation();
    expect_rejected(
        &mut simulation,
        open_wire(
            vec![point(1, 0)],
            EndpointTarget::Junction(JunctionId(UNKNOWN_ID)),
            EndpointTarget::Free,
        ),
        CommandRejectionReason::InvalidGeometryShape,
    );
}

#[test]
fn unknown_wire_endpoint_precedes_geometry_quantization() {
    let mut simulation = simulation();
    expect_rejected(
        &mut simulation,
        open_wire(
            vec![point(1, 0), point(WORLD_PITCH, 0)],
            EndpointTarget::Junction(JunctionId(UNKNOWN_ID)),
            EndpointTarget::Free,
        ),
        CommandRejectionReason::UnknownEntity,
    );
}

#[test]
fn unknown_wire_endpoint_precedes_internal_vertex_routing_pitch() {
    let mut simulation = simulation();
    expect_rejected(
        &mut simulation,
        open_wire(
            vec![point(0, 0), point(QUANTUM, 0), point(WORLD_PITCH, 0)],
            EndpointTarget::Junction(JunctionId(UNKNOWN_ID)),
            EndpointTarget::Free,
        ),
        CommandRejectionReason::UnknownEntity,
    );
}

#[test]
fn unknown_fixed_domain_precedes_gate_and_junction_quantization() {
    let commands = [
        Command::PlaceGate(PlaceGateCommand {
            gate_type: GateType::Not,
            origin: point(1, 0),
            routing_domain: RoutingDomain::FixedSubstrate(UNKNOWN_ID),
        }),
        Command::PlaceJunction(PlaceJunctionCommand {
            routing_domain: RoutingDomain::FixedSubstrate(UNKNOWN_ID),
            position: point(1, 0),
        }),
    ];

    for command in commands {
        expect_rejected(
            &mut simulation(),
            command,
            CommandRejectionReason::UnknownEntity,
        );
    }
}

#[test]
fn wire_reference_lifecycle_follows_domain_then_endpoint_payload_order() {
    fn removed_domain_and_endpoint_fixture() -> (Simulation, EntityId, JunctionId) {
        let mut simulation = simulation();
        let substrate = expect_created(&mut simulation, valid_fixed_substrate());
        let junction = JunctionId(expect_created(
            &mut simulation,
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: point(0, 0),
            }),
        ));
        remove(&mut simulation, substrate);
        remove(&mut simulation, junction.entity_id());
        (simulation, substrate, junction)
    }

    let (mut unknown_domain, _, removed_junction) = removed_domain_and_endpoint_fixture();
    expect_rejected(
        &mut unknown_domain,
        Command::PlaceWire(PlaceWireCommand {
            routing_domain: RoutingDomain::FixedSubstrate(UNKNOWN_ID),
            points: vec![point(0, 0), point(WORLD_PITCH, 0)],
            endpoint_a: EndpointTarget::Junction(removed_junction),
            endpoint_b: EndpointTarget::Free,
        }),
        CommandRejectionReason::UnknownEntity,
    );

    let (mut removed_domain, removed_substrate, _) = removed_domain_and_endpoint_fixture();
    expect_rejected(
        &mut removed_domain,
        Command::PlaceWire(PlaceWireCommand {
            routing_domain: RoutingDomain::FixedSubstrate(removed_substrate),
            points: vec![point(0, 0), point(WORLD_PITCH, 0)],
            endpoint_a: EndpointTarget::Junction(JunctionId(UNKNOWN_ID)),
            endpoint_b: EndpointTarget::Free,
        }),
        CommandRejectionReason::RemovedEntity,
    );
}

#[test]
fn wire_endpoint_reference_lifecycle_follows_a_then_b_payload_order() {
    let (mut unknown_a, removed_b) = removed_open_junction_fixture();
    expect_rejected(
        &mut unknown_a,
        open_wire(
            vec![point(0, 0), point(WORLD_PITCH, 0)],
            EndpointTarget::Junction(JunctionId(UNKNOWN_ID)),
            EndpointTarget::Junction(removed_b),
        ),
        CommandRejectionReason::UnknownEntity,
    );

    let (mut removed_a, removed_a_target) = removed_open_junction_fixture();
    expect_rejected(
        &mut removed_a,
        open_wire(
            vec![point(0, 0), point(WORLD_PITCH, 0)],
            EndpointTarget::Junction(removed_a_target),
            EndpointTarget::Junction(JunctionId(UNKNOWN_ID)),
        ),
        CommandRejectionReason::RemovedEntity,
    );
}

#[test]
fn bind_validates_target_existence_before_rejecting_a_wrong_kind_wire() {
    let mut simulation = simulation();
    let junction = expect_created(
        &mut simulation,
        Command::PlaceJunction(PlaceJunctionCommand {
            routing_domain: RoutingDomain::OpenWorld,
            position: point(0, 0),
        }),
    );

    expect_rejected(
        &mut simulation,
        Command::BindPort(BindPortCommand {
            wire: WireId(junction),
            end: WireEnd::A,
            target: EndpointTarget::Junction(JunctionId(UNKNOWN_ID)),
        }),
        CommandRejectionReason::UnknownEntity,
    );
}

#[test]
fn bind_reference_lifecycle_follows_wire_then_target_payload_order() {
    fn removed_wire_and_junction_fixture() -> (Simulation, WireId, JunctionId) {
        let mut simulation = simulation();
        let wire = WireId(expect_created(
            &mut simulation,
            open_wire(
                vec![point(0, 0), point(WORLD_PITCH, 0)],
                EndpointTarget::Free,
                EndpointTarget::Free,
            ),
        ));
        let junction = JunctionId(expect_created(
            &mut simulation,
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: point(2 * WORLD_PITCH, 0),
            }),
        ));
        remove(&mut simulation, wire.entity_id());
        remove(&mut simulation, junction.entity_id());
        (simulation, wire, junction)
    }

    let (mut unknown_wire, _, removed_target) = removed_wire_and_junction_fixture();
    expect_rejected(
        &mut unknown_wire,
        Command::BindPort(BindPortCommand {
            wire: WireId(UNKNOWN_ID),
            end: WireEnd::A,
            target: EndpointTarget::Junction(removed_target),
        }),
        CommandRejectionReason::UnknownEntity,
    );

    let (mut removed_wire, removed_wire_target, _) = removed_wire_and_junction_fixture();
    expect_rejected(
        &mut removed_wire,
        Command::BindPort(BindPortCommand {
            wire: removed_wire_target,
            end: WireEnd::A,
            target: EndpointTarget::Junction(JunctionId(UNKNOWN_ID)),
        }),
        CommandRejectionReason::RemovedEntity,
    );
}
