use aon_sim::{
    BalanceProfile, BindPortCommand, Command, CommandEnvelope, DriveStrength, DriverId,
    EndpointTarget, EntityId, FIXED_ONE, Fixed, FixedAabb, FixedVec2, GateId, GatePort,
    GatePortRef, GateSignalPorts, GateType, InitialWorld, JunctionId, LogicLevel, NumericProfile,
    PhysicalScaleProfile, PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceJunctionCommand,
    PlaceWireCommand, ProfileBundle, RemoveEntityCommand, RoutingDomain, SetExternalDriverCommand,
    Simulation, SimulationContract, SimulationPackage, StageFeatureSet, StepReport, Tick, WireEnd,
    WireId,
};

const CIRCUIT_PITCH: i64 = 16_384;
const SUBSTRATE_HALF_EXTENT: i64 = 128 * FIXED_ONE;
const EXTERNAL_STRENGTH: DriveStrength = DriveStrength(100);
const DIRECT_ROUTE_DELAY: u64 = 3;

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

fn simulation() -> Simulation {
    let mut balance = BalanceProfile::stage0_alpha("topology-sync");
    balance.fanout_free_load = 1_000;
    let profiles = ProfileBundle {
        numeric: NumericProfile::reference_v1("topology-sync"),
        physical_scale: PhysicalScaleProfile::stage0_alpha("topology-sync"),
        balance,
    };
    let contract = SimulationContract::from_profiles(&profiles).expect("profiles are valid");
    let mut required_features = StageFeatureSet::none();
    required_features.signal = true;
    Simulation::new(SimulationPackage::new(
        "topology-sync",
        InitialWorld::Empty,
        required_features,
        contract,
        profiles,
    ))
    .expect("the S0-M4 conformance simulation starts")
}

fn envelope(simulation: &Simulation, ordinal: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        target_tick: simulation.next_tick(),
        ordinal,
        command,
    }
}

fn step_commands(simulation: &mut Simulation, commands: Vec<(u64, Command)>) -> StepReport {
    let envelopes = commands
        .into_iter()
        .map(|(ordinal, command)| envelope(simulation, ordinal, command))
        .collect::<Vec<_>>();
    let report = simulation
        .step(&envelopes)
        .expect("the conformance command batch succeeds");
    assert!(report.command_rejections.is_empty());
    report
}

fn expect_created(simulation: &mut Simulation, command: Command) -> EntityId {
    let report = step_commands(simulation, vec![(0, command)]);
    assert_eq!(report.command_acceptances.len(), 1);
    report.command_acceptances[0]
        .created_entity
        .expect("the command creates one entity")
}

fn place_substrate(simulation: &mut Simulation) -> RoutingDomain {
    let bounds = FixedAabb::new(
        point(-SUBSTRATE_HALF_EXTENT, -SUBSTRATE_HALF_EXTENT),
        point(SUBSTRATE_HALF_EXTENT, SUBSTRATE_HALF_EXTENT),
    );
    let substrate = expect_created(
        simulation,
        Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
            origin: point(0, 0),
            routing_area: bounds,
            footprint: bounds,
        }),
    );
    RoutingDomain::FixedSubstrate(substrate)
}

fn place_not(
    simulation: &mut Simulation,
    domain: RoutingDomain,
    origin: FixedVec2,
) -> (GateId, StepReport) {
    let report = step_commands(
        simulation,
        vec![(
            0,
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Not,
                origin,
                routing_domain: domain,
            }),
        )],
    );
    let gate = GateId(
        report.command_acceptances[0]
            .created_entity
            .expect("placing a NOT Gate returns its EntityId"),
    );
    (gate, report)
}

fn place_wire(
    simulation: &mut Simulation,
    domain: RoutingDomain,
    points: Vec<FixedVec2>,
    endpoint_a: EndpointTarget,
    endpoint_b: EndpointTarget,
) -> (WireId, StepReport) {
    let report = step_commands(
        simulation,
        vec![(
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: domain,
                points,
                endpoint_a,
                endpoint_b,
            }),
        )],
    );
    let wire = WireId(
        report.command_acceptances[0]
            .created_entity
            .expect("placing a Wire returns its EntityId"),
    );
    (wire, report)
}

fn set_external(simulation: &mut Simulation, driver: DriverId, level: LogicLevel) -> StepReport {
    step_commands(
        simulation,
        vec![(
            0,
            Command::SetExternalDriver(SetExternalDriverCommand {
                driver,
                level,
                strength: EXTERNAL_STRENGTH,
            }),
        )],
    )
}

fn step_empty(simulation: &mut Simulation) -> StepReport {
    let report = simulation.step(&[]).expect("the empty Tick succeeds");
    assert!(!report.topology_changed);
    assert_eq!(report.signal_counters.routes_added, 0);
    assert_eq!(report.signal_counters.routes_removed, 0);
    assert_eq!(report.signal_counters.routes_retained, 0);
    assert_eq!(report.signal_counters.routes_replaced, 0);
    assert_eq!(report.signal_counters.topology_sync_arrivals_staged, 0);
    report
}

fn advance_to_due(simulation: &mut Simulation, due_tick: Tick) -> StepReport {
    while simulation.next_tick() < due_tick {
        step_empty(simulation);
    }
    assert_eq!(simulation.next_tick(), due_tick);
    step_empty(simulation)
}

fn settle_not_high(simulation: &mut Simulation, gates: &[GateId]) {
    for _ in 0..16 {
        if gates.iter().all(|&gate| {
            let state = simulation
                .gate_signal_state(gate)
                .expect("the NOT Gate remains live");
            state.current_output == LogicLevel::High
                && state.desired_output == LogicLevel::High
                && state.pending_due_tick.is_none()
        }) {
            return;
        }
        step_empty(simulation);
    }
    panic!("the NOT Gate fixture did not settle High within 16 Ticks");
}

fn output_endpoint(gate: GateId) -> EndpointTarget {
    EndpointTarget::GatePort(GatePortRef {
        gate,
        port: GatePort::Output,
    })
}

fn input_endpoint(gate: GateId) -> EndpointTarget {
    EndpointTarget::GatePort(GatePortRef {
        gate,
        port: GatePort::InputA,
    })
}

struct DirectFixture {
    simulation: Simulation,
    domain: RoutingDomain,
    source: GateId,
    downstream: GateId,
    source_ports: GateSignalPorts,
    downstream_ports: GateSignalPorts,
    wire: WireId,
    wire_points: Vec<FixedVec2>,
}

fn direct_fixture() -> DirectFixture {
    let mut simulation = simulation();
    let domain = place_substrate(&mut simulation);
    let (source, _) = place_not(&mut simulation, domain, point(0, 0));
    let (downstream, _) = place_not(&mut simulation, domain, point(34 * CIRCUIT_PITCH, 0));
    settle_not_high(&mut simulation, &[source, downstream]);
    let source_ports = simulation
        .gate_signal_ports(source)
        .expect("the source ports are observable");
    let downstream_ports = simulation
        .gate_signal_ports(downstream)
        .expect("the downstream ports are observable");
    let wire_points = vec![point(CIRCUIT_PITCH, 0), point(33 * CIRCUIT_PITCH, 0)];
    let attach_tick = simulation.next_tick();
    let (wire, report) = place_wire(
        &mut simulation,
        domain,
        wire_points.clone(),
        output_endpoint(source),
        input_endpoint(downstream),
    );
    assert_eq!(report.signal_counters.routes_added, 1);
    assert_eq!(report.signal_counters.routes_replaced, 0);
    assert_eq!(report.signal_counters.topology_sync_arrivals_staged, 1);
    assert_eq!(
        simulation.sink_driver_sample(downstream_ports.input_a.sink, source_ports.output),
        None
    );
    let due = Tick(attach_tick.0 + DIRECT_ROUTE_DELAY);
    let due_report = advance_to_due(&mut simulation, due);
    assert_eq!(due_report.signal_counters.signal_arrivals_applied, 1);
    let sample = simulation
        .sink_driver_sample(downstream_ports.input_a.sink, source_ports.output)
        .expect("the exact-delay topology sync creates the physical Slot");
    assert_eq!(sample.level, LogicLevel::High);
    assert_eq!(sample.revision.0, 1);

    DirectFixture {
        simulation,
        domain,
        source,
        downstream,
        source_ports,
        downstream_ports,
        wire,
        wire_points,
    }
}

fn stage_source_output_low(fixture: &mut DirectFixture) -> Tick {
    set_external(
        &mut fixture.simulation,
        fixture.source_ports.input_a.external_driver,
        LogicLevel::High,
    );
    let staged = step_empty(&mut fixture.simulation);
    assert!(staged.driver_changes.iter().any(|change| {
        change.driver == fixture.source_ports.output
            && change.current.level == LogicLevel::Low
            && change.current.revision.0 == 2
    }));
    Tick(staged.completed_tick.0 + DIRECT_ROUTE_DELAY)
}

#[test]
fn local_zero_sync_is_same_tick_while_c18_physical_sync_waits_exact_delay() {
    let mut simulation = simulation();
    let domain = place_substrate(&mut simulation);
    let (source, local_report) = place_not(&mut simulation, domain, point(0, 0));
    let source_ports = simulation.gate_signal_ports(source).unwrap();
    let local_sample = simulation
        .sink_driver_sample(
            source_ports.input_a.sink,
            source_ports.input_a.external_driver,
        )
        .expect("an empty-route sync applies in the Gate placement Tick");
    assert_eq!(local_sample.revision.0, 0);
    assert_eq!(local_sample.level, LogicLevel::Low);
    assert_eq!(local_report.signal_counters.routes_added, 1);
    assert_eq!(
        local_report.signal_counters.topology_sync_arrivals_staged,
        1
    );
    assert_eq!(local_report.signal_counters.signal_arrivals_applied, 1);

    let (downstream, _) = place_not(&mut simulation, domain, point(34 * CIRCUIT_PITCH, 0));
    settle_not_high(&mut simulation, &[source, downstream]);
    let source_ports = simulation.gate_signal_ports(source).unwrap();
    let downstream_ports = simulation.gate_signal_ports(downstream).unwrap();
    let attach_tick = simulation.next_tick();
    let (_, attach) = place_wire(
        &mut simulation,
        domain,
        vec![point(CIRCUIT_PITCH, 0), point(33 * CIRCUIT_PITCH, 0)],
        output_endpoint(source),
        input_endpoint(downstream),
    );
    assert_eq!(attach.signal_counters.routes_added, 1);
    assert_eq!(attach.signal_counters.topology_sync_arrivals_staged, 1);
    assert_eq!(
        simulation.sink_level(downstream_ports.input_a.sink),
        Some(LogicLevel::Low),
        "C-18: attaching a physical route cannot expose the live High source immediately"
    );
    assert_eq!(
        simulation.sink_driver_sample(downstream_ports.input_a.sink, source_ports.output),
        None
    );

    let due_tick = Tick(attach_tick.0 + DIRECT_ROUTE_DELAY);
    while simulation.next_tick() < due_tick {
        let early = step_empty(&mut simulation);
        assert_eq!(early.signal_counters.signal_arrivals_applied, 0);
        assert_eq!(
            simulation.sink_driver_sample(downstream_ports.input_a.sink, source_ports.output),
            None
        );
        assert_eq!(
            simulation.sink_level(downstream_ports.input_a.sink),
            Some(LogicLevel::Low)
        );
    }
    let due = step_empty(&mut simulation);
    assert_eq!(due.completed_tick, due_tick);
    assert_eq!(due.signal_counters.signal_arrivals_applied, 1);
    let applied = simulation
        .sink_driver_sample(downstream_ports.input_a.sink, source_ports.output)
        .unwrap();
    assert_eq!(applied.level, LogicLevel::High);
    assert_eq!(applied.revision.0, 1);
    assert_eq!(
        simulation.sink_level(downstream_ports.input_a.sink),
        Some(LogicLevel::High)
    );
}

#[test]
fn replaced_shorter_route_sync_and_same_tick_revision_win_preserve_c19() {
    const VERTICAL_SEPARATION: i64 = 46 * CIRCUIT_PITCH;
    const LONG_DELAY: u64 = 8;
    const SHORT_DELAY: u64 = 5;

    let mut simulation = simulation();
    let domain = place_substrate(&mut simulation);
    let (source, _) = place_not(&mut simulation, domain, point(0, 0));
    let (downstream, _) = place_not(&mut simulation, domain, point(0, VERTICAL_SEPARATION));
    settle_not_high(&mut simulation, &[source, downstream]);
    let source_ports = simulation.gate_signal_ports(source).unwrap();
    let downstream_ports = simulation.gate_signal_ports(downstream).unwrap();

    set_external(
        &mut simulation,
        source_ports.input_a.external_driver,
        LogicLevel::High,
    );
    set_external(
        &mut simulation,
        source_ports.input_a.external_driver,
        LogicLevel::Low,
    );
    settle_not_high(&mut simulation, &[source]);
    assert_eq!(
        simulation
            .driver_sample(source_ports.input_a.external_driver)
            .unwrap()
            .revision
            .0,
        2
    );

    let long_points = vec![
        point(-CIRCUIT_PITCH, 0),
        point(-20 * CIRCUIT_PITCH, 16 * CIRCUIT_PITCH),
        point(-20 * CIRCUIT_PITCH, 30 * CIRCUIT_PITCH),
        point(-CIRCUIT_PITCH, VERTICAL_SEPARATION),
    ];
    let long_tick = simulation.next_tick();
    let long = step_commands(
        &mut simulation,
        vec![
            (
                10,
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: domain,
                    points: long_points,
                    endpoint_a: input_endpoint(source),
                    endpoint_b: input_endpoint(downstream),
                }),
            ),
            (
                20,
                Command::SetExternalDriver(SetExternalDriverCommand {
                    driver: source_ports.input_a.external_driver,
                    level: LogicLevel::High,
                    strength: EXTERNAL_STRENGTH,
                }),
            ),
        ],
    );
    assert_eq!(long.signal_counters.routes_added, 2);
    assert_eq!(long.signal_counters.routes_replaced, 0);
    assert_eq!(long.signal_counters.topology_sync_arrivals_staged, 2);
    assert_eq!(
        simulation
            .driver_sample(source_ports.input_a.external_driver)
            .unwrap()
            .revision
            .0,
        3
    );

    let short_points = vec![
        point(-CIRCUIT_PITCH, 0),
        point(-2 * CIRCUIT_PITCH, CIRCUIT_PITCH),
        point(-2 * CIRCUIT_PITCH, 45 * CIRCUIT_PITCH),
        point(-CIRCUIT_PITCH, VERTICAL_SEPARATION),
    ];
    let replacement_tick = simulation.next_tick();
    let replaced = step_commands(
        &mut simulation,
        vec![
            (
                10,
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: domain,
                    points: short_points,
                    endpoint_a: input_endpoint(source),
                    endpoint_b: input_endpoint(downstream),
                }),
            ),
            (
                20,
                Command::SetExternalDriver(SetExternalDriverCommand {
                    driver: source_ports.input_a.external_driver,
                    level: LogicLevel::Low,
                    strength: EXTERNAL_STRENGTH,
                }),
            ),
        ],
    );
    assert_eq!(replaced.signal_counters.routes_added, 0);
    assert_eq!(replaced.signal_counters.routes_removed, 0);
    assert_eq!(replaced.signal_counters.routes_replaced, 2);
    assert_eq!(replaced.signal_counters.topology_sync_arrivals_staged, 2);
    assert_eq!(
        simulation
            .driver_sample(source_ports.input_a.external_driver)
            .unwrap()
            .revision
            .0,
        4
    );

    let short_due = Tick(replacement_tick.0 + SHORT_DELAY);
    while simulation.next_tick() < short_due {
        step_empty(&mut simulation);
        assert_eq!(
            simulation.sink_driver_sample(
                downstream_ports.input_a.sink,
                source_ports.input_a.external_driver,
            ),
            None
        );
    }
    let revision_four = step_empty(&mut simulation);
    assert_eq!(revision_four.completed_tick, short_due);
    assert_eq!(revision_four.signal_counters.stale_revision_arrivals, 1);
    assert_eq!(revision_four.signal_counters.invalid_path_arrivals, 0);
    let winning = simulation
        .sink_driver_sample(
            downstream_ports.input_a.sink,
            source_ports.input_a.external_driver,
        )
        .expect("same-Tick propagation Revision 4 beats sync Revision 3");
    assert_eq!(winning.level, LogicLevel::Low);
    assert_eq!(winning.revision.0, 4);

    let original_due = Tick(long_tick.0 + LONG_DELAY);
    let old_arrival = advance_to_due(&mut simulation, original_due);
    assert_eq!(old_arrival.completed_tick, original_due);
    assert_eq!(old_arrival.signal_counters.stale_revision_arrivals, 2);
    assert_eq!(old_arrival.signal_counters.invalid_path_arrivals, 0);
    assert_eq!(old_arrival.signal_counters.idempotent_signal_arrivals, 1);
    assert_eq!(old_arrival.signal_counters.signal_arrivals_applied, 0);
    let after_old_revision_three = simulation
        .sink_driver_sample(
            downstream_ports.input_a.sink,
            source_ports.input_a.external_driver,
        )
        .unwrap();
    assert_eq!(after_old_revision_three, winning);
    assert_eq!(
        simulation.sink_level(downstream_ports.input_a.sink),
        Some(LogicLevel::Low),
        "C-19: the still-valid old path cannot restore Revision 3 High after Revision 4 Low"
    );
}

#[test]
fn removing_a_route_deletes_its_slot_and_resolves_passive_low_without_an_arrival() {
    let mut fixture = direct_fixture();
    assert_eq!(
        fixture
            .simulation
            .sink_level(fixture.downstream_ports.input_a.sink),
        Some(LogicLevel::High)
    );

    let removed = step_commands(
        &mut fixture.simulation,
        vec![(
            0,
            Command::RemoveEntity(RemoveEntityCommand {
                target: fixture.wire.entity_id(),
            }),
        )],
    );
    assert_eq!(removed.signal_counters.routes_removed, 1);
    assert_eq!(removed.signal_counters.routes_added, 0);
    assert_eq!(removed.signal_counters.routes_replaced, 0);
    assert_eq!(removed.signal_counters.topology_sync_arrivals_staged, 0);
    assert_eq!(removed.signal_counters.signal_arrivals_applied, 0);
    assert_eq!(removed.signal_counters.sinks_resolved, 1);
    assert_eq!(
        fixture.simulation.sink_driver_sample(
            fixture.downstream_ports.input_a.sink,
            fixture.source_ports.output,
        ),
        None
    );
    assert_eq!(
        fixture
            .simulation
            .sink_level(fixture.downstream_ports.input_a.sink),
        Some(LogicLevel::Low)
    );
}

#[test]
fn removing_an_in_flight_route_resolves_a_live_sink_even_before_its_slot_exists() {
    let mut simulation = simulation();
    let domain = place_substrate(&mut simulation);
    let (source, _) = place_not(&mut simulation, domain, point(0, 0));
    let (downstream, _) = place_not(&mut simulation, domain, point(34 * CIRCUIT_PITCH, 0));
    settle_not_high(&mut simulation, &[source, downstream]);
    let source_ports = simulation.gate_signal_ports(source).unwrap();
    let downstream_ports = simulation.gate_signal_ports(downstream).unwrap();
    let attach_tick = simulation.next_tick();
    let (wire, attached) = place_wire(
        &mut simulation,
        domain,
        vec![point(CIRCUIT_PITCH, 0), point(33 * CIRCUIT_PITCH, 0)],
        output_endpoint(source),
        input_endpoint(downstream),
    );
    assert_eq!(attached.signal_counters.routes_added, 1);
    assert_eq!(
        simulation.sink_driver_sample(downstream_ports.input_a.sink, source_ports.output),
        None
    );

    let removed = step_commands(
        &mut simulation,
        vec![(
            0,
            Command::RemoveEntity(RemoveEntityCommand {
                target: wire.entity_id(),
            }),
        )],
    );
    assert_eq!(removed.signal_counters.routes_removed, 1);
    assert_eq!(removed.signal_counters.sinks_resolved, 1);
    assert!(removed.signal_changes.is_empty());
    assert_eq!(
        simulation.sink_driver_sample(downstream_ports.input_a.sink, source_ports.output),
        None
    );
    assert_eq!(
        simulation.sink_level(downstream_ports.input_a.sink),
        Some(LogicLevel::Low)
    );

    let invalidated = advance_to_due(&mut simulation, Tick(attach_tick.0 + DIRECT_ROUTE_DELAY));
    assert_eq!(invalidated.signal_counters.invalid_path_arrivals, 1);
    assert_eq!(invalidated.signal_counters.signal_arrivals_applied, 0);
}

enum WireMutation {
    Remove,
    RebindAway,
    BindAwayAndBack,
    Rebuild,
}

fn assert_wire_mutation_invalidates_pending_arrival(mutation: WireMutation) {
    let mut fixture = direct_fixture();
    let old_due = stage_source_output_low(&mut fixture);
    let target = input_endpoint(fixture.downstream);
    let mutation_report = match mutation {
        WireMutation::Remove => step_commands(
            &mut fixture.simulation,
            vec![(
                0,
                Command::RemoveEntity(RemoveEntityCommand {
                    target: fixture.wire.entity_id(),
                }),
            )],
        ),
        WireMutation::RebindAway => step_commands(
            &mut fixture.simulation,
            vec![(
                0,
                Command::BindPort(BindPortCommand {
                    wire: fixture.wire,
                    end: WireEnd::B,
                    target: EndpointTarget::Free,
                }),
            )],
        ),
        WireMutation::BindAwayAndBack => step_commands(
            &mut fixture.simulation,
            vec![
                (
                    10,
                    Command::BindPort(BindPortCommand {
                        wire: fixture.wire,
                        end: WireEnd::B,
                        target: EndpointTarget::Free,
                    }),
                ),
                (
                    20,
                    Command::BindPort(BindPortCommand {
                        wire: fixture.wire,
                        end: WireEnd::B,
                        target,
                    }),
                ),
            ],
        ),
        WireMutation::Rebuild => {
            let report = step_commands(
                &mut fixture.simulation,
                vec![
                    (
                        10,
                        Command::RemoveEntity(RemoveEntityCommand {
                            target: fixture.wire.entity_id(),
                        }),
                    ),
                    (
                        20,
                        Command::PlaceWire(PlaceWireCommand {
                            routing_domain: fixture.domain,
                            points: fixture.wire_points.clone(),
                            endpoint_a: output_endpoint(fixture.source),
                            endpoint_b: target,
                        }),
                    ),
                ],
            );
            let rebuilt = WireId(
                report.command_acceptances[1]
                    .created_entity
                    .expect("the replacement Wire has a fresh ID"),
            );
            assert_ne!(rebuilt, fixture.wire);
            assert_eq!(fixture.simulation.wire_signal_state(fixture.wire), None);
            assert!(fixture.simulation.wire_signal_state(rebuilt).is_some());
            report
        }
    };

    match mutation {
        WireMutation::Remove | WireMutation::RebindAway => {
            assert_eq!(mutation_report.signal_counters.routes_removed, 1);
            assert_eq!(mutation_report.signal_counters.routes_replaced, 0);
            assert_eq!(
                mutation_report
                    .signal_counters
                    .topology_sync_arrivals_staged,
                0
            );
        }
        WireMutation::BindAwayAndBack | WireMutation::Rebuild => {
            assert_eq!(mutation_report.signal_counters.routes_removed, 0);
            assert_eq!(mutation_report.signal_counters.routes_replaced, 1);
            assert_eq!(
                mutation_report
                    .signal_counters
                    .topology_sync_arrivals_staged,
                1
            );
        }
    }

    let invalid = advance_to_due(&mut fixture.simulation, old_due);
    assert_eq!(invalid.signal_counters.invalid_path_arrivals, 1);
    assert_eq!(invalid.signal_counters.signal_arrivals_applied, 0);
}

#[test]
fn wire_remove_rebind_bind_away_back_and_rebuild_invalidate_old_certificates() {
    assert_wire_mutation_invalidates_pending_arrival(WireMutation::Remove);
    assert_wire_mutation_invalidates_pending_arrival(WireMutation::RebindAway);
    assert_wire_mutation_invalidates_pending_arrival(WireMutation::BindAwayAndBack);
    assert_wire_mutation_invalidates_pending_arrival(WireMutation::Rebuild);
}

#[test]
fn removing_a_stamped_junction_invalidates_the_pending_arrival() {
    let mut simulation = simulation();
    let domain = place_substrate(&mut simulation);
    let (source, _) = place_not(&mut simulation, domain, point(0, 0));
    let (downstream, _) = place_not(&mut simulation, domain, point(34 * CIRCUIT_PITCH, 0));
    settle_not_high(&mut simulation, &[source, downstream]);
    let source_ports = simulation.gate_signal_ports(source).unwrap();
    let downstream_ports = simulation.gate_signal_ports(downstream).unwrap();
    let junction = JunctionId(expect_created(
        &mut simulation,
        Command::PlaceJunction(PlaceJunctionCommand {
            routing_domain: domain,
            position: point(17 * CIRCUIT_PITCH, 0),
        }),
    ));
    place_wire(
        &mut simulation,
        domain,
        vec![point(CIRCUIT_PITCH, 0), point(17 * CIRCUIT_PITCH, 0)],
        output_endpoint(source),
        EndpointTarget::Junction(junction),
    );
    let attach_tick = simulation.next_tick();
    let (_, attached) = place_wire(
        &mut simulation,
        domain,
        vec![point(17 * CIRCUIT_PITCH, 0), point(33 * CIRCUIT_PITCH, 0)],
        EndpointTarget::Junction(junction),
        input_endpoint(downstream),
    );
    assert_eq!(attached.signal_counters.routes_added, 1);
    advance_to_due(&mut simulation, Tick(attach_tick.0 + DIRECT_ROUTE_DELAY));
    assert_eq!(
        simulation
            .sink_driver_sample(downstream_ports.input_a.sink, source_ports.output)
            .unwrap()
            .level,
        LogicLevel::High
    );

    set_external(
        &mut simulation,
        source_ports.input_a.external_driver,
        LogicLevel::High,
    );
    let staged = step_empty(&mut simulation);
    assert!(
        staged
            .driver_changes
            .iter()
            .any(|change| change.driver == source_ports.output)
    );
    let old_due = Tick(staged.completed_tick.0 + DIRECT_ROUTE_DELAY);
    let removed = step_commands(
        &mut simulation,
        vec![(
            0,
            Command::RemoveEntity(RemoveEntityCommand {
                target: junction.entity_id(),
            }),
        )],
    );
    assert_eq!(removed.signal_counters.routes_removed, 1);

    let invalid = advance_to_due(&mut simulation, old_due);
    assert_eq!(invalid.signal_counters.invalid_path_arrivals, 1);
    assert_eq!(invalid.signal_counters.signal_arrivals_applied, 0);
}

#[test]
fn binding_another_incident_wire_advances_the_stamped_junction_and_invalidates_old_arrivals() {
    let mut simulation = simulation();
    let domain = place_substrate(&mut simulation);
    let (source, _) = place_not(&mut simulation, domain, point(0, 0));
    let (downstream, _) = place_not(&mut simulation, domain, point(34 * CIRCUIT_PITCH, 0));
    settle_not_high(&mut simulation, &[source, downstream]);
    let source_ports = simulation.gate_signal_ports(source).unwrap();
    let downstream_ports = simulation.gate_signal_ports(downstream).unwrap();
    let junction = JunctionId(expect_created(
        &mut simulation,
        Command::PlaceJunction(PlaceJunctionCommand {
            routing_domain: domain,
            position: point(17 * CIRCUIT_PITCH, 0),
        }),
    ));
    place_wire(
        &mut simulation,
        domain,
        vec![point(CIRCUIT_PITCH, 0), point(17 * CIRCUIT_PITCH, 0)],
        output_endpoint(source),
        EndpointTarget::Junction(junction),
    );
    let attach_tick = simulation.next_tick();
    place_wire(
        &mut simulation,
        domain,
        vec![point(17 * CIRCUIT_PITCH, 0), point(33 * CIRCUIT_PITCH, 0)],
        EndpointTarget::Junction(junction),
        input_endpoint(downstream),
    );
    advance_to_due(&mut simulation, Tick(attach_tick.0 + DIRECT_ROUTE_DELAY));
    assert_eq!(
        simulation
            .sink_driver_sample(downstream_ports.input_a.sink, source_ports.output)
            .unwrap()
            .level,
        LogicLevel::High
    );
    let (probe, _) = place_wire(
        &mut simulation,
        domain,
        vec![
            point(17 * CIRCUIT_PITCH, 0),
            point(17 * CIRCUIT_PITCH, 8 * CIRCUIT_PITCH),
        ],
        EndpointTarget::Free,
        EndpointTarget::Free,
    );

    set_external(
        &mut simulation,
        source_ports.input_a.external_driver,
        LogicLevel::High,
    );
    let staged = step_empty(&mut simulation);
    assert!(
        staged
            .driver_changes
            .iter()
            .any(|change| change.driver == source_ports.output)
    );
    let old_due = Tick(staged.completed_tick.0 + DIRECT_ROUTE_DELAY);
    let rebound = step_commands(
        &mut simulation,
        vec![(
            0,
            Command::BindPort(BindPortCommand {
                wire: probe,
                end: WireEnd::A,
                target: EndpointTarget::Junction(junction),
            }),
        )],
    );
    assert_eq!(rebound.signal_counters.routes_removed, 0);
    assert_eq!(rebound.signal_counters.routes_replaced, 1);
    assert_eq!(rebound.signal_counters.topology_sync_arrivals_staged, 1);

    let invalid = advance_to_due(&mut simulation, old_due);
    assert_eq!(invalid.signal_counters.invalid_path_arrivals, 1);
}

#[test]
fn unrelated_topology_edit_keeps_a_pending_certificate_valid() {
    let mut fixture = direct_fixture();
    let old_due = stage_source_output_low(&mut fixture);
    let edit = step_commands(
        &mut fixture.simulation,
        vec![(
            0,
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: fixture.domain,
                position: point(0, 16 * FIXED_ONE),
            }),
        )],
    );
    assert!(edit.topology_changed);
    assert_eq!(edit.signal_counters.routes_added, 0);
    assert_eq!(edit.signal_counters.routes_removed, 0);
    assert_eq!(edit.signal_counters.routes_replaced, 0);
    assert_eq!(edit.signal_counters.routes_retained, 3);
    assert_eq!(edit.signal_counters.topology_sync_arrivals_staged, 0);

    let delivered = advance_to_due(&mut fixture.simulation, old_due);
    assert_eq!(delivered.signal_counters.invalid_path_arrivals, 0);
    assert_eq!(delivered.signal_counters.stale_revision_arrivals, 0);
    assert_eq!(delivered.signal_counters.signal_arrivals_applied, 1);
    let sample = fixture
        .simulation
        .sink_driver_sample(
            fixture.downstream_ports.input_a.sink,
            fixture.source_ports.output,
        )
        .expect("the unrelated edit leaves the old path certificate valid");
    assert_eq!(sample.level, LogicLevel::Low);
    assert_eq!(sample.revision.0, 2);
}

#[test]
fn route_diagnostics_and_public_observations_do_not_mutate_state_hash() {
    let mut fixture = direct_fixture();
    let edit = step_commands(
        &mut fixture.simulation,
        vec![(
            0,
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: fixture.domain,
                position: point(0, 16 * FIXED_ONE),
            }),
        )],
    );
    assert_eq!(edit.signal_counters.routes_retained, 3);
    let before = fixture.simulation.state_hash();

    let _diagnostics = (
        edit.signal_counters.routes_added,
        edit.signal_counters.routes_removed,
        edit.signal_counters.routes_retained,
        edit.signal_counters.routes_replaced,
        edit.signal_counters.topology_sync_arrivals_staged,
    );
    let _contract = fixture.simulation.contract();
    let _profiles = fixture.simulation.profiles();
    let _scenario_id = fixture.simulation.scenario_id();
    let _tick = fixture.simulation.next_tick();
    let _topology_revision = fixture.simulation.topology_revision();
    let _ports = fixture.simulation.gate_signal_ports(fixture.source);
    let _driver = fixture
        .simulation
        .driver_sample(fixture.source_ports.output);
    let _sink = fixture
        .simulation
        .sink_level(fixture.downstream_ports.input_a.sink);
    let _slot = fixture.simulation.sink_driver_sample(
        fixture.downstream_ports.input_a.sink,
        fixture.source_ports.output,
    );
    let _gate = fixture.simulation.gate_signal_state(fixture.source);
    let _wire = fixture.simulation.wire_signal_state(fixture.wire);
    let mut render = aon_sim::RenderSnapshot::default();
    fixture.simulation.write_render_snapshot(&mut render);

    assert_eq!(fixture.simulation.state_hash(), before);
}
