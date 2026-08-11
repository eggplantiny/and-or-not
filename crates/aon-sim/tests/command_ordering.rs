use aon_sim::{
    ArtifactBytes, BindPortCommand, Command, CommandAcceptance, CommandEnvelope, CommandRejection,
    CommandRejectionReason, EndpointTarget, EntityId, Fixed, FixedAabb, FixedVec2, GateType,
    JunctionId, PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceJunctionCommand,
    PlaceWireCommand, RemoveEntityCommand, Revision, RoutingDomain, Simulation, SimulationError,
    SimulationPackage, StateHash, Tick, WireEnd, WireId, decode_package,
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

const fn envelope(target_tick: u64, ordinal: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        target_tick: Tick(target_tick),
        ordinal,
        command,
    }
}

fn substrate_command(origin: FixedVec2) -> Command {
    Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
        origin,
        routing_area: FixedAabb::new(
            point(-4 * WORLD_PITCH, -4 * WORLD_PITCH),
            point(4 * WORLD_PITCH, 4 * WORLD_PITCH),
        ),
        footprint: FixedAabb::new(
            point(-4 * WORLD_PITCH, -4 * WORLD_PITCH),
            point(4 * WORLD_PITCH, 4 * WORLD_PITCH),
        ),
    })
}

fn junction_command(position: FixedVec2) -> Command {
    Command::PlaceJunction(PlaceJunctionCommand {
        routing_domain: RoutingDomain::OpenWorld,
        position,
    })
}

fn wire_command(endpoint_a: EndpointTarget) -> Command {
    Command::PlaceWire(PlaceWireCommand {
        routing_domain: RoutingDomain::OpenWorld,
        points: vec![point(0, 0), point(WORLD_PITCH, 0)],
        endpoint_a,
        endpoint_b: EndpointTarget::Free,
    })
}

fn bind_a(target: EndpointTarget) -> Command {
    Command::BindPort(BindPortCommand {
        wire: WireId(EntityId(2)),
        end: WireEnd::A,
        target,
    })
}

fn remove(target: u64) -> Command {
    Command::RemoveEntity(RemoveEntityCommand {
        target: EntityId(target),
    })
}

fn simulation_with_substrate() -> Simulation {
    let mut simulation = simulation();
    let report = simulation
        .step(&[envelope(0, 0, substrate_command(point(0, 0)))])
        .expect("the fixture substrate is valid");
    assert_eq!(
        report.command_acceptances,
        vec![CommandAcceptance {
            target_tick: Tick(0),
            ordinal: 0,
            created_entity: Some(EntityId(1)),
        }]
    );
    simulation
}

fn simulation_with_wire(endpoint_a: EndpointTarget) -> Simulation {
    let mut simulation = simulation();
    let junction = simulation
        .step(&[envelope(0, 0, junction_command(point(0, 0)))])
        .expect("the fixture junction is valid");
    assert_eq!(
        junction.command_acceptances[0].created_entity,
        Some(EntityId(1))
    );

    let wire = simulation
        .step(&[envelope(1, 0, wire_command(endpoint_a))])
        .expect("the fixture wire is valid");
    assert_eq!(
        wire.command_acceptances[0].created_entity,
        Some(EntityId(2))
    );
    simulation
}

fn observed_state(
    report: &aon_sim::StepReport,
    simulation: &Simulation,
) -> (
    Vec<CommandAcceptance>,
    Vec<CommandRejection>,
    bool,
    Revision,
    StateHash,
) {
    (
        report.command_acceptances.clone(),
        report.command_rejections.clone(),
        report.topology_changed,
        simulation.topology_revision(),
        simulation.state_hash(),
    )
}

#[test]
fn c20_same_tick_order_accepts_the_lower_ordinal_gate_only() {
    let gate = Command::PlaceGate(PlaceGateCommand {
        gate_type: GateType::And,
        origin: point(0, 0),
        routing_domain: RoutingDomain::FixedSubstrate(EntityId(1)),
    });
    let mut under_test = simulation_with_substrate();
    let mut single_gate = simulation_with_substrate();

    let report = under_test
        .step(&[envelope(1, 2, gate.clone()), envelope(1, 1, gate.clone())])
        .expect("ordinary overlap is a command rejection");
    single_gate
        .step(&[envelope(1, 1, gate)])
        .expect("the lower-ordinal gate is valid");

    assert_eq!(
        report.command_acceptances,
        vec![CommandAcceptance {
            target_tick: Tick(1),
            ordinal: 1,
            created_entity: Some(EntityId(2)),
        }]
    );
    assert_eq!(
        report.command_rejections,
        vec![CommandRejection {
            target_tick: Tick(1),
            ordinal: 2,
            reason: CommandRejectionReason::GeometryOverlap,
        }]
    );
    assert!(report.topology_changed);
    assert_eq!(under_test.topology_revision(), Revision(1));
    assert_eq!(under_test.state_hash(), single_gate.state_hash());
}

#[test]
fn every_input_permutation_has_the_same_results_ids_and_state() {
    let domain = RoutingDomain::FixedSubstrate(EntityId(1));
    let commands = [
        envelope(
            1,
            30,
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: domain,
                position: point(0, 2 * WORLD_PITCH),
            }),
        ),
        envelope(
            1,
            10,
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::And,
                origin: point(-WORLD_PITCH, 0),
                routing_domain: domain,
            }),
        ),
        envelope(
            1,
            20,
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Or,
                origin: point(WORLD_PITCH, 0),
                routing_domain: domain,
            }),
        ),
    ];
    let permutations = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let mut baseline = None;

    for permutation in permutations {
        let input: Vec<_> = permutation
            .into_iter()
            .map(|index| commands[index].clone())
            .collect();
        let mut simulation = simulation_with_substrate();
        let report = simulation
            .step(&input)
            .expect("the uniquely ordinaled batch is valid");
        let observed = observed_state(&report, &simulation);

        assert_eq!(
            report.command_acceptances,
            vec![
                CommandAcceptance {
                    target_tick: Tick(1),
                    ordinal: 10,
                    created_entity: Some(EntityId(2)),
                },
                CommandAcceptance {
                    target_tick: Tick(1),
                    ordinal: 20,
                    created_entity: Some(EntityId(3)),
                },
                CommandAcceptance {
                    target_tick: Tick(1),
                    ordinal: 30,
                    created_entity: Some(EntityId(4)),
                },
            ]
        );
        assert!(report.command_rejections.is_empty());
        assert!(report.topology_changed);

        match &baseline {
            Some(expected) => assert_eq!(&observed, expected),
            None => baseline = Some(observed),
        }
    }
}

#[test]
fn duplicate_ordinal_rejects_the_entire_group_without_consuming_ids() {
    let domain = RoutingDomain::FixedSubstrate(EntityId(1));
    let commands = [
        envelope(
            1,
            5,
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::And,
                origin: point(-WORLD_PITCH, 0),
                routing_domain: domain,
            }),
        ),
        envelope(
            1,
            5,
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Or,
                origin: point(WORLD_PITCH, 0),
                routing_domain: domain,
            }),
        ),
        envelope(
            1,
            6,
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: domain,
                position: point(0, 2 * WORLD_PITCH),
            }),
        ),
    ];
    let permutations = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let mut baseline = simulation_with_substrate();
    baseline
        .step(&[commands[2].clone()])
        .expect("the non-duplicate command is valid");

    for permutation in permutations {
        let input: Vec<_> = permutation
            .into_iter()
            .map(|index| commands[index].clone())
            .collect();
        let mut simulation = simulation_with_substrate();
        let report = simulation
            .step(&input)
            .expect("duplicate ordinals are ordinary rejections");

        assert_eq!(
            report.command_acceptances,
            vec![CommandAcceptance {
                target_tick: Tick(1),
                ordinal: 6,
                created_entity: Some(EntityId(2)),
            }]
        );
        assert_eq!(
            report.command_rejections,
            vec![
                CommandRejection {
                    target_tick: Tick(1),
                    ordinal: 5,
                    reason: CommandRejectionReason::DuplicateOrdinal,
                },
                CommandRejection {
                    target_tick: Tick(1),
                    ordinal: 5,
                    reason: CommandRejectionReason::DuplicateOrdinal,
                },
            ]
        );
        assert_eq!(simulation.state_hash(), baseline.state_hash());
    }
}

#[test]
fn wrong_tick_does_not_join_the_current_tick_duplicate_group_or_consume_an_id() {
    let current = envelope(0, 7, junction_command(point(0, 0)));
    let wrong = envelope(9, 7, junction_command(point(WORLD_PITCH, 0)));
    let mut under_test = simulation();
    let mut current_only = simulation();

    let report = under_test
        .step(&[wrong, current.clone()])
        .expect("wrong-tick input is an ordinary rejection");
    current_only
        .step(&[current])
        .expect("the current-tick command is valid");

    assert_eq!(
        report.command_acceptances,
        vec![CommandAcceptance {
            target_tick: Tick(0),
            ordinal: 7,
            created_entity: Some(EntityId(1)),
        }]
    );
    assert_eq!(
        report.command_rejections,
        vec![CommandRejection {
            target_tick: Tick(9),
            ordinal: 7,
            reason: CommandRejectionReason::WrongTick,
        }]
    );
    assert_eq!(under_test.state_hash(), current_only.state_hash());
}

#[test]
fn rejected_placement_does_not_consume_the_next_entity_id() {
    let mut simulation = simulation();
    let invalid_gate = Command::PlaceGate(PlaceGateCommand {
        gate_type: GateType::Not,
        origin: point(0, 0),
        routing_domain: RoutingDomain::OpenWorld,
    });

    let report = simulation
        .step(&[
            envelope(0, 1, invalid_gate),
            envelope(0, 2, junction_command(point(0, 0))),
        ])
        .expect("unsupported placement is an ordinary rejection");

    assert_eq!(
        report.command_rejections,
        vec![CommandRejection {
            target_tick: Tick(0),
            ordinal: 1,
            reason: CommandRejectionReason::UnsupportedPlacement,
        }]
    );
    assert_eq!(
        report.command_acceptances,
        vec![CommandAcceptance {
            target_tick: Tick(0),
            ordinal: 2,
            created_entity: Some(EntityId(1)),
        }]
    );
}

#[test]
fn a_predicted_new_id_is_rejected_until_the_next_tick() {
    let junction = JunctionId(EntityId(1));
    let mut simulation = simulation();

    let first = simulation
        .step(&[
            envelope(0, 1, junction_command(point(0, 0))),
            envelope(0, 2, wire_command(EndpointTarget::Junction(junction))),
        ])
        .expect("a predicted ID is an ordinary rejection");
    assert_eq!(
        first.command_acceptances,
        vec![CommandAcceptance {
            target_tick: Tick(0),
            ordinal: 1,
            created_entity: Some(EntityId(1)),
        }]
    );
    assert_eq!(
        first.command_rejections,
        vec![CommandRejection {
            target_tick: Tick(0),
            ordinal: 2,
            reason: CommandRejectionReason::UnknownEntity,
        }]
    );

    let second = simulation
        .step(&[envelope(
            1,
            0,
            wire_command(EndpointTarget::Junction(junction)),
        )])
        .expect("the returned ID is referenceable on the next tick");
    assert_eq!(
        second.command_acceptances,
        vec![CommandAcceptance {
            target_tick: Tick(1),
            ordinal: 0,
            created_entity: Some(EntityId(2)),
        }]
    );
    assert!(second.command_rejections.is_empty());
}

#[test]
fn rebinding_the_same_target_is_an_accepted_state_preserving_noop() {
    let junction = EndpointTarget::Junction(JunctionId(EntityId(1)));
    let mut rebound = simulation_with_wire(junction);
    let mut untouched = simulation_with_wire(junction);
    let revision_before = rebound.topology_revision();

    let report = rebound
        .step(&[envelope(2, 4, bind_a(junction))])
        .expect("same-target rebind is accepted");
    untouched.step(&[]).expect("empty comparison step succeeds");

    assert_eq!(
        report.command_acceptances,
        vec![CommandAcceptance {
            target_tick: Tick(2),
            ordinal: 4,
            created_entity: None,
        }]
    );
    assert!(report.command_rejections.is_empty());
    assert!(!report.topology_changed);
    assert_eq!(rebound.topology_revision(), revision_before);
    assert_eq!(rebound.state_hash(), untouched.state_hash());
}

#[test]
fn wire_and_junction_generations_advance_and_coalesce_once_per_phase() {
    let junction = EndpointTarget::Junction(JunctionId(EntityId(1)));

    // Removing the Junction makes all three final worlds identical except for the live Wire's
    // generation. One effective bind and three effective changes must therefore hash alike, while
    // removing the unconnected Junction leaves generation zero and must hash differently.
    let mut wire_once = simulation_with_wire(EndpointTarget::Free);
    let mut wire_many = simulation_with_wire(EndpointTarget::Free);
    let mut wire_control = simulation_with_wire(EndpointTarget::Free);
    let once_report = wire_once
        .step(&[envelope(2, 1, bind_a(junction)), envelope(2, 10, remove(1))])
        .expect("bind then Junction removal succeeds");
    let many_report = wire_many
        .step(&[
            envelope(2, 1, bind_a(junction)),
            envelope(2, 2, bind_a(EndpointTarget::Free)),
            envelope(2, 3, bind_a(junction)),
            envelope(2, 10, remove(1)),
        ])
        .expect("repeated binds then Junction removal succeeds");
    wire_control
        .step(&[envelope(2, 10, remove(1))])
        .expect("unconnected Junction removal succeeds");

    assert!(once_report.topology_changed);
    assert!(many_report.topology_changed);
    assert_eq!(wire_once.topology_revision(), Revision(3));
    assert_eq!(wire_once.state_hash(), wire_many.state_hash());
    assert_ne!(wire_once.state_hash(), wire_control.state_hash());

    // Removing the Wire instead leaves the Junction generation as the only semantic difference.
    let mut junction_once = simulation_with_wire(EndpointTarget::Free);
    let mut junction_many = simulation_with_wire(EndpointTarget::Free);
    let mut junction_control = simulation_with_wire(EndpointTarget::Free);
    junction_once
        .step(&[envelope(2, 1, bind_a(junction)), envelope(2, 10, remove(2))])
        .expect("bind then Wire removal succeeds");
    junction_many
        .step(&[
            envelope(2, 1, bind_a(junction)),
            envelope(2, 2, bind_a(EndpointTarget::Free)),
            envelope(2, 3, bind_a(junction)),
            envelope(2, 10, remove(2)),
        ])
        .expect("repeated binds then Wire removal succeeds");
    junction_control
        .step(&[envelope(2, 10, remove(2))])
        .expect("unconnected Wire removal succeeds");

    assert_eq!(junction_once.topology_revision(), Revision(3));
    assert_eq!(junction_once.state_hash(), junction_many.state_hash());
    assert_ne!(junction_once.state_hash(), junction_control.state_hash());
}

#[test]
fn fatal_geometry_overflow_rolls_back_the_entire_phase() {
    let mut simulation = simulation();
    let before_hash = simulation.state_hash();
    let largest_aligned_origin = i64::MAX - i64::MAX.rem_euclid(WORLD_PITCH);
    let overflowing = Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
        origin: point(largest_aligned_origin, 0),
        routing_area: FixedAabb::new(
            point(-WORLD_PITCH, -WORLD_PITCH),
            point(WORLD_PITCH, WORLD_PITCH),
        ),
        footprint: FixedAabb::new(
            point(-WORLD_PITCH, -WORLD_PITCH),
            point(WORLD_PITCH, WORLD_PITCH),
        ),
    });

    assert_eq!(
        simulation.step(&[
            envelope(0, 1, substrate_command(point(0, 0))),
            envelope(0, 2, overflowing),
        ]),
        Err(SimulationError::NumericOverflow)
    );
    assert_eq!(simulation.next_tick(), Tick(0));
    assert_eq!(simulation.topology_revision(), Revision(0));
    assert_eq!(simulation.state_hash(), before_hash);

    let retry = simulation
        .step(&[envelope(0, 0, junction_command(point(0, 0)))])
        .expect("the rolled-back EntityId remains available");
    assert_eq!(
        retry.command_acceptances,
        vec![CommandAcceptance {
            target_tick: Tick(0),
            ordinal: 0,
            created_entity: Some(EntityId(1)),
        }]
    );
}

#[test]
fn fatal_negative_translation_underflow_rolls_back_the_entire_phase() {
    let mut simulation = simulation();
    let before_hash = simulation.state_hash();
    let underflowing = Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
        origin: point(i64::MIN, 0),
        routing_area: FixedAabb::new(
            point(-WORLD_PITCH, -WORLD_PITCH),
            point(WORLD_PITCH, WORLD_PITCH),
        ),
        footprint: FixedAabb::new(
            point(-WORLD_PITCH, -WORLD_PITCH),
            point(WORLD_PITCH, WORLD_PITCH),
        ),
    });

    assert_eq!(
        simulation.step(&[
            envelope(0, 1, substrate_command(point(0, 0))),
            envelope(0, 2, underflowing),
        ]),
        Err(SimulationError::NumericOverflow)
    );
    assert_eq!(simulation.next_tick(), Tick(0));
    assert_eq!(simulation.topology_revision(), Revision(0));
    assert_eq!(simulation.state_hash(), before_hash);

    let retry = simulation
        .step(&[envelope(0, 0, junction_command(point(0, 0)))])
        .expect("the rolled-back EntityId remains available");
    assert_eq!(
        retry.command_acceptances,
        vec![CommandAcceptance {
            target_tick: Tick(0),
            ordinal: 0,
            created_entity: Some(EntityId(1)),
        }]
    );
}

#[test]
fn fatal_single_segment_wire_length_overflow_rolls_back_the_entire_phase() {
    let mut simulation = simulation();
    let before_hash = simulation.state_hash();
    let overflowing = Command::PlaceWire(PlaceWireCommand {
        routing_domain: RoutingDomain::OpenWorld,
        points: vec![point(i64::MIN, 0), point(0, 0)],
        endpoint_a: EndpointTarget::Free,
        endpoint_b: EndpointTarget::Free,
    });

    assert_eq!(
        simulation.step(&[
            envelope(0, 1, substrate_command(point(0, 0))),
            envelope(0, 2, overflowing),
        ]),
        Err(SimulationError::NumericOverflow)
    );
    assert_eq!(simulation.next_tick(), Tick(0));
    assert_eq!(simulation.topology_revision(), Revision(0));
    assert_eq!(simulation.state_hash(), before_hash);

    let retry = simulation
        .step(&[envelope(0, 0, junction_command(point(0, 0)))])
        .expect("the rolled-back EntityId remains available");
    assert_eq!(
        retry.command_acceptances,
        vec![CommandAcceptance {
            target_tick: Tick(0),
            ordinal: 0,
            created_entity: Some(EntityId(1)),
        }]
    );
}

#[test]
fn fatal_multi_segment_wire_length_overflow_rolls_back_the_entire_phase() {
    let mut simulation = simulation();
    let before_hash = simulation.state_hash();
    let half_range = 1_i64 << 62;
    let overflowing = Command::PlaceWire(PlaceWireCommand {
        routing_domain: RoutingDomain::OpenWorld,
        points: vec![point(-half_range, 0), point(0, 0), point(0, half_range)],
        endpoint_a: EndpointTarget::Free,
        endpoint_b: EndpointTarget::Free,
    });

    assert_eq!(
        simulation.step(&[
            envelope(0, 1, substrate_command(point(0, 0))),
            envelope(0, 2, overflowing),
        ]),
        Err(SimulationError::NumericOverflow)
    );
    assert_eq!(simulation.next_tick(), Tick(0));
    assert_eq!(simulation.topology_revision(), Revision(0));
    assert_eq!(simulation.state_hash(), before_hash);

    let retry = simulation
        .step(&[envelope(0, 0, junction_command(point(0, 0)))])
        .expect("the rolled-back EntityId remains available");
    assert_eq!(
        retry.command_acceptances,
        vec![CommandAcceptance {
            target_tick: Tick(0),
            ordinal: 0,
            created_entity: Some(EntityId(1)),
        }]
    );
}
