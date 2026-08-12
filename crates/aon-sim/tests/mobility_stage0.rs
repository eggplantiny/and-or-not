use aon_sim::{
    ArtifactBytes, BindPortCommand, Command, CommandEnvelope, CommandRejectionReason,
    DriveStrength, EndpointTarget, EntityId, Fixed, FixedAabb, FixedVec2, GateId, GatePort,
    GatePortRef, GateType, Heading, JunctionDecisionKind, JunctionId, LogicLevel, MobileId,
    MobilePort, MobilePortRef, PlaceGateCommand, PlaceJunctionCommand, PlaceMobileSubstrateCommand,
    PlaceWireCommand, RemoveEntityCommand, RenderSnapshot, RoutingDomain, SetExternalDriverCommand,
    Simulation, SimulationPackage, Tick, TrackPosition, WireEnd, WireId, decode_package,
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
const CIRCUIT_PITCH: i64 = 16_384;

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

fn package() -> SimulationPackage {
    decode_package(ArtifactBytes {
        scenario: SCENARIO,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("reference package")
}

fn envelope(tick: u64, ordinal: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        target_tick: Tick(tick),
        ordinal,
        command,
    }
}

fn mobile_command(origin: FixedVec2) -> Command {
    let bounds = FixedAabb::new(
        point(-4 * CIRCUIT_PITCH, -4 * CIRCUIT_PITCH),
        point(4 * CIRCUIT_PITCH, 4 * CIRCUIT_PITCH),
    );
    Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
        origin,
        routing_area: bounds,
        footprint: bounds,
    })
}

fn simulation_with_track() -> Simulation {
    let mut simulation = Simulation::new(package()).expect("simulation");
    let report = simulation
        .step(&[envelope(
            0,
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![point(0, 0), point(4 * WORLD_PITCH, 0)],
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Free,
            }),
        )])
        .expect("track placement");
    assert_eq!(
        report.command_acceptances[0].created_entity,
        Some(EntityId(1))
    );
    simulation
}

#[test]
fn mobile_placement_allocates_track_position_and_three_low_control_sinks() {
    let mut simulation = simulation_with_track();
    let before = simulation.state_hash();
    let report = simulation
        .step(&[envelope(1, 0, mobile_command(point(WORLD_PITCH, 0)))])
        .expect("mobile placement");
    assert_eq!(
        report.command_acceptances[0].created_entity,
        Some(EntityId(2))
    );
    assert_ne!(report.state_hash, before);
    assert_eq!(report.mobile_movements.len(), 1);
    assert_eq!(
        report.mobile_movements[0].granted_budget,
        Fixed(WORLD_PITCH)
    );
    assert_eq!(
        report.mobile_movements[0].consumed_budget,
        Fixed(WORLD_PITCH)
    );

    let mut snapshot = RenderSnapshot::default();
    simulation.write_render_snapshot(&mut snapshot);
    assert_eq!(snapshot.mobiles().len(), 1);
    let mobile = snapshot.mobiles()[0];
    assert_eq!(mobile.id.entity_id(), EntityId(2));
    assert_eq!(
        mobile.track_position,
        TrackPosition::Edge {
            edge: WireId(EntityId(1)),
            offset: Fixed(2 * WORLD_PITCH),
            heading: Heading::Forward,
        }
    );
    assert_eq!(mobile.world_position, point(2 * WORLD_PITCH, 0));
    assert_eq!(
        (mobile.stop, mobile.left, mobile.right),
        (LogicLevel::Low, LogicLevel::Low, LogicLevel::Low,)
    );
    assert_eq!(
        (
            mobile.ports.stop.entity_id(),
            mobile.ports.left.entity_id(),
            mobile.ports.right.entity_id(),
        ),
        (EntityId(1), EntityId(2), EntityId(3))
    );
}

#[test]
fn off_track_placement_rejects_without_allocating_mobile_or_control_sinks() {
    let mut simulation = simulation_with_track();
    let before = simulation.state_hash();
    let report = simulation
        .step(&[envelope(
            1,
            0,
            mobile_command(point(WORLD_PITCH, WORLD_PITCH)),
        )])
        .expect("typed rejection");
    assert!(report.command_acceptances.is_empty());
    assert_eq!(
        report.command_rejections[0].reason,
        CommandRejectionReason::UnsupportedPlacement
    );

    let mut snapshot = RenderSnapshot::default();
    simulation.write_render_snapshot(&mut snapshot);
    assert!(snapshot.mobiles().is_empty());
    assert_ne!(
        simulation.state_hash(),
        before,
        "Tick advancement remains canonical"
    );

    let report = simulation
        .step(&[envelope(2, 0, mobile_command(point(2 * WORLD_PITCH, 0)))])
        .expect("later valid placement");
    assert_eq!(
        report.command_acceptances[0].created_entity,
        Some(EntityId(2))
    );
    simulation.write_render_snapshot(&mut snapshot);
    assert_eq!(snapshot.mobiles()[0].ports.stop.entity_id(), EntityId(1));
}

#[test]
fn mobile_local_circuit_routes_gate_outputs_to_all_intrinsic_control_sinks() {
    let mut simulation = simulation_with_track();
    simulation
        .step(&[envelope(1, 0, mobile_command(point(WORLD_PITCH, 0)))])
        .expect("mobile placement");

    let domain = RoutingDomain::MobileSubstrate(EntityId(2));
    let gate_ys = [-2 * CIRCUIT_PITCH, 0, 2 * CIRCUIT_PITCH];
    let gate_report = simulation
        .step(
            &gate_ys
                .into_iter()
                .enumerate()
                .map(|(ordinal, y)| {
                    envelope(
                        2,
                        ordinal as u64,
                        Command::PlaceGate(PlaceGateCommand {
                            gate_type: GateType::Not,
                            origin: point(0, y),
                            routing_domain: domain,
                        }),
                    )
                })
                .collect::<Vec<_>>(),
        )
        .expect("mobile-local gates place");
    assert_eq!(gate_report.command_rejections, []);
    assert_eq!(
        gate_report
            .command_acceptances
            .iter()
            .map(|acceptance| acceptance.created_entity)
            .collect::<Vec<_>>(),
        vec![Some(EntityId(3)), Some(EntityId(4)), Some(EntityId(5))]
    );

    let ports = [MobilePort::Stop, MobilePort::Left, MobilePort::Right];
    let wire_report = simulation
        .step(
            &gate_ys
                .into_iter()
                .zip(ports)
                .enumerate()
                .map(|(ordinal, (y, port))| {
                    let gate = GateId(EntityId(3 + ordinal as u64));
                    envelope(
                        3,
                        ordinal as u64,
                        Command::PlaceWire(PlaceWireCommand {
                            routing_domain: domain,
                            points: vec![point(CIRCUIT_PITCH, y), point(3 * CIRCUIT_PITCH, y)],
                            endpoint_a: EndpointTarget::GatePort(GatePortRef {
                                gate,
                                port: GatePort::Output,
                            }),
                            endpoint_b: EndpointTarget::MobilePort(MobilePortRef {
                                mobile: MobileId(EntityId(2)),
                                port,
                            }),
                        }),
                    )
                })
                .collect::<Vec<_>>(),
        )
        .expect("mobile control routes place");
    assert_eq!(wire_report.command_rejections, []);

    while simulation.next_tick().0 < 12 {
        simulation.step(&[]).expect("signal settles");
    }
    let mut snapshot = RenderSnapshot::default();
    simulation.write_render_snapshot(&mut snapshot);
    let mobile = snapshot.mobiles()[0];
    assert_eq!(
        (mobile.stop, mobile.left, mobile.right),
        (LogicLevel::High, LogicLevel::High, LogicLevel::High)
    );
}

fn assert_unknown_control_stops_at_junction(port: MobilePort) {
    let mut simulation = Simulation::new(package()).expect("simulation");
    let junction = JunctionId(EntityId(1));
    simulation
        .step(&[envelope(
            0,
            0,
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: point(2 * WORLD_PITCH, 0),
            }),
        )])
        .expect("junction placement");
    let track = simulation
        .step(&[
            envelope(
                1,
                0,
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: RoutingDomain::OpenWorld,
                    points: vec![point(0, 0), point(2 * WORLD_PITCH, 0)],
                    endpoint_a: EndpointTarget::Free,
                    endpoint_b: EndpointTarget::Junction(junction),
                }),
            ),
            envelope(
                1,
                1,
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: RoutingDomain::OpenWorld,
                    points: vec![point(2 * WORLD_PITCH, 0), point(4 * WORLD_PITCH, 0)],
                    endpoint_a: EndpointTarget::Junction(junction),
                    endpoint_b: EndpointTarget::Free,
                }),
            ),
        ])
        .expect("two-edge track placement");
    assert!(track.command_rejections.is_empty());

    let mobile = MobileId(EntityId(4));
    let placement = simulation
        .step(&[envelope(2, 0, mobile_command(point(WORLD_PITCH, 0)))])
        .expect("mobile reaches junction");
    assert_eq!(
        placement.mobile_movements[0].end,
        TrackPosition::Junction {
            junction,
            incoming_edge: WireId(EntityId(2)),
        }
    );

    let domain = RoutingDomain::MobileSubstrate(mobile.entity_id());
    let gate = GateId(EntityId(5));
    let gate_report = simulation
        .step(&[envelope(
            3,
            0,
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Not,
                origin: point(0, 0),
                routing_domain: domain,
            }),
        )])
        .expect("mobile-local source gate placement");
    assert!(gate_report.command_rejections.is_empty());
    let driver = simulation
        .gate_signal_ports(gate)
        .expect("source gate ports")
        .input_a
        .external_driver;

    let route_report = simulation
        .step(&[envelope(
            4,
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: domain,
                points: vec![point(-CIRCUIT_PITCH, 0), point(-2 * CIRCUIT_PITCH, 0)],
                endpoint_a: EndpointTarget::GatePort(GatePortRef {
                    gate,
                    port: GatePort::InputA,
                }),
                endpoint_b: EndpointTarget::MobilePort(MobilePortRef { mobile, port }),
            }),
        )])
        .expect("valid source-to-control route placement");
    assert!(route_report.command_rejections.is_empty());

    let topology_sync = simulation.step(&[]).expect("LOW topology sync arrives");
    assert!(
        topology_sync
            .signal_arrivals
            .iter()
            .any(|arrival| arrival.sample.driver_id == driver)
    );
    let pre_unknown = simulation
        .step(&[envelope(
            6,
            0,
            Command::SetExternalDriver(SetExternalDriverCommand {
                driver,
                level: LogicLevel::X,
                strength: DriveStrength(400),
            }),
        )])
        .expect("external X driver update is accepted");
    assert!(pre_unknown.command_rejections.is_empty());
    let at_junction = TrackPosition::Junction {
        junction,
        incoming_edge: WireId(EntityId(3)),
    };
    assert_eq!(pre_unknown.mobile_movements[0].end, at_junction);
    assert_eq!(
        pre_unknown.mobile_movements[0].consumed_budget,
        Fixed(WORLD_PITCH)
    );

    let stopped = simulation
        .step(&[])
        .expect("routed X reaches the control before movement sampling");
    let mut snapshot = RenderSnapshot::default();
    simulation.write_render_snapshot(&mut snapshot);
    let mobile_record = snapshot.mobiles()[0];
    let (sink, expected_controls) = match port {
        MobilePort::Stop => (
            mobile_record.ports.stop,
            (LogicLevel::X, LogicLevel::Low, LogicLevel::Low),
        ),
        MobilePort::Left => (
            mobile_record.ports.left,
            (LogicLevel::Low, LogicLevel::X, LogicLevel::Low),
        ),
        MobilePort::Right => (
            mobile_record.ports.right,
            (LogicLevel::Low, LogicLevel::Low, LogicLevel::X),
        ),
    };
    assert!(stopped.signal_arrivals.iter().any(|arrival| {
        arrival.source_driver == driver
            && arrival.sink == sink
            && arrival.sample.level == LogicLevel::X
    }));
    assert_eq!(simulation.sink_level(sink), Some(LogicLevel::X));
    assert_eq!(
        (mobile_record.stop, mobile_record.left, mobile_record.right),
        expected_controls
    );
    assert_eq!(
        (
            stopped.mobile_movements[0].controls.stop,
            stopped.mobile_movements[0].controls.left,
            stopped.mobile_movements[0].controls.right,
        ),
        expected_controls
    );
    assert_eq!(mobile_record.track_position, at_junction);
    assert_eq!(stopped.mobile_movements[0].start, at_junction);
    assert_eq!(stopped.mobile_movements[0].end, at_junction);
    assert_eq!(stopped.mobile_movements[0].granted_budget, Fixed::ZERO);
    assert_eq!(stopped.mobile_movements[0].consumed_budget, Fixed::ZERO);
    assert!(stopped.mobile_movements[0].junction_decisions.is_empty());
}

#[test]
fn routed_unknown_stop_left_or_right_blocks_the_entire_junction_tick() {
    for port in [MobilePort::Stop, MobilePort::Left, MobilePort::Right] {
        assert_unknown_control_stops_at_junction(port);
    }
}

#[test]
fn mobile_local_bounds_rejection_does_not_consume_structural_identity() {
    let mut simulation = simulation_with_track();
    simulation
        .step(&[envelope(1, 0, mobile_command(point(WORLD_PITCH, 0)))])
        .expect("mobile placement");
    let domain = RoutingDomain::MobileSubstrate(EntityId(2));

    let rejected = simulation
        .step(&[envelope(
            2,
            0,
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Not,
                origin: point(4 * CIRCUIT_PITCH, 0),
                routing_domain: domain,
            }),
        )])
        .expect("typed local bounds rejection");
    assert_eq!(
        rejected.command_rejections[0].reason,
        CommandRejectionReason::SubstrateBoundsViolation
    );

    let accepted = simulation
        .step(&[envelope(
            3,
            0,
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Not,
                origin: point(0, 0),
                routing_domain: domain,
            }),
        )])
        .expect("later local gate placement");
    assert_eq!(
        accepted.command_acceptances[0].created_entity,
        Some(EntityId(3))
    );
}

#[test]
fn simulation_phases_arrive_at_junction_then_commit_straight_on_the_next_tick() {
    let mut simulation = Simulation::new(package()).expect("simulation");
    let junction = JunctionId(EntityId(1));
    simulation
        .step(&[envelope(
            0,
            0,
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: point(2 * WORLD_PITCH, 0),
            }),
        )])
        .expect("junction placement");
    let tracks = simulation
        .step(&[
            envelope(
                1,
                0,
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: RoutingDomain::OpenWorld,
                    points: vec![point(0, 0), point(2 * WORLD_PITCH, 0)],
                    endpoint_a: EndpointTarget::Free,
                    endpoint_b: EndpointTarget::Junction(junction),
                }),
            ),
            envelope(
                1,
                1,
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: RoutingDomain::OpenWorld,
                    points: vec![point(2 * WORLD_PITCH, 0), point(4 * WORLD_PITCH, 0)],
                    endpoint_a: EndpointTarget::Junction(junction),
                    endpoint_b: EndpointTarget::Free,
                }),
            ),
        ])
        .expect("track placement");
    assert_eq!(tracks.command_rejections, []);

    let placement = simulation
        .step(&[envelope(2, 0, mobile_command(point(WORLD_PITCH, 0)))])
        .expect("mobile placement and first movement");
    assert_eq!(
        placement.mobile_movements[0].end,
        TrackPosition::Junction {
            junction,
            incoming_edge: WireId(EntityId(2)),
        }
    );
    assert!(placement.mobile_movements[0].junction_decisions.is_empty());

    let turn = simulation.step(&[]).expect("junction turn and movement");
    assert_eq!(turn.mobile_movements.len(), 1);
    assert_eq!(turn.mobile_movements[0].junction_decisions.len(), 1);
    assert_eq!(
        turn.mobile_movements[0].junction_decisions[0].kind,
        JunctionDecisionKind::Straight
    );
    assert_eq!(
        turn.mobile_movements[0].junction_decisions[0].selected_edge,
        Some(WireId(EntityId(3)))
    );
    assert_eq!(
        turn.mobile_movements[0].end,
        TrackPosition::Edge {
            edge: WireId(EntityId(3)),
            offset: Fixed(WORLD_PITCH),
            heading: Heading::Forward,
        }
    );
}

#[test]
fn occupied_track_wire_is_rejected_for_removal_or_rebinding() {
    let mut simulation = simulation_with_track();
    simulation
        .step(&[envelope(1, 0, mobile_command(point(WORLD_PITCH, 0)))])
        .expect("mobile placement");
    let removal = simulation
        .step(&[envelope(
            2,
            0,
            Command::RemoveEntity(RemoveEntityCommand {
                target: EntityId(1),
            }),
        )])
        .expect("occupied removal rejection");
    assert_eq!(
        removal.command_rejections[0].reason,
        CommandRejectionReason::TrackOccupied
    );
}

#[test]
fn occupied_track_junction_is_rejected_for_removal() {
    let mut simulation = Simulation::new(package()).expect("simulation");
    let junction = JunctionId(EntityId(1));
    simulation
        .step(&[envelope(
            0,
            0,
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: point(2 * WORLD_PITCH, 0),
            }),
        )])
        .expect("junction placement");
    simulation
        .step(&[envelope(
            1,
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![point(0, 0), point(2 * WORLD_PITCH, 0)],
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Junction(junction),
            }),
        )])
        .expect("track placement");
    let placement = simulation
        .step(&[envelope(2, 0, mobile_command(point(WORLD_PITCH, 0)))])
        .expect("mobile reaches junction");
    assert_eq!(
        placement.mobile_movements[0].end,
        TrackPosition::Junction {
            junction,
            incoming_edge: WireId(EntityId(2)),
        }
    );

    let removal = simulation
        .step(&[envelope(
            3,
            0,
            Command::RemoveEntity(RemoveEntityCommand {
                target: junction.entity_id(),
            }),
        )])
        .expect("occupied junction rejection");
    assert_eq!(
        removal.command_rejections[0].reason,
        CommandRejectionReason::TrackOccupied
    );
}

#[test]
fn multiple_mobiles_overlap_without_capacity_and_ignore_batch_insertion_order() {
    let mut forward = simulation_with_track();
    let mut reversed = simulation_with_track();
    let commands = vec![
        envelope(1, 0, mobile_command(point(WORLD_PITCH, 0))),
        envelope(1, 1, mobile_command(point(WORLD_PITCH, 0))),
    ];
    let forward_report = forward.step(&commands).expect("forward placement batch");
    let reversed_report = reversed
        .step(&commands.iter().cloned().rev().collect::<Vec<_>>())
        .expect("reversed placement batch");
    assert_eq!(forward_report, reversed_report);
    assert_eq!(forward.state_hash(), reversed.state_hash());

    let mut forward_snapshot = RenderSnapshot::default();
    let mut reversed_snapshot = RenderSnapshot::default();
    forward.write_render_snapshot(&mut forward_snapshot);
    reversed.write_render_snapshot(&mut reversed_snapshot);
    assert_eq!(forward_snapshot, reversed_snapshot);
    assert_eq!(forward_snapshot.mobiles().len(), 2);
    assert_eq!(
        forward_snapshot.mobiles()[0].track_position,
        forward_snapshot.mobiles()[1].track_position,
        "Stage 0 Track has no capacity reservation or collision displacement"
    );
}

#[test]
fn unrelated_topology_edit_does_not_change_mobile_movement() {
    let mut edited = simulation_with_track();
    let mut baseline = simulation_with_track();
    let placement = [envelope(1, 0, mobile_command(point(WORLD_PITCH, 0)))];
    edited.step(&placement).expect("edited mobile placement");
    baseline
        .step(&placement)
        .expect("baseline mobile placement");

    let edited_report = edited
        .step(&[envelope(
            2,
            0,
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: point(0, 2 * WORLD_PITCH),
            }),
        )])
        .expect("unrelated edit");
    let baseline_report = baseline.step(&[]).expect("baseline movement");
    assert_eq!(
        edited_report.mobile_movements,
        baseline_report.mobile_movements
    );

    let mut edited_snapshot = RenderSnapshot::default();
    let mut baseline_snapshot = RenderSnapshot::default();
    edited.write_render_snapshot(&mut edited_snapshot);
    baseline.write_render_snapshot(&mut baseline_snapshot);
    assert_eq!(edited_snapshot.mobiles(), baseline_snapshot.mobiles());
}

#[test]
fn local_mobile_geometry_is_hash_sensitive_without_changing_track_projection() {
    let mut wide = simulation_with_track();
    let mut narrow = simulation_with_track();
    wide.step(&[envelope(1, 0, mobile_command(point(WORLD_PITCH, 0)))])
        .expect("wide mobile");
    let narrow_bounds = FixedAabb::new(
        point(-3 * CIRCUIT_PITCH, -3 * CIRCUIT_PITCH),
        point(3 * CIRCUIT_PITCH, 3 * CIRCUIT_PITCH),
    );
    narrow
        .step(&[envelope(
            1,
            0,
            Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
                origin: point(WORLD_PITCH, 0),
                routing_area: narrow_bounds,
                footprint: narrow_bounds,
            }),
        )])
        .expect("narrow mobile");

    let mut wide_snapshot = RenderSnapshot::default();
    let mut narrow_snapshot = RenderSnapshot::default();
    wide.write_render_snapshot(&mut wide_snapshot);
    narrow.write_render_snapshot(&mut narrow_snapshot);
    assert_eq!(
        wide_snapshot.mobiles()[0].track_position,
        narrow_snapshot.mobiles()[0].track_position
    );
    assert_ne!(wide.state_hash(), narrow.state_hash());
}

#[test]
fn track_heading_is_v5_hash_sensitive_at_the_same_edge_offset() {
    let mut forward = simulation_with_track();
    let mut reverse = simulation_with_track();
    forward
        .step(&[envelope(1, 0, mobile_command(point(2 * WORLD_PITCH, 0)))])
        .expect("forward placement");
    reverse
        .step(&[envelope(1, 0, mobile_command(point(4 * WORLD_PITCH, 0)))])
        .expect("reverse placement");

    let mut forward_snapshot = RenderSnapshot::default();
    let mut reverse_snapshot = RenderSnapshot::default();
    forward.write_render_snapshot(&mut forward_snapshot);
    reverse.write_render_snapshot(&mut reverse_snapshot);
    assert_eq!(
        forward_snapshot.mobiles()[0].track_position,
        TrackPosition::Edge {
            edge: WireId(EntityId(1)),
            offset: Fixed(3 * WORLD_PITCH),
            heading: Heading::Forward,
        }
    );
    assert_eq!(
        reverse_snapshot.mobiles()[0].track_position,
        TrackPosition::Edge {
            edge: WireId(EntityId(1)),
            offset: Fixed(3 * WORLD_PITCH),
            heading: Heading::Reverse,
        }
    );
    assert_ne!(forward.state_hash(), reverse.state_hash());
}

#[test]
fn identical_track_rebuild_gets_a_new_identity_without_retargeting_a_mobile() {
    let mut simulation = Simulation::new(package()).expect("simulation");
    let junction = JunctionId(EntityId(1));
    simulation
        .step(&[envelope(
            0,
            0,
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: point(2 * WORLD_PITCH, 0),
            }),
        )])
        .expect("junction placement");
    simulation
        .step(&[
            envelope(
                1,
                0,
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: RoutingDomain::OpenWorld,
                    points: vec![point(0, 0), point(2 * WORLD_PITCH, 0)],
                    endpoint_a: EndpointTarget::Free,
                    endpoint_b: EndpointTarget::Junction(junction),
                }),
            ),
            envelope(
                1,
                1,
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: RoutingDomain::OpenWorld,
                    points: vec![point(2 * WORLD_PITCH, 0), point(4 * WORLD_PITCH, 0)],
                    endpoint_a: EndpointTarget::Junction(junction),
                    endpoint_b: EndpointTarget::Free,
                }),
            ),
        ])
        .expect("track placement");
    simulation
        .step(&[envelope(2, 0, mobile_command(point(WORLD_PITCH, 0)))])
        .expect("mobile reaches junction");
    simulation.step(&[]).expect("mobile leaves old edge");

    let rebuild = simulation
        .step(&[
            envelope(
                4,
                0,
                Command::RemoveEntity(RemoveEntityCommand {
                    target: EntityId(2),
                }),
            ),
            envelope(
                4,
                1,
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: RoutingDomain::OpenWorld,
                    points: vec![point(0, 0), point(2 * WORLD_PITCH, 0)],
                    endpoint_a: EndpointTarget::Free,
                    endpoint_b: EndpointTarget::Junction(junction),
                }),
            ),
        ])
        .expect("remove and rebuild");
    assert_eq!(rebuild.command_rejections, []);
    assert_eq!(
        rebuild.command_acceptances[1].created_entity,
        Some(EntityId(5))
    );
    assert_eq!(
        rebuild.mobile_movements[0].start,
        TrackPosition::Edge {
            edge: WireId(EntityId(3)),
            offset: Fixed(WORLD_PITCH),
            heading: Heading::Forward,
        }
    );
    assert_ne!(
        rebuild.mobile_movements[0].end,
        TrackPosition::Edge {
            edge: WireId(EntityId(2)),
            offset: Fixed::ZERO,
            heading: Heading::Forward,
        }
    );
}

#[test]
fn an_open_world_wire_cannot_bind_both_ends_to_the_same_track_junction() {
    let mut simulation = Simulation::new(package()).expect("simulation");
    let junction = JunctionId(EntityId(1));
    simulation
        .step(&[envelope(
            0,
            0,
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: point(0, 0),
            }),
        )])
        .expect("junction placement");
    let loop_points = vec![
        point(0, 0),
        point(WORLD_PITCH, 0),
        point(WORLD_PITCH, WORLD_PITCH),
        point(0, WORLD_PITCH),
        point(0, 0),
    ];
    let rejected = simulation
        .step(&[envelope(
            1,
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: loop_points.clone(),
                endpoint_a: EndpointTarget::Junction(junction),
                endpoint_b: EndpointTarget::Junction(junction),
            }),
        )])
        .expect("same-junction loop is a typed rejection");
    assert_eq!(
        rejected.command_rejections[0].reason,
        CommandRejectionReason::InvalidPortBinding
    );

    let placed = simulation
        .step(&[envelope(
            2,
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: loop_points,
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Free,
            }),
        )])
        .expect("unbound closed track places");
    let wire = WireId(
        placed.command_acceptances[0]
            .created_entity
            .expect("wire identity"),
    );
    simulation
        .step(&[envelope(
            3,
            0,
            Command::BindPort(BindPortCommand {
                wire,
                end: WireEnd::A,
                target: EndpointTarget::Junction(junction),
            }),
        )])
        .expect("first junction end binds");
    let second = simulation
        .step(&[envelope(
            4,
            0,
            Command::BindPort(BindPortCommand {
                wire,
                end: WireEnd::B,
                target: EndpointTarget::Junction(junction),
            }),
        )])
        .expect("second same-junction end is a typed rejection");
    assert_eq!(
        second.command_rejections[0].reason,
        CommandRejectionReason::InvalidPortBinding
    );
}
