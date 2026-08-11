use aon_sim::{
    ArtifactBytes, Command, CommandAcceptance, CommandEnvelope, CommandRejection,
    CommandRejectionReason, EndpointTarget, EntityId, Fixed, FixedAabb, FixedVec2, GateId,
    GatePort, GatePortRef, GateType, JunctionId, PlaceFixedSubstrateCommand, PlaceGateCommand,
    PlaceJunctionCommand, PlaceWireCommand, RenderSnapshot, RoutingDomain, Simulation,
    SimulationPackage, decode_package,
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

fn package() -> SimulationPackage {
    decode_package(ArtifactBytes {
        scenario: SCENARIO,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("the reference S0 package is valid")
}

fn simulation() -> Simulation {
    Simulation::new(package()).expect("the reference S0 simulation starts")
}

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

fn fixed_substrate_command() -> Command {
    Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
        origin: point(0, 0),
        // These routing bounds satisfy circuit pitch but not world pitch.
        routing_area: FixedAabb::new(
            point(-15 * CIRCUIT_PITCH, -15 * CIRCUIT_PITCH),
            point(15 * CIRCUIT_PITCH, 15 * CIRCUIT_PITCH),
        ),
        footprint: FixedAabb::new(
            point(-4 * WORLD_PITCH, -4 * WORLD_PITCH),
            point(4 * WORLD_PITCH, 4 * WORLD_PITCH),
        ),
    })
}

fn junction(domain: RoutingDomain, position: FixedVec2) -> Command {
    Command::PlaceJunction(PlaceJunctionCommand {
        routing_domain: domain,
        position,
    })
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

fn gate(domain: RoutingDomain, gate_type: GateType, origin: FixedVec2) -> Command {
    Command::PlaceGate(PlaceGateCommand {
        gate_type,
        origin,
        routing_domain: domain,
    })
}

fn expect_created(simulation: &mut Simulation, command: Command) -> EntityId {
    let target_tick = simulation.next_tick();
    let report = simulation
        .step(&[CommandEnvelope {
            target_tick,
            ordinal: 0,
            command,
        }])
        .expect("valid structural placement is accepted");
    assert!(report.command_rejections.is_empty());
    assert_eq!(report.command_acceptances.len(), 1);
    let created = report.command_acceptances[0]
        .created_entity
        .expect("placement returns its created EntityId");
    assert_eq!(
        report.command_acceptances,
        vec![CommandAcceptance {
            target_tick,
            ordinal: 0,
            created_entity: Some(created),
        }]
    );
    created
}

fn expect_rejected(simulation: &mut Simulation, command: Command, reason: CommandRejectionReason) {
    let target_tick = simulation.next_tick();
    let report = simulation
        .step(&[CommandEnvelope {
            target_tick,
            ordinal: 0,
            command,
        }])
        .expect("invalid player geometry is a command rejection, not a run error");
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

fn simulation_with_fixed_substrate() -> (Simulation, EntityId) {
    let mut simulation = simulation();
    let substrate = expect_created(&mut simulation, fixed_substrate_command());
    (simulation, substrate)
}

fn simulation_with_not_gate() -> (Simulation, EntityId, GateId) {
    let (mut simulation, substrate) = simulation_with_fixed_substrate();
    let domain = RoutingDomain::FixedSubstrate(substrate);
    let gate = GateId(expect_created(
        &mut simulation,
        gate(domain, GateType::Not, point(0, 0)),
    ));
    (simulation, substrate, gate)
}

#[test]
fn fixed_domain_accepts_circuit_pitch_and_rejects_quantum_only_coordinates() {
    let (mut accepted, substrate) = simulation_with_fixed_substrate();
    let circuit_only = point(CIRCUIT_PITCH, 0);
    assert_eq!(circuit_only.x.0.rem_euclid(WORLD_PITCH), CIRCUIT_PITCH);
    expect_created(
        &mut accepted,
        junction(RoutingDomain::FixedSubstrate(substrate), circuit_only),
    );

    let (mut rejected, substrate) = simulation_with_fixed_substrate();
    let quantum_only = point(QUANTUM, 0);
    assert_eq!(quantum_only.x.0.rem_euclid(QUANTUM), 0);
    assert_ne!(quantum_only.x.0.rem_euclid(CIRCUIT_PITCH), 0);
    expect_rejected(
        &mut rejected,
        junction(RoutingDomain::FixedSubstrate(substrate), quantum_only),
        CommandRejectionReason::InvalidRoutingPitch,
    );
}

#[test]
fn every_structural_placement_rejects_off_quantum_coordinates() {
    let mut substrate = simulation();
    expect_rejected(
        &mut substrate,
        Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
            origin: point(1, 0),
            routing_area: FixedAabb::new(
                point(-WORLD_PITCH, -WORLD_PITCH),
                point(WORLD_PITCH, WORLD_PITCH),
            ),
            footprint: FixedAabb::new(
                point(-WORLD_PITCH, -WORLD_PITCH),
                point(WORLD_PITCH, WORLD_PITCH),
            ),
        }),
        CommandRejectionReason::InvalidGeometryQuantum,
    );

    let mut wire_simulation = simulation();
    expect_rejected(
        &mut wire_simulation,
        wire(
            RoutingDomain::OpenWorld,
            vec![point(1, 0), point(WORLD_PITCH, 0)],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
        CommandRejectionReason::InvalidGeometryQuantum,
    );

    let mut junction_simulation = simulation();
    expect_rejected(
        &mut junction_simulation,
        junction(RoutingDomain::OpenWorld, point(-1, 0)),
        CommandRejectionReason::InvalidGeometryQuantum,
    );

    let (mut gate_simulation, substrate) = simulation_with_fixed_substrate();
    expect_rejected(
        &mut gate_simulation,
        gate(
            RoutingDomain::FixedSubstrate(substrate),
            GateType::Not,
            point(1, 0),
        ),
        CommandRejectionReason::InvalidGeometryQuantum,
    );
}

#[test]
fn substrate_origin_routing_pitch_and_footprint_quantization_are_distinct() {
    let bounds = FixedAabb::new(
        point(-15 * CIRCUIT_PITCH, -15 * CIRCUIT_PITCH),
        point(15 * CIRCUIT_PITCH, 15 * CIRCUIT_PITCH),
    );
    let footprint = FixedAabb::new(
        point(-4 * WORLD_PITCH, -4 * WORLD_PITCH),
        point(4 * WORLD_PITCH, 4 * WORLD_PITCH),
    );

    let mut origin = simulation();
    expect_rejected(
        &mut origin,
        Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
            origin: point(QUANTUM, 0),
            routing_area: bounds,
            footprint,
        }),
        CommandRejectionReason::InvalidRoutingPitch,
    );

    let mut routing_bound = simulation();
    expect_rejected(
        &mut routing_bound,
        Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
            origin: point(0, 0),
            routing_area: FixedAabb::new(
                point(-15 * CIRCUIT_PITCH + QUANTUM, -15 * CIRCUIT_PITCH),
                point(15 * CIRCUIT_PITCH, 15 * CIRCUIT_PITCH),
            ),
            footprint,
        }),
        CommandRejectionReason::InvalidRoutingPitch,
    );

    let mut quantum_only_footprint = simulation();
    let footprint_extent = 4 * WORLD_PITCH - QUANTUM;
    assert_ne!(footprint_extent.rem_euclid(CIRCUIT_PITCH), 0);
    expect_created(
        &mut quantum_only_footprint,
        Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
            origin: point(0, 0),
            routing_area: bounds,
            footprint: FixedAabb::new(
                point(-footprint_extent, -footprint_extent),
                point(footprint_extent, footprint_extent),
            ),
        }),
    );
}

#[test]
fn negative_fixed_domain_coordinates_translate_without_rounding() {
    let mut simulation = simulation();
    let origin = point(-8 * WORLD_PITCH, -6 * WORLD_PITCH);
    let substrate = expect_created(
        &mut simulation,
        Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
            origin,
            routing_area: FixedAabb::new(
                point(-8 * CIRCUIT_PITCH, -8 * CIRCUIT_PITCH),
                point(8 * CIRCUIT_PITCH, 8 * CIRCUIT_PITCH),
            ),
            footprint: FixedAabb::new(
                point(-2 * WORLD_PITCH, -2 * WORLD_PITCH),
                point(2 * WORLD_PITCH, 2 * WORLD_PITCH),
            ),
        }),
    );
    let domain = RoutingDomain::FixedSubstrate(substrate);
    expect_created(&mut simulation, gate(domain, GateType::Not, origin));
    expect_created(
        &mut simulation,
        junction(domain, point(origin.x.0 - 4 * CIRCUIT_PITCH, origin.y.0)),
    );
}

#[test]
fn malformed_substrate_and_out_of_bounds_fixed_geometry_have_specific_reasons() {
    let mut malformed = simulation();
    expect_rejected(
        &mut malformed,
        Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
            origin: point(0, 0),
            routing_area: FixedAabb::new(point(0, 0), point(0, CIRCUIT_PITCH)),
            footprint: FixedAabb::new(
                point(-WORLD_PITCH, -WORLD_PITCH),
                point(WORLD_PITCH, WORLD_PITCH),
            ),
        }),
        CommandRejectionReason::InvalidGeometryShape,
    );

    let (mut out_of_bounds, substrate) = simulation_with_fixed_substrate();
    expect_rejected(
        &mut out_of_bounds,
        junction(
            RoutingDomain::FixedSubstrate(substrate),
            point(16 * CIRCUIT_PITCH, 0),
        ),
        CommandRejectionReason::SubstrateBoundsViolation,
    );
}

#[test]
fn wire_shape_rejections_distinguish_short_zero_and_self_overlap() {
    for points in [Vec::new(), vec![point(0, 0)]] {
        let mut simulation = simulation();
        expect_rejected(
            &mut simulation,
            wire(
                RoutingDomain::OpenWorld,
                points,
                EndpointTarget::Free,
                EndpointTarget::Free,
            ),
            CommandRejectionReason::InvalidGeometryShape,
        );
    }

    let mut zero = simulation();
    expect_rejected(
        &mut zero,
        wire(
            RoutingDomain::OpenWorld,
            vec![point(0, 0), point(0, 0)],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
        CommandRejectionReason::ZeroLengthSegment,
    );

    let mut self_overlap = simulation();
    expect_rejected(
        &mut self_overlap,
        wire(
            RoutingDomain::OpenWorld,
            vec![
                point(0, 0),
                point(2 * WORLD_PITCH, 0),
                point(WORLD_PITCH, 0),
            ],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
        CommandRejectionReason::GeometryOverlap,
    );
}

#[test]
fn every_zero_segment_position_and_nonadjacent_self_overlap_reject() {
    for points in [
        vec![
            point(0, 0),
            point(0, 0),
            point(WORLD_PITCH, 0),
            point(2 * WORLD_PITCH, 0),
        ],
        vec![
            point(0, 0),
            point(WORLD_PITCH, 0),
            point(WORLD_PITCH, 0),
            point(2 * WORLD_PITCH, 0),
        ],
        vec![
            point(0, 0),
            point(WORLD_PITCH, 0),
            point(2 * WORLD_PITCH, 0),
            point(2 * WORLD_PITCH, 0),
        ],
    ] {
        let mut simulation = simulation();
        expect_rejected(
            &mut simulation,
            wire(
                RoutingDomain::OpenWorld,
                points,
                EndpointTarget::Free,
                EndpointTarget::Free,
            ),
            CommandRejectionReason::ZeroLengthSegment,
        );
    }

    let mut nonadjacent = simulation();
    expect_rejected(
        &mut nonadjacent,
        wire(
            RoutingDomain::OpenWorld,
            vec![
                point(0, 0),
                point(3 * WORLD_PITCH, 0),
                point(3 * WORLD_PITCH, WORLD_PITCH),
                point(2 * WORLD_PITCH, WORLD_PITCH),
                point(2 * WORLD_PITCH, 0),
                point(WORLD_PITCH, 0),
            ],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
        CommandRejectionReason::GeometryOverlap,
    );
}

#[test]
fn distinct_wire_overlap_and_parallel_spacing_are_rejected_separately() {
    let mut overlap = simulation();
    expect_created(
        &mut overlap,
        wire(
            RoutingDomain::OpenWorld,
            vec![point(0, 0), point(2 * WORLD_PITCH, 0)],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
    );
    expect_rejected(
        &mut overlap,
        wire(
            RoutingDomain::OpenWorld,
            vec![point(WORLD_PITCH, 0), point(3 * WORLD_PITCH, 0)],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
        CommandRejectionReason::GeometryOverlap,
    );

    let mut spacing = simulation();
    expect_created(
        &mut spacing,
        wire(
            RoutingDomain::OpenWorld,
            vec![
                point(-WORLD_PITCH, -WORLD_PITCH),
                point(WORLD_PITCH, WORLD_PITCH),
            ],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
    );
    expect_rejected(
        &mut spacing,
        wire(
            RoutingDomain::OpenWorld,
            vec![point(-WORLD_PITCH, 0), point(WORLD_PITCH, 2 * WORLD_PITCH)],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
        CommandRejectionReason::InsufficientSpacing,
    );
}

#[test]
fn nonadjacent_segments_of_one_wire_obey_parallel_spacing() {
    let mut too_close = simulation();
    expect_rejected(
        &mut too_close,
        wire(
            RoutingDomain::OpenWorld,
            vec![
                point(0, QUANTUM),
                point(WORLD_PITCH, 0),
                point(2 * WORLD_PITCH, 0),
                point(WORLD_PITCH, QUANTUM),
            ],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
        CommandRejectionReason::InsufficientSpacing,
    );

    let mut exact_pitch = simulation();
    expect_created(
        &mut exact_pitch,
        wire(
            RoutingDomain::OpenWorld,
            vec![
                point(0, 0),
                point(WORLD_PITCH, 0),
                point(WORLD_PITCH, WORLD_PITCH),
                point(0, WORLD_PITCH),
            ],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
    );
}

#[test]
fn coordinate_equal_free_fanout_is_spacing_exempt_but_remains_disconnected() {
    let first_points = vec![point(0, 0), point(WORLD_PITCH, WORLD_PITCH)];
    let second_points = vec![point(0, 0), point(-WORLD_PITCH, -WORLD_PITCH)];

    let mut free = simulation();
    expect_created(&mut free, junction(RoutingDomain::OpenWorld, point(0, 0)));
    expect_created(
        &mut free,
        wire(
            RoutingDomain::OpenWorld,
            first_points.clone(),
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
    );
    expect_created(
        &mut free,
        wire(
            RoutingDomain::OpenWorld,
            second_points.clone(),
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
    );

    let mut connected = simulation();
    let junction = JunctionId(expect_created(
        &mut connected,
        junction(RoutingDomain::OpenWorld, point(0, 0)),
    ));
    let target = EndpointTarget::Junction(junction);
    expect_created(
        &mut connected,
        wire(
            RoutingDomain::OpenWorld,
            first_points.clone(),
            target,
            EndpointTarget::Free,
        ),
    );
    expect_created(
        &mut connected,
        wire(
            RoutingDomain::OpenWorld,
            second_points.clone(),
            target,
            EndpointTarget::Free,
        ),
    );

    assert_eq!(free.next_tick(), connected.next_tick());
    assert_eq!(free.topology_revision(), connected.topology_revision());
    assert_ne!(free.state_hash(), connected.state_hash());

    let mut separated = simulation();
    expect_created(
        &mut separated,
        wire(
            RoutingDomain::OpenWorld,
            first_points,
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
    );
    expect_rejected(
        &mut separated,
        wire(
            RoutingDomain::OpenWorld,
            vec![point(0, WORLD_PITCH), point(-WORLD_PITCH, 0)],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
        CommandRejectionReason::InsufficientSpacing,
    );
}

#[test]
fn a_shared_endpoint_coordinate_never_exempts_positive_length_overlap() {
    let mut simulation = simulation();
    expect_created(
        &mut simulation,
        wire(
            RoutingDomain::OpenWorld,
            vec![point(0, 0), point(2 * WORLD_PITCH, 0)],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
    );
    expect_rejected(
        &mut simulation,
        wire(
            RoutingDomain::OpenWorld,
            vec![point(0, 0), point(WORLD_PITCH, 0)],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
        CommandRejectionReason::GeometryOverlap,
    );
}

#[test]
fn wire_endpoints_need_only_quantum_but_internal_vertices_still_need_domain_pitch() {
    let (mut endpoint, substrate) = simulation_with_fixed_substrate();
    let domain = RoutingDomain::FixedSubstrate(substrate);
    expect_created(
        &mut endpoint,
        wire(
            domain,
            vec![point(QUANTUM, 0), point(CIRCUIT_PITCH, 0)],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
    );

    let (mut internal, substrate) = simulation_with_fixed_substrate();
    expect_rejected(
        &mut internal,
        wire(
            RoutingDomain::FixedSubstrate(substrate),
            vec![point(0, 0), point(QUANTUM, 0), point(CIRCUIT_PITCH, 0)],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
        CommandRejectionReason::InvalidRoutingPitch,
    );
}

#[test]
fn point_crossing_is_allowed_without_implicit_junction_creation() {
    let mut simulation = simulation();
    expect_created(
        &mut simulation,
        wire(
            RoutingDomain::OpenWorld,
            vec![point(-WORLD_PITCH, 0), point(WORLD_PITCH, 0)],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
    );
    expect_created(
        &mut simulation,
        wire(
            RoutingDomain::OpenWorld,
            vec![point(0, -WORLD_PITCH), point(0, WORLD_PITCH)],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
    );

    let mut snapshot = RenderSnapshot::default();
    simulation.write_render_snapshot(&mut snapshot);
    assert_eq!(snapshot.primitive_count(), 2);
}

#[test]
fn junction_cannot_be_placed_in_a_wire_segment_strict_interior() {
    let mut simulation = simulation();
    expect_created(
        &mut simulation,
        wire(
            RoutingDomain::OpenWorld,
            vec![point(-WORLD_PITCH, 0), point(WORLD_PITCH, 0)],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
    );
    expect_rejected(
        &mut simulation,
        junction(RoutingDomain::OpenWorld, point(0, 0)),
        CommandRejectionReason::GeometryOverlap,
    );
}

#[test]
fn endpoint_position_and_domain_must_match_the_explicit_junction() {
    let mut wrong_position = simulation();
    let open_junction = JunctionId(expect_created(
        &mut wrong_position,
        junction(RoutingDomain::OpenWorld, point(0, 0)),
    ));
    expect_rejected(
        &mut wrong_position,
        wire(
            RoutingDomain::OpenWorld,
            vec![point(WORLD_PITCH, 0), point(2 * WORLD_PITCH, 0)],
            EndpointTarget::Junction(open_junction),
            EndpointTarget::Free,
        ),
        CommandRejectionReason::InvalidEndpoint,
    );

    let (mut wrong_domain, substrate) = simulation_with_fixed_substrate();
    let fixed_junction = JunctionId(expect_created(
        &mut wrong_domain,
        junction(RoutingDomain::FixedSubstrate(substrate), point(0, 0)),
    ));
    expect_rejected(
        &mut wrong_domain,
        wire(
            RoutingDomain::OpenWorld,
            vec![point(0, 0), point(WORLD_PITCH, 0)],
            EndpointTarget::Junction(fixed_junction),
            EndpointTarget::Free,
        ),
        CommandRejectionReason::InvalidEndpoint,
    );
}

#[test]
fn gate_port_validation_distinguishes_exact_position_and_missing_not_input_b() {
    let (mut exact, substrate, gate) = simulation_with_not_gate();
    let domain = RoutingDomain::FixedSubstrate(substrate);
    expect_created(
        &mut exact,
        wire(
            domain,
            vec![point(-CIRCUIT_PITCH, 0), point(-2 * CIRCUIT_PITCH, 0)],
            EndpointTarget::GatePort(GatePortRef {
                gate,
                port: GatePort::InputA,
            }),
            EndpointTarget::Free,
        ),
    );

    let (mut wrong_position, substrate, gate) = simulation_with_not_gate();
    expect_rejected(
        &mut wrong_position,
        wire(
            RoutingDomain::FixedSubstrate(substrate),
            vec![point(-2 * CIRCUIT_PITCH, 0), point(-3 * CIRCUIT_PITCH, 0)],
            EndpointTarget::GatePort(GatePortRef {
                gate,
                port: GatePort::InputA,
            }),
            EndpointTarget::Free,
        ),
        CommandRejectionReason::InvalidPortBinding,
    );

    let (mut missing_port, substrate, gate) = simulation_with_not_gate();
    expect_rejected(
        &mut missing_port,
        wire(
            RoutingDomain::FixedSubstrate(substrate),
            vec![point(-CIRCUIT_PITCH, 0), point(-2 * CIRCUIT_PITCH, 0)],
            EndpointTarget::GatePort(GatePortRef {
                gate,
                port: GatePort::InputB,
            }),
            EndpointTarget::Free,
        ),
        CommandRejectionReason::InvalidPort,
    );
}

#[test]
fn off_circuit_profile_gate_anchor_is_a_physical_endpoint_pitch_exception() {
    let (mut simulation, substrate) = simulation_with_fixed_substrate();
    let domain = RoutingDomain::FixedSubstrate(substrate);
    expect_created(&mut simulation, gate(domain, GateType::And, point(0, 0)));
    let input_a = point(-CIRCUIT_PITCH, -8 * QUANTUM);
    assert_eq!(input_a.x.0.rem_euclid(CIRCUIT_PITCH), 0);
    assert_eq!(input_a.y.0.rem_euclid(QUANTUM), 0);
    assert_ne!(input_a.y.0.rem_euclid(CIRCUIT_PITCH), 0);

    expect_created(
        &mut simulation,
        wire(
            domain,
            vec![input_a, point(-2 * CIRCUIT_PITCH, 0)],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
    );
}

#[test]
fn free_exact_anchor_contact_accepts_but_other_gate_contacts_reject() {
    let (mut anchor, substrate, _) = simulation_with_not_gate();
    expect_created(
        &mut anchor,
        wire(
            RoutingDomain::FixedSubstrate(substrate),
            vec![point(CIRCUIT_PITCH, 0), point(2 * CIRCUIT_PITCH, 0)],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
    );

    let (mut reverse_order, substrate) = simulation_with_fixed_substrate();
    let domain = RoutingDomain::FixedSubstrate(substrate);
    expect_created(
        &mut reverse_order,
        wire(
            domain,
            vec![point(CIRCUIT_PITCH, 0), point(2 * CIRCUIT_PITCH, 0)],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
    );
    expect_created(&mut reverse_order, gate(domain, GateType::Not, point(0, 0)));

    let (mut interior, substrate, _) = simulation_with_not_gate();
    expect_rejected(
        &mut interior,
        wire(
            RoutingDomain::FixedSubstrate(substrate),
            vec![point(-2 * CIRCUIT_PITCH, 0), point(2 * CIRCUIT_PITCH, 0)],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
        CommandRejectionReason::GeometryOverlap,
    );

    let (mut boundary, substrate, _) = simulation_with_not_gate();
    expect_rejected(
        &mut boundary,
        wire(
            RoutingDomain::FixedSubstrate(substrate),
            vec![
                point(-2 * CIRCUIT_PITCH, CIRCUIT_PITCH),
                point(-CIRCUIT_PITCH, CIRCUIT_PITCH),
            ],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
        CommandRejectionReason::InvalidPortBinding,
    );

    let (mut edge_run, substrate, _) = simulation_with_not_gate();
    expect_rejected(
        &mut edge_run,
        wire(
            RoutingDomain::FixedSubstrate(substrate),
            vec![point(CIRCUIT_PITCH, 0), point(CIRCUIT_PITCH, CIRCUIT_PITCH)],
            EndpointTarget::Free,
            EndpointTarget::Free,
        ),
        CommandRejectionReason::GeometryOverlap,
    );
}

#[test]
fn extreme_disjoint_wires_accept_in_both_insertion_orders() {
    let aligned_maximum = i64::MAX - i64::MAX.rem_euclid(QUANTUM);
    let near_minimum = vec![
        point(i64::MIN, i64::MIN),
        point(i64::MIN + QUANTUM, i64::MIN),
    ];
    let near_maximum = vec![
        point(aligned_maximum - QUANTUM, aligned_maximum),
        point(aligned_maximum, aligned_maximum),
    ];

    for (first, second) in [
        (near_minimum.clone(), near_maximum.clone()),
        (near_maximum, near_minimum),
    ] {
        let mut simulation = simulation();
        expect_created(
            &mut simulation,
            wire(
                RoutingDomain::OpenWorld,
                first,
                EndpointTarget::Free,
                EndpointTarget::Free,
            ),
        );
        expect_created(
            &mut simulation,
            wire(
                RoutingDomain::OpenWorld,
                second,
                EndpointTarget::Free,
                EndpointTarget::Free,
            ),
        );

        let mut snapshot = RenderSnapshot::default();
        simulation.write_render_snapshot(&mut snapshot);
        assert_eq!(snapshot.primitive_count(), 2);
    }
}

#[test]
fn substrate_footprint_boundary_contact_accepts_but_open_interior_overlap_rejects() {
    let substrate_at = |x| {
        let Command::PlaceFixedSubstrate(mut command) = fixed_substrate_command() else {
            unreachable!("fixture command is a fixed substrate")
        };
        command.origin = point(x, 0);
        Command::PlaceFixedSubstrate(command)
    };

    let mut touching = simulation();
    expect_created(&mut touching, substrate_at(0));
    expect_created(&mut touching, substrate_at(8 * WORLD_PITCH));

    let mut overlapping = simulation();
    expect_created(&mut overlapping, substrate_at(0));
    expect_rejected(
        &mut overlapping,
        substrate_at(7 * WORLD_PITCH),
        CommandRejectionReason::GeometryOverlap,
    );
}
