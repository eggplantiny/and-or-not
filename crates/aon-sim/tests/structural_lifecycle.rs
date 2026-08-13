use aon_sim::{
    ArtifactBytes, BindPortCommand, Command, CommandAcceptance, CommandEnvelope, CommandRejection,
    CommandRejectionReason, DriveStrength, DriverId, EndpointTarget, EntityId, Fixed, FixedAabb,
    FixedVec2, GateId, GatePort, GatePortRef, GateType, JunctionId, LogicLevel,
    PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceJunctionCommand,
    PlaceMobileSubstrateCommand, PlaceWireCommand, RemoveEntityCommand, Revision, RoutingDomain,
    SetExternalDriverCommand, Simulation, SimulationContract, SimulationPackage, Tick, WireEnd,
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
const CIRCUIT_PITCH: i64 = 16_384;
const WORLD_PITCH: i64 = 65_536;
const SUBSTRATE_HALF_EXTENT: i64 = 4 * WORLD_PITCH;

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

fn envelope(target_tick: u64, ordinal: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        target_tick: Tick(target_tick),
        ordinal,
        command,
    }
}

fn substrate_payload() -> PlaceFixedSubstrateCommand {
    let bounds = FixedAabb::new(
        point(-SUBSTRATE_HALF_EXTENT, -SUBSTRATE_HALF_EXTENT),
        point(SUBSTRATE_HALF_EXTENT, SUBSTRATE_HALF_EXTENT),
    );
    PlaceFixedSubstrateCommand {
        origin: point(0, 0),
        routing_area: bounds,
        footprint: bounds,
    }
}

fn remove(target: u64) -> Command {
    Command::RemoveEntity(RemoveEntityCommand {
        target: EntityId(target),
    })
}

fn bind(wire: u64, end: WireEnd, target: EndpointTarget) -> Command {
    Command::BindPort(BindPortCommand {
        wire: WireId(EntityId(wire)),
        end,
        target,
    })
}

fn simulation_with_substrate() -> Simulation {
    let mut simulation = simulation();
    let report = simulation
        .step(&[envelope(
            0,
            0,
            Command::PlaceFixedSubstrate(substrate_payload()),
        )])
        .expect("the fixture substrate is valid");
    assert_eq!(
        report.command_acceptances[0].created_entity,
        Some(EntityId(1))
    );
    simulation
}

fn simulation_with_open_wire(endpoint_a: EndpointTarget) -> Simulation {
    let mut simulation = simulation();
    simulation
        .step(&[envelope(
            0,
            0,
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: point(0, 0),
            }),
        )])
        .expect("the fixture Junction is valid");
    let wire = simulation
        .step(&[envelope(
            1,
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![point(0, 0), point(WORLD_PITCH, 0)],
                endpoint_a,
                endpoint_b: EndpointTarget::Free,
            }),
        )])
        .expect("the fixture Wire is valid");
    assert_eq!(
        wire.command_acceptances[0].created_entity,
        Some(EntityId(2))
    );
    simulation
}

#[test]
fn heterogeneous_rejections_are_permutation_invariant_and_unsupported_commands_are_inert() {
    let bounds = substrate_payload();
    let commands = [
        envelope(3, 99, remove(77)),
        envelope(0, 30, remove(77)),
        envelope(
            0,
            20,
            Command::SetExternalDriver(SetExternalDriverCommand {
                driver: DriverId(EntityId(77)),
                level: LogicLevel::X,
                strength: DriveStrength(1),
            }),
        ),
        envelope(
            0,
            10,
            Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
                origin: bounds.origin,
                routing_area: bounds.routing_area,
                footprint: bounds.footprint,
            }),
        ),
    ];
    let expected = vec![
        CommandRejection {
            target_tick: Tick(0),
            ordinal: 10,
            reason: CommandRejectionReason::UnsupportedPlacement,
        },
        CommandRejection {
            target_tick: Tick(0),
            ordinal: 20,
            reason: CommandRejectionReason::UnknownDriver,
        },
        CommandRejection {
            target_tick: Tick(0),
            ordinal: 30,
            reason: CommandRejectionReason::UnknownEntity,
        },
        CommandRejection {
            target_tick: Tick(3),
            ordinal: 99,
            reason: CommandRejectionReason::WrongTick,
        },
    ];
    let mut baseline_hash = None;

    for first in 0..commands.len() {
        for second in 0..commands.len() {
            for third in 0..commands.len() {
                for fourth in 0..commands.len() {
                    let permutation = [first, second, third, fourth];
                    if permutation.iter().any(|index| {
                        permutation
                            .iter()
                            .filter(|candidate| *candidate == index)
                            .count()
                            != 1
                    }) {
                        continue;
                    }
                    let input: Vec<_> = permutation
                        .into_iter()
                        .map(|index| commands[index].clone())
                        .collect();
                    let mut simulation = simulation();
                    let report = simulation
                        .step(&input)
                        .expect("ordinary rejections do not fail the phase");

                    assert!(report.command_acceptances.is_empty());
                    assert_eq!(report.command_rejections, expected);
                    assert!(!report.topology_changed);
                    assert_eq!(simulation.topology_revision(), Revision(0));
                    match baseline_hash {
                        Some(hash) => assert_eq!(simulation.state_hash(), hash),
                        None => baseline_hash = Some(simulation.state_hash()),
                    }
                }
            }
        }
    }

    let mut no_id_consumption = simulation();
    no_id_consumption
        .step(&commands)
        .expect("the inert rejection batch succeeds");
    let retry = no_id_consumption
        .step(&[envelope(
            1,
            0,
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: point(0, 0),
            }),
        )])
        .expect("unsupported commands consume no EntityId");
    assert_eq!(
        retry.command_acceptances[0].created_entity,
        Some(EntityId(1))
    );
}

#[test]
fn empty_substrate_add_and_remove_are_revision_neutral() {
    let mut simulation = simulation();
    let creation = simulation
        .step(&[envelope(
            0,
            4,
            Command::PlaceFixedSubstrate(substrate_payload()),
        )])
        .expect("empty substrate placement succeeds");

    assert_eq!(
        creation.command_acceptances,
        vec![CommandAcceptance {
            target_tick: Tick(0),
            ordinal: 4,
            created_entity: Some(EntityId(1)),
        }]
    );
    assert!(!creation.topology_changed);
    assert_eq!(simulation.topology_revision(), Revision(0));

    let removal = simulation
        .step(&[envelope(1, 8, remove(1))])
        .expect("empty substrate removal succeeds");
    assert_eq!(
        removal.command_acceptances,
        vec![CommandAcceptance {
            target_tick: Tick(1),
            ordinal: 8,
            created_entity: None,
        }]
    );
    assert!(removal.command_rejections.is_empty());
    assert!(!removal.topology_changed);
    assert_eq!(simulation.topology_revision(), Revision(0));
}

#[test]
fn nonempty_substrate_removal_rejects_without_mutation() {
    let mut under_test = simulation_with_substrate();
    let mut untouched = simulation_with_substrate();
    let junction = envelope(
        1,
        0,
        Command::PlaceJunction(PlaceJunctionCommand {
            routing_domain: RoutingDomain::FixedSubstrate(EntityId(1)),
            position: point(0, 0),
        }),
    );
    under_test
        .step(std::slice::from_ref(&junction))
        .expect("the substrate Junction is valid");
    untouched
        .step(&[junction])
        .expect("the comparison Junction is valid");
    let revision_before = under_test.topology_revision();

    let rejection = under_test
        .step(&[envelope(2, 3, remove(1))])
        .expect("SubstrateInUse is an ordinary rejection");
    untouched.step(&[]).expect("empty comparison step succeeds");

    assert!(rejection.command_acceptances.is_empty());
    assert_eq!(
        rejection.command_rejections,
        vec![CommandRejection {
            target_tick: Tick(2),
            ordinal: 3,
            reason: CommandRejectionReason::SubstrateInUse,
        }]
    );
    assert!(!rejection.topology_changed);
    assert_eq!(under_test.topology_revision(), revision_before);
    assert_eq!(under_test.state_hash(), untouched.state_hash());
}

fn simulation_with_gate_junction_and_wire() -> Simulation {
    let mut simulation = simulation_with_substrate();
    let domain = RoutingDomain::FixedSubstrate(EntityId(1));
    simulation
        .step(&[
            envelope(
                1,
                1,
                Command::PlaceGate(PlaceGateCommand {
                    gate_type: GateType::Not,
                    origin: point(0, 0),
                    routing_domain: domain,
                }),
            ),
            envelope(
                1,
                2,
                Command::PlaceJunction(PlaceJunctionCommand {
                    routing_domain: domain,
                    position: point(CIRCUIT_PITCH, 0),
                }),
            ),
        ])
        .expect("the fixture Gate and Junction are valid");
    let wire = simulation
        .step(&[envelope(
            2,
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: domain,
                points: vec![point(CIRCUIT_PITCH, 0), point(2 * CIRCUIT_PITCH, 0)],
                endpoint_a: EndpointTarget::GatePort(GatePortRef {
                    gate: GateId(EntityId(2)),
                    port: GatePort::Output,
                }),
                endpoint_b: EndpointTarget::Free,
            }),
        )])
        .expect("the fixture Gate-bound Wire is valid");
    assert_eq!(
        wire.command_acceptances[0].created_entity,
        Some(EntityId(4))
    );
    simulation
}

fn simulation_with_binary_gate_and_off_pitch_wire(gate_type: GateType) -> Simulation {
    let mut simulation = simulation_with_substrate();
    let domain = RoutingDomain::FixedSubstrate(EntityId(1));
    let gate = simulation
        .step(&[envelope(
            1,
            0,
            Command::PlaceGate(PlaceGateCommand {
                gate_type,
                origin: point(0, 0),
                routing_domain: domain,
            }),
        )])
        .expect("the binary Gate fixture is valid");
    assert_eq!(
        gate.command_acceptances[0].created_entity,
        Some(EntityId(2))
    );

    let input_a = point(-CIRCUIT_PITCH, -8 * QUANTUM);
    assert_eq!(input_a.x.0.rem_euclid(CIRCUIT_PITCH), 0);
    assert_eq!(input_a.y.0.rem_euclid(QUANTUM), 0);
    assert_ne!(input_a.y.0.rem_euclid(CIRCUIT_PITCH), 0);
    let wire = simulation
        .step(&[envelope(
            2,
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: domain,
                points: vec![input_a, point(-2 * CIRCUIT_PITCH, 0)],
                endpoint_a: EndpointTarget::GatePort(GatePortRef {
                    gate: GateId(EntityId(2)),
                    port: GatePort::InputA,
                }),
                endpoint_b: EndpointTarget::Free,
            }),
        )])
        .expect("the off-pitch GatePort Wire fixture is valid");
    assert_eq!(
        wire.command_acceptances[0].created_entity,
        Some(EntityId(3))
    );
    simulation
}

#[test]
fn gate_port_endpoint_can_be_directly_unbound_to_free() {
    let mut simulation = simulation_with_gate_junction_and_wire();
    let report = simulation
        .step(&[envelope(3, 7, bind(4, WireEnd::A, EndpointTarget::Free))])
        .expect("GatePort to Free is an effective connectivity edit");

    assert_eq!(
        report.command_acceptances,
        vec![CommandAcceptance {
            target_tick: Tick(3),
            ordinal: 7,
            created_entity: None,
        }]
    );
    assert!(report.command_rejections.is_empty());
    assert!(report.topology_changed);
    assert_eq!(simulation.topology_revision(), Revision(3));
}

#[test]
fn binary_gate_removal_and_explicit_unbind_preserve_the_same_off_pitch_wire_state() {
    for gate_type in [GateType::And, GateType::Or] {
        let mut automatic = simulation_with_binary_gate_and_off_pitch_wire(gate_type);
        let mut explicit = simulation_with_binary_gate_and_off_pitch_wire(gate_type);

        let automatic_report = automatic
            .step(&[envelope(3, 2, remove(2))])
            .expect("Gate removal frees its off-pitch endpoint");
        let explicit_report = explicit
            .step(&[
                envelope(3, 1, bind(3, WireEnd::A, EndpointTarget::Free)),
                envelope(3, 2, remove(2)),
            ])
            .expect("explicit unbind followed by Gate removal succeeds atomically");

        assert!(automatic_report.command_rejections.is_empty());
        assert!(explicit_report.command_rejections.is_empty());
        assert_eq!(automatic_report.command_acceptances.len(), 1);
        assert_eq!(explicit_report.command_acceptances.len(), 2);
        assert!(automatic_report.topology_changed);
        assert!(explicit_report.topology_changed);
        assert_eq!(automatic.topology_revision(), Revision(3));
        assert_eq!(explicit.topology_revision(), Revision(3));
        assert_eq!(automatic.state_hash(), explicit.state_hash());

        let mut snapshot = aon_sim::RenderSnapshot::default();
        automatic.write_render_snapshot(&mut snapshot);
        assert_eq!(snapshot.primitive_count(), 2);
    }
}

fn simulation_with_bound_junction_fanout() -> Simulation {
    let mut simulation = simulation();
    simulation
        .step(&[envelope(
            0,
            0,
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: point(0, 0),
            }),
        )])
        .expect("the fan-out Junction is valid");
    let target = EndpointTarget::Junction(JunctionId(EntityId(1)));
    let report = simulation
        .step(&[
            envelope(
                1,
                1,
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: RoutingDomain::OpenWorld,
                    points: vec![point(0, 0), point(WORLD_PITCH, WORLD_PITCH)],
                    endpoint_a: target,
                    endpoint_b: EndpointTarget::Free,
                }),
            ),
            envelope(
                1,
                2,
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: RoutingDomain::OpenWorld,
                    points: vec![point(0, 0), point(-WORLD_PITCH, -WORLD_PITCH)],
                    endpoint_a: target,
                    endpoint_b: EndpointTarget::Free,
                }),
            ),
        ])
        .expect("the coordinate-equal Junction fan-out is valid");
    assert_eq!(
        report
            .command_acceptances
            .iter()
            .map(|acceptance| acceptance.created_entity)
            .collect::<Vec<_>>(),
        vec![Some(EntityId(2)), Some(EntityId(3))]
    );
    simulation
}

#[test]
fn junction_removal_preserves_coordinate_based_fanout_geometry() {
    let mut automatic = simulation_with_bound_junction_fanout();
    let mut explicit = simulation_with_bound_junction_fanout();

    automatic
        .step(&[envelope(2, 3, remove(1))])
        .expect("Junction removal frees both fan-out endpoints");
    let explicit_report = explicit
        .step(&[
            envelope(2, 1, bind(2, WireEnd::A, EndpointTarget::Free)),
            envelope(2, 2, bind(3, WireEnd::A, EndpointTarget::Free)),
            envelope(2, 3, remove(1)),
        ])
        .expect("explicit fan-out unbinds and Junction removal succeed in one phase");

    assert!(explicit_report.command_rejections.is_empty());
    assert_eq!(explicit_report.command_acceptances.len(), 3);
    assert_eq!(automatic.topology_revision(), Revision(3));
    assert_eq!(explicit.topology_revision(), Revision(3));
    assert_eq!(automatic.state_hash(), explicit.state_hash());
}

#[test]
fn gate_and_junction_removal_free_incident_wire_endpoints() {
    let junction_target = EndpointTarget::Junction(JunctionId(EntityId(3)));
    let mut removed_gate = simulation_with_gate_junction_and_wire();
    let mut explicitly_rebound = simulation_with_gate_junction_and_wire();

    let automatic = removed_gate
        .step(&[envelope(3, 1, remove(2)), envelope(3, 4, remove(3))])
        .expect("Gate and Junction removal succeeds");
    let explicit = explicitly_rebound
        .step(&[
            envelope(3, 1, remove(2)),
            envelope(3, 2, bind(4, WireEnd::A, junction_target)),
            envelope(3, 3, bind(4, WireEnd::A, EndpointTarget::Free)),
            envelope(3, 4, remove(3)),
        ])
        .expect("the explicit endpoint-free comparison succeeds");

    assert!(automatic.command_rejections.is_empty());
    assert!(explicit.command_rejections.is_empty());
    assert!(automatic.topology_changed);
    assert!(explicit.topology_changed);
    assert_eq!(removed_gate.topology_revision(), Revision(3));
    assert_eq!(removed_gate.state_hash(), explicitly_rebound.state_hash());

    let bound_junction = EndpointTarget::Junction(JunctionId(EntityId(1)));
    let mut removed_junction = simulation_with_open_wire(bound_junction);
    let mut explicitly_unbound = simulation_with_open_wire(bound_junction);
    removed_junction
        .step(&[envelope(2, 2, remove(1))])
        .expect("incident Junction removal succeeds");
    explicitly_unbound
        .step(&[
            envelope(2, 1, bind(2, WireEnd::A, EndpointTarget::Free)),
            envelope(2, 2, remove(1)),
        ])
        .expect("explicit unbind then Junction removal succeeds");

    assert_eq!(
        removed_junction.state_hash(),
        explicitly_unbound.state_hash()
    );
}

#[test]
fn effective_bind_returns_no_entity_and_net_zero_binding_advances_revision_once() {
    let junction = EndpointTarget::Junction(JunctionId(EntityId(1)));
    let mut effective = simulation_with_open_wire(EndpointTarget::Free);
    let effective_report = effective
        .step(&[envelope(2, 5, bind(2, WireEnd::A, junction))])
        .expect("effective bind succeeds");

    assert_eq!(
        effective_report.command_acceptances,
        vec![CommandAcceptance {
            target_tick: Tick(2),
            ordinal: 5,
            created_entity: None,
        }]
    );
    assert!(effective_report.command_rejections.is_empty());
    assert!(effective_report.topology_changed);
    assert_eq!(effective.topology_revision(), Revision(3));

    let mut net_zero = simulation_with_open_wire(EndpointTarget::Free);
    let mut untouched = simulation_with_open_wire(EndpointTarget::Free);
    let report = net_zero
        .step(&[
            envelope(2, 1, bind(2, WireEnd::A, junction)),
            envelope(2, 2, bind(2, WireEnd::A, EndpointTarget::Free)),
        ])
        .expect("bind then unbind succeeds");
    untouched.step(&[]).expect("empty comparison step succeeds");

    assert_eq!(report.command_acceptances.len(), 2);
    assert!(
        report
            .command_acceptances
            .iter()
            .all(|acceptance| acceptance.created_entity.is_none())
    );
    assert!(report.topology_changed);
    assert_eq!(net_zero.topology_revision(), Revision(3));
    assert_eq!(untouched.topology_revision(), Revision(2));
    assert_ne!(net_zero.state_hash(), untouched.state_hash());
}

fn simulation_with_every_structural_kind() -> Simulation {
    let mut simulation = simulation_with_substrate();
    let domain = RoutingDomain::FixedSubstrate(EntityId(1));
    let placement = simulation
        .step(&[
            envelope(
                1,
                1,
                Command::PlaceGate(PlaceGateCommand {
                    gate_type: GateType::Not,
                    origin: point(0, 0),
                    routing_domain: domain,
                }),
            ),
            envelope(
                1,
                2,
                Command::PlaceJunction(PlaceJunctionCommand {
                    routing_domain: domain,
                    position: point(WORLD_PITCH, 0),
                }),
            ),
        ])
        .expect("the all-kinds Gate and Junction are valid");
    assert_eq!(
        placement
            .command_acceptances
            .iter()
            .map(|acceptance| acceptance.created_entity)
            .collect::<Vec<_>>(),
        vec![Some(EntityId(2)), Some(EntityId(3))]
    );

    let wire = simulation
        .step(&[envelope(
            2,
            1,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: domain,
                points: vec![point(CIRCUIT_PITCH, 0), point(WORLD_PITCH, 0)],
                endpoint_a: EndpointTarget::GatePort(GatePortRef {
                    gate: GateId(EntityId(2)),
                    port: GatePort::Output,
                }),
                endpoint_b: EndpointTarget::Junction(JunctionId(EntityId(3))),
            }),
        )])
        .expect("the all-kinds Wire is valid");
    assert_eq!(
        wire.command_acceptances[0].created_entity,
        Some(EntityId(4))
    );
    simulation
}

fn independently_encoded_all_kinds_state(contract: &SimulationContract) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"AON\0STATE\0V6\0");
    push_u16(&mut bytes, 6); // encoder version
    bytes.push(0); // aon-semantics-v1
    bytes.extend_from_slice(contract.numeric_profile_hash.as_bytes());
    bytes.extend_from_slice(contract.physical_scale_profile_hash.as_bytes());
    bytes.extend_from_slice(contract.balance_profile_hash.as_bytes());
    push_u64(&mut bytes, 3); // next Tick
    push_u64(&mut bytes, 2); // topology revision
    push_u64(&mut bytes, 5); // EntityId frontier
    push_u64(&mut bytes, 4); // allocated EntityId slots

    // Registry records: FixedSubstrate(5), Gate(2), Junction(4), Wire(3).
    for (id, kind) in [(1, 5), (2, 2), (3, 4), (4, 3)] {
        push_u64(&mut bytes, id);
        bytes.push(1);
        bytes.push(kind);
    }

    bytes.push(0); // no Main Core
    push_u32(&mut bytes, 0); // no Power Sources

    push_u64(&mut bytes, 1); // Gate count
    push_u64(&mut bytes, 2);
    bytes.push(2); // GateType::Not
    push_point(&mut bytes, 0, 0);
    push_fixed_domain(&mut bytes, 1);

    push_u64(&mut bytes, 1); // Wire count
    push_u64(&mut bytes, 4);
    push_fixed_domain(&mut bytes, 1);
    push_u64(&mut bytes, 0); // creation generation
    push_u32(&mut bytes, 2);
    push_point(&mut bytes, CIRCUIT_PITCH, 0);
    push_point(&mut bytes, WORLD_PITCH, 0);
    bytes.push(2); // EndpointTarget::GatePort
    push_u64(&mut bytes, 2);
    bytes.push(2); // GatePort::Output
    bytes.push(1); // EndpointTarget::Junction
    push_u64(&mut bytes, 3);

    push_u64(&mut bytes, 1); // Junction count
    push_u64(&mut bytes, 3);
    push_fixed_domain(&mut bytes, 1);
    push_point(&mut bytes, WORLD_PITCH, 0);
    push_u64(&mut bytes, 1); // Wire placement affected this live Junction once

    push_u64(&mut bytes, 1); // Fixed Substrate count
    push_u64(&mut bytes, 1);
    push_point(&mut bytes, 0, 0);
    for _ in 0..2 {
        push_point(&mut bytes, -SUBSTRATE_HALF_EXTENT, -SUBSTRATE_HALF_EXTENT);
        push_point(&mut bytes, SUBSTRATE_HALF_EXTENT, SUBSTRATE_HALF_EXTENT);
    }
    push_u64(&mut bytes, 0); // Mobile Substrate count

    // Driver allocation slots. Driver 1 is the inert external input; Driver 2 is the NOT output.
    push_u64(&mut bytes, 3); // DriverId frontier
    push_u64(&mut bytes, 2); // allocated Driver slots
    push_u64(&mut bytes, 1);
    bytes.push(1); // alive
    push_u64(&mut bytes, 2); // owner Gate
    bytes.push(0); // ExternalInputA
    bytes.push(0); // Low
    push_u64(&mut bytes, 0); // strength
    push_u64(&mut bytes, 0); // revision
    push_u64(&mut bytes, 1); // emitted Tick
    push_u64(&mut bytes, 1); // sample DriverId
    push_u64(&mut bytes, 2);
    bytes.push(1); // alive
    push_u64(&mut bytes, 2); // owner Gate
    bytes.push(2); // GateOutput
    bytes.push(1); // High
    push_u64(&mut bytes, 400); // nominal Gate drive
    push_u64(&mut bytes, 1); // revision after the startup transition
    push_u64(&mut bytes, 2); // emitted Tick
    push_u64(&mut bytes, 2); // sample DriverId

    push_u64(&mut bytes, 2); // SinkId frontier
    push_u64(&mut bytes, 1); // allocated Sink slots
    push_u64(&mut bytes, 1);
    bytes.push(1); // alive
    push_u64(&mut bytes, 2); // owner Gate
    bytes.push(0); // InputA
    bytes.push(0); // Low
    bytes.push(0); // committed dirty=false

    push_u64(&mut bytes, 1); // Gate signal count
    push_u64(&mut bytes, 2); // GateId
    push_u64(&mut bytes, 1); // input A SinkId
    push_u64(&mut bytes, 1); // input A external DriverId
    bytes.push(0); // no input B
    push_u64(&mut bytes, 2); // output DriverId
    bytes.push(1); // current High
    bytes.push(1); // desired High
    push_u32(&mut bytes, 1); // pending generation frontier
    bytes.push(0); // no pending due Tick
    bytes.push(0); // no pending level
    bytes.push(0); // no pending energy
    push_u64(&mut bytes, 0); // canceled switching heat
    push_u64(&mut bytes, 0); // unpowered Tick count

    push_u64(&mut bytes, 0); // Mobile control-port map count

    push_u64(&mut bytes, 1); // Wire excitation count
    push_u64(&mut bytes, 4); // WireId
    push_u128(&mut bytes, 400); // active High
    push_u128(&mut bytes, 0); // active Low
    push_u128(&mut bytes, 0); // active X
    push_u128(&mut bytes, 0); // previous High
    push_u128(&mut bytes, 0); // previous Low
    push_u128(&mut bytes, 0); // previous X
    bytes.push(0); // no Wire Sense state in this non-Power world

    push_u64(&mut bytes, 1); // Sink Driver slot count
    push_u64(&mut bytes, 1); // SinkId
    push_u64(&mut bytes, 1); // DriverId
    bytes.push(0); // Low
    push_u64(&mut bytes, 0); // strength
    push_u64(&mut bytes, 0); // revision
    push_u64(&mut bytes, 1); // emitted Tick
    push_u64(&mut bytes, 4); // event payload frontier; payloads 1 through 3 were drained
    push_u64(&mut bytes, 0); // pending DriverTransition count
    push_u64(&mut bytes, 0); // pending SignalArrival count

    for _ in 0..3 {
        push_u64(&mut bytes, 0); // later-stage reserved sections
    }
    push_u64(&mut bytes, 2); // PathCertificateId frontier
    push_u64(&mut bytes, 1); // allocated PathCertificate slots
    push_u64(&mut bytes, 1); // consumed PathCertificateId
    bytes.push(0); // tombstone
    bytes
}

fn push_fixed_domain(bytes: &mut Vec<u8>, substrate: u64) {
    bytes.push(1);
    push_u64(bytes, substrate);
}

fn push_point(bytes: &mut Vec<u8>, x: i64, y: i64) {
    bytes.extend_from_slice(&x.to_le_bytes());
    bytes.extend_from_slice(&y.to_le_bytes());
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u128(bytes: &mut Vec<u8>, value: u128) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

#[test]
fn every_structural_kind_has_an_independently_encoded_state_hash_golden() {
    let simulation = simulation_with_every_structural_kind();
    let bytes = independently_encoded_all_kinds_state(simulation.contract());
    let independently_hashed = blake3::hash(&bytes);

    assert_eq!(
        independently_hashed.to_hex().as_str(),
        "975c7a849af4ea9f2cf093e181bfb60757d7c53578452a81443ca80c31686d72"
    );
    assert_eq!(
        simulation.state_hash().as_bytes(),
        independently_hashed.as_bytes(),
        "simulation hash {} independent hash {}",
        simulation.state_hash(),
        independently_hashed.to_hex()
    );
}

#[test]
fn render_snapshot_counts_live_structural_primitives_without_mutating_state() {
    let simulation = simulation_with_every_structural_kind();
    let before = simulation.state_hash();
    let mut snapshot = aon_sim::RenderSnapshot::default();

    simulation.write_render_snapshot(&mut snapshot);

    assert_eq!(snapshot.primitive_count(), 4);
    assert_eq!(snapshot.next_tick(), simulation.next_tick());
    assert_eq!(snapshot.state_hash(), before);
    assert_eq!(simulation.state_hash(), before);
}

#[test]
fn canonical_hash_excludes_geometry_arena_ranges_and_removed_raw_vertices() {
    let build = |discarded_points: Vec<FixedVec2>| {
        let mut simulation = simulation();
        let first = simulation
            .step(&[envelope(
                0,
                0,
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: RoutingDomain::OpenWorld,
                    points: discarded_points,
                    endpoint_a: EndpointTarget::Free,
                    endpoint_b: EndpointTarget::Free,
                }),
            )])
            .expect("discarded Wire placement succeeds");
        assert_eq!(
            first.command_acceptances[0].created_entity,
            Some(EntityId(1))
        );
        simulation
            .step(&[envelope(1, 0, remove(1))])
            .expect("discarded Wire removal succeeds");
        let retained = simulation
            .step(&[envelope(
                2,
                0,
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: RoutingDomain::OpenWorld,
                    points: vec![
                        point(0, 2 * WORLD_PITCH),
                        point(WORLD_PITCH, 2 * WORLD_PITCH),
                    ],
                    endpoint_a: EndpointTarget::Free,
                    endpoint_b: EndpointTarget::Free,
                }),
            )])
            .expect("retained Wire placement succeeds");
        assert_eq!(
            retained.command_acceptances[0].created_entity,
            Some(EntityId(2))
        );
        simulation
    };

    let short_prefix = build(vec![point(0, 0), point(WORLD_PITCH, 0)]);
    let long_prefix = build(vec![
        point(0, 0),
        point(WORLD_PITCH, 0),
        point(2 * WORLD_PITCH, 0),
    ]);

    assert_eq!(short_prefix.next_tick(), long_prefix.next_tick());
    assert_eq!(
        short_prefix.topology_revision(),
        long_prefix.topology_revision()
    );
    assert_eq!(short_prefix.state_hash(), long_prefix.state_hash());
}

#[test]
fn canonical_hash_includes_every_redundant_live_raw_wire_vertex() {
    let build = |points: Vec<FixedVec2>| {
        let mut simulation = simulation();
        simulation
            .step(&[envelope(
                0,
                0,
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: RoutingDomain::OpenWorld,
                    points,
                    endpoint_a: EndpointTarget::Free,
                    endpoint_b: EndpointTarget::Free,
                }),
            )])
            .expect("Wire placement succeeds");
        simulation
    };
    let two_points = vec![point(0, 0), point(2 * WORLD_PITCH, 0)];
    let three_points = vec![
        point(0, 0),
        point(WORLD_PITCH, 0),
        point(2 * WORLD_PITCH, 0),
    ];
    assert_eq!(
        aon_sim::polyline_length(&two_points),
        aon_sim::polyline_length(&three_points)
    );

    let direct = build(two_points);
    let split = build(three_points);

    assert_eq!(direct.next_tick(), split.next_tick());
    assert_eq!(direct.topology_revision(), split.topology_revision());
    assert_ne!(direct.state_hash(), split.state_hash());
}
