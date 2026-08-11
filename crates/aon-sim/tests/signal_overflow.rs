use aon_sim::{
    BalanceProfile, Command, CommandEnvelope, DriveStrength, DriverId, DriverSample,
    EndpointTarget, EntityId, FIXED_ONE, Fixed, FixedAabb, FixedVec2, GateId, GatePort,
    GatePortRef, GateSignalSnapshot, GateType, InitialWorld, LogicLevel, NumericProfile,
    PhysicalScaleProfile, PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceJunctionCommand,
    PlaceWireCommand, ProfileBundle, Rational, RenderSnapshot, Revision, RoutingDomain,
    SetExternalDriverCommand, Simulation, SimulationContract, SimulationError, SimulationPackage,
    SinkId, StageFeatureSet, StateHash, Tick, WireId, WireSignalSnapshot,
};

const CIRCUIT_PITCH: i64 = 16_384;
const HALF_EXTENT: i64 = 32 * FIXED_ONE;

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

fn simulation(balance: BalanceProfile) -> Simulation {
    let profiles = ProfileBundle {
        numeric: NumericProfile::reference_v1("signal-overflow"),
        physical_scale: PhysicalScaleProfile::stage0_alpha("signal-overflow"),
        balance,
    };
    let contract = SimulationContract::from_profiles(&profiles).expect("profiles are valid");
    let mut required = StageFeatureSet::none();
    required.signal = true;
    Simulation::new(SimulationPackage::new(
        "signal-overflow",
        InitialWorld::Empty,
        required,
        contract,
        profiles,
    ))
    .expect("overflow fixture starts")
}

fn envelope(simulation: &Simulation, ordinal: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        target_tick: simulation.next_tick(),
        ordinal,
        command,
    }
}

fn place_substrate(simulation: &mut Simulation) -> RoutingDomain {
    let bounds = FixedAabb::new(
        point(-HALF_EXTENT, -HALF_EXTENT),
        point(HALF_EXTENT, HALF_EXTENT),
    );
    let report = simulation
        .step(&[envelope(
            simulation,
            0,
            Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                origin: point(0, 0),
                routing_area: bounds,
                footprint: bounds,
            }),
        )])
        .expect("Substrate placement succeeds");
    RoutingDomain::FixedSubstrate(
        report.command_acceptances[0]
            .created_entity
            .expect("Substrate allocates an EntityId"),
    )
}

fn place_not(simulation: &mut Simulation, domain: RoutingDomain) -> GateId {
    place_not_at(simulation, domain, point(0, 0))
}

fn place_not_at(simulation: &mut Simulation, domain: RoutingDomain, origin: FixedVec2) -> GateId {
    let report = simulation
        .step(&[envelope(
            simulation,
            0,
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Not,
                origin,
                routing_domain: domain,
            }),
        )])
        .expect("NOT placement succeeds");
    GateId(
        report.command_acceptances[0]
            .created_entity
            .expect("Gate allocates an EntityId"),
    )
}

#[derive(Debug, PartialEq, Eq)]
struct ObservableCheckpoint {
    next_tick: Tick,
    topology_revision: Revision,
    state_hash: StateHash,
    render: RenderSnapshot,
    gates: Vec<(GateId, GateSignalSnapshot)>,
    drivers: Vec<(DriverId, Option<DriverSample>)>,
    sinks: Vec<(SinkId, Option<LogicLevel>)>,
    wires: Vec<(WireId, Option<WireSignalSnapshot>)>,
}

fn checkpoint(simulation: &Simulation, gates: &[GateId], wires: &[WireId]) -> ObservableCheckpoint {
    let mut render = RenderSnapshot::default();
    simulation.write_render_snapshot(&mut render);
    let gate_states: Vec<_> = gates
        .iter()
        .map(|gate| {
            (
                *gate,
                simulation
                    .gate_signal_state(*gate)
                    .expect("checkpoint Gate remains observable"),
            )
        })
        .collect();
    let mut drivers = Vec::new();
    let mut sinks = Vec::new();
    for (_, state) in &gate_states {
        drivers.push((
            state.ports.input_a.external_driver,
            simulation.driver_sample(state.ports.input_a.external_driver),
        ));
        sinks.push((
            state.ports.input_a.sink,
            simulation.sink_level(state.ports.input_a.sink),
        ));
        if let Some(input_b) = state.ports.input_b {
            drivers.push((
                input_b.external_driver,
                simulation.driver_sample(input_b.external_driver),
            ));
            sinks.push((input_b.sink, simulation.sink_level(input_b.sink)));
        }
        drivers.push((
            state.ports.output,
            simulation.driver_sample(state.ports.output),
        ));
    }
    drivers.sort_unstable_by_key(|(driver, _)| *driver);
    sinks.sort_unstable_by_key(|(sink, _)| *sink);
    ObservableCheckpoint {
        next_tick: simulation.next_tick(),
        topology_revision: simulation.topology_revision(),
        state_hash: simulation.state_hash(),
        render,
        gates: gate_states,
        drivers,
        sinks,
        wires: wires
            .iter()
            .map(|wire| (*wire, simulation.wire_signal_state(*wire)))
            .collect(),
    }
}

fn assert_gate_endpoints_match(
    left: &Simulation,
    right: &Simulation,
    gate: GateId,
    expected_drivers: (u64, u64),
    expected_sink: u64,
) {
    let left_ports = left
        .gate_signal_ports(gate)
        .expect("left retry Gate exposes signal endpoints");
    let right_ports = right
        .gate_signal_ports(gate)
        .expect("right retry Gate exposes signal endpoints");
    assert_eq!(left_ports, right_ports);
    assert_eq!(
        left_ports.input_a.external_driver.0,
        EntityId(expected_drivers.0)
    );
    assert_eq!(left_ports.output.0, EntityId(expected_drivers.1));
    assert_eq!(left_ports.input_a.sink.0, EntityId(expected_sink));
}

fn output_wire(gate: GateId, domain: RoutingDomain) -> Command {
    Command::PlaceWire(PlaceWireCommand {
        routing_domain: domain,
        points: vec![point(CIRCUIT_PITCH, 0), point(8 * FIXED_ONE, 0)],
        endpoint_a: EndpointTarget::GatePort(GatePortRef {
            gate,
            port: GatePort::Output,
        }),
        endpoint_b: EndpointTarget::Free,
    })
}

#[test]
fn fanout_delay_overflow_public_tick_rolls_back_every_frontier_and_observation() {
    let mut balance = BalanceProfile::stage0_alpha("fanout-overflow");
    balance.wire_load_per_wu = Rational::new(i64::MAX, 1).expect("positive Rational");
    balance.fanout_free_load = 0;
    balance.fanout_step = 1;
    // An isolated NOT can schedule normally, while adding a one-wu output Wire computes
    // gateBaseDelay + fanoutPenalty == 2^64 exactly.
    balance.gate_base_delay = u64::MAX - i64::MAX as u64 + 1;

    let mut failed = simulation(balance.clone());
    let mut control = simulation(balance);
    let failed_domain = place_substrate(&mut failed);
    let control_domain = place_substrate(&mut control);
    let failed_gate = place_not(&mut failed, failed_domain);
    let control_gate = place_not(&mut control, control_domain);
    assert_eq!(failed_gate, control_gate);

    let failed_wire = WireId(EntityId(3));
    let before = checkpoint(&failed, &[failed_gate], &[failed_wire]);
    let command = Command::PlaceWire(PlaceWireCommand {
        routing_domain: failed_domain,
        points: vec![point(CIRCUIT_PITCH, 0), point(CIRCUIT_PITCH + FIXED_ONE, 0)],
        endpoint_a: EndpointTarget::GatePort(GatePortRef {
            gate: failed_gate,
            port: GatePort::Output,
        }),
        endpoint_b: EndpointTarget::Free,
    });
    assert_eq!(
        failed.step(&[envelope(&failed, 0, command)]),
        Err(SimulationError::NumericOverflow)
    );
    assert_eq!(checkpoint(&failed, &[failed_gate], &[failed_wire]), before);
    assert_eq!(failed.state_hash(), control.state_hash());

    // A successful event-producing Gate allocation must consume the same Entity, Driver, Sink,
    // and payload frontiers as the untouched control. The post-step hash covers the payload order.
    let retry_origin = point(16 * CIRCUIT_PITCH, 0);
    let failed_retry = place_not_at(&mut failed, failed_domain, retry_origin);
    let control_retry = place_not_at(&mut control, control_domain, retry_origin);
    assert_eq!(failed_retry, GateId(EntityId(3)));
    assert_eq!(failed_retry, control_retry);
    assert_gate_endpoints_match(&failed, &control, failed_retry, (3, 4), 2);
    assert_eq!(failed.state_hash(), control.state_hash());
}

#[test]
fn physical_wire_delay_overflow_public_tick_rolls_back_every_frontier_and_observation() {
    let mut balance = BalanceProfile::stage0_alpha("wire-delay-overflow");
    balance.wire_quadratic_k = Rational::new(i64::MAX, 1).expect("positive Rational");

    let mut failed = simulation(balance.clone());
    let mut control = simulation(balance);
    let failed_domain = place_substrate(&mut failed);
    let control_domain = place_substrate(&mut control);
    let failed_source = place_not(&mut failed, failed_domain);
    let control_source = place_not(&mut control, control_domain);
    let downstream_origin = point(10 * CIRCUIT_PITCH, 0);
    let failed_downstream = place_not_at(&mut failed, failed_domain, downstream_origin);
    let control_downstream = place_not_at(&mut control, control_domain, downstream_origin);
    assert_eq!(failed_source, control_source);
    assert_eq!(failed_downstream, control_downstream);

    let failed_wire = WireId(EntityId(4));
    let before = checkpoint(&failed, &[failed_source, failed_downstream], &[failed_wire]);
    // The anchors are exactly two wu apart, so i64::MAX * length^2 cannot fit a Tick.
    let command = Command::PlaceWire(PlaceWireCommand {
        routing_domain: failed_domain,
        points: vec![point(CIRCUIT_PITCH, 0), point(9 * CIRCUIT_PITCH, 0)],
        endpoint_a: EndpointTarget::GatePort(GatePortRef {
            gate: failed_source,
            port: GatePort::Output,
        }),
        endpoint_b: EndpointTarget::GatePort(GatePortRef {
            gate: failed_downstream,
            port: GatePort::InputA,
        }),
    });
    assert_eq!(
        failed.step(&[envelope(&failed, 0, command)]),
        Err(SimulationError::NumericOverflow)
    );
    assert_eq!(
        checkpoint(&failed, &[failed_source, failed_downstream], &[failed_wire],),
        before
    );
    assert_eq!(failed.state_hash(), control.state_hash());

    let retry_origin = point(20 * CIRCUIT_PITCH, 0);
    let failed_retry = place_not_at(&mut failed, failed_domain, retry_origin);
    let control_retry = place_not_at(&mut control, control_domain, retry_origin);
    assert_eq!(failed_retry, GateId(EntityId(4)));
    assert_eq!(failed_retry, control_retry);
    assert_gate_endpoints_match(&failed, &control, failed_retry, (5, 6), 3);
    assert_eq!(failed.state_hash(), control.state_hash());
}

#[test]
fn due_tick_overflow_rolls_back_gate_signal_ids_events_and_structural_id() {
    let mut balance = BalanceProfile::stage0_alpha("due-overflow");
    balance.gate_base_delay = u64::MAX;
    let mut simulation = simulation(balance);
    let domain = place_substrate(&mut simulation);
    let before_tick = simulation.next_tick();
    let before_hash = simulation.state_hash();

    let gate = Command::PlaceGate(PlaceGateCommand {
        gate_type: GateType::Not,
        origin: point(0, 0),
        routing_domain: domain,
    });
    assert_eq!(
        simulation.step(&[envelope(&simulation, 0, gate)]),
        Err(SimulationError::NumericOverflow)
    );
    assert_eq!(simulation.next_tick(), before_tick);
    assert_eq!(simulation.state_hash(), before_hash);
    assert_eq!(simulation.gate_signal_ports(GateId(EntityId(2))), None);

    let retry = simulation
        .step(&[envelope(
            &simulation,
            0,
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: domain,
                position: point(4 * FIXED_ONE, 0),
            }),
        )])
        .expect("a non-Gate retry succeeds");
    assert_eq!(
        retry.command_acceptances[0].created_entity,
        Some(EntityId(2))
    );
}

#[test]
fn component_load_overflow_rolls_back_wire_and_preserves_pending_gate_event() {
    let mut balance = BalanceProfile::stage0_alpha("load-overflow");
    balance.wire_load_per_wu = Rational::new(i64::MAX, 1).expect("positive Rational");
    let mut simulation = simulation(balance);
    let domain = place_substrate(&mut simulation);
    let gate = place_not(&mut simulation, domain);
    let before_tick = simulation.next_tick();
    let before_hash = simulation.state_hash();

    assert_eq!(
        simulation.step(&[envelope(&simulation, 0, output_wire(gate, domain))]),
        Err(SimulationError::NumericOverflow)
    );
    assert_eq!(simulation.next_tick(), before_tick);
    assert_eq!(simulation.state_hash(), before_hash);
    assert_eq!(simulation.wire_signal_state(WireId(EntityId(3))), None);

    let retry = simulation
        .step(&[envelope(
            &simulation,
            0,
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: domain,
                position: point(4 * FIXED_ONE, 0),
            }),
        )])
        .expect("rollback leaves the structural allocation frontier intact");
    assert_eq!(
        retry.command_acceptances[0].created_entity,
        Some(EntityId(3))
    );
}

#[test]
fn switch_energy_overflow_rolls_back_external_sample_and_gate_pending_state() {
    let mut balance = BalanceProfile::stage0_alpha("energy-overflow");
    balance.gate_switch_base_energy = u64::MAX;
    let mut simulation = simulation(balance);
    let domain = place_substrate(&mut simulation);
    let gate = place_not(&mut simulation, domain);
    simulation
        .step(&[envelope(&simulation, 0, output_wire(gate, domain))])
        .expect("Wire placement and startup transition succeed");
    let ports = simulation
        .gate_signal_ports(gate)
        .expect("Gate ports exist");
    let before_tick = simulation.next_tick();
    let before_hash = simulation.state_hash();
    let before_driver = simulation
        .driver_sample(ports.input_a.external_driver)
        .expect("external Driver exists");
    let before_gate = simulation
        .gate_signal_state(gate)
        .expect("Gate signal state exists");

    let update = Command::SetExternalDriver(SetExternalDriverCommand {
        driver: ports.input_a.external_driver,
        level: LogicLevel::High,
        strength: DriveStrength(100),
    });
    assert_eq!(
        simulation.step(&[envelope(&simulation, 0, update)]),
        Err(SimulationError::NumericOverflow)
    );
    assert_eq!(simulation.next_tick(), before_tick);
    assert_eq!(simulation.state_hash(), before_hash);
    assert_eq!(
        simulation.driver_sample(ports.input_a.external_driver),
        Some(before_driver)
    );
    assert_eq!(simulation.gate_signal_state(gate), Some(before_gate));
}
