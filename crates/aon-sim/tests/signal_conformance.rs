use aon_sim::{
    Command, CommandAcceptance, CommandEnvelope, DriveStrength, DriverId, EndpointTarget, Energy,
    EntityId, FIXED_ONE, Fixed, FixedAabb, FixedVec2, GateId, GatePort, GatePortRef, GateType,
    HeatEnergy, InitialWorld, LogicLevel, NumericProfile, PhysicalScaleProfile,
    PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceWireCommand, ProfileBundle, RoutingDomain,
    SetExternalDriverCommand, Simulation, SimulationContract, SimulationPackage, StageFeatureSet,
    Tick, WireId,
};

const CIRCUIT_PITCH: i64 = 16_384;
const SUBSTRATE_HALF_EXTENT: i64 = 32 * FIXED_ONE;
const LOGIC_STRENGTH: DriveStrength = DriveStrength(100);

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

fn simulation_with_gate_delay(gate_base_delay: u64) -> Simulation {
    let mut balance = aon_sim::BalanceProfile::stage0_alpha("signal-conformance");
    balance.gate_base_delay = gate_base_delay;
    // These fixtures isolate the stated Gate delay from load-induced fan-out delay. Wire delay
    // still uses the normative Stage 0 linear and quadratic coefficients.
    balance.fanout_free_load = 1_000;
    let profiles = ProfileBundle {
        numeric: NumericProfile::reference_v1("signal-conformance"),
        physical_scale: PhysicalScaleProfile::stage0_alpha("signal-conformance"),
        balance,
    };
    let contract = SimulationContract::from_profiles(&profiles).expect("profiles are valid");
    let mut required_features = StageFeatureSet::none();
    required_features.signal = true;
    Simulation::new(SimulationPackage::new(
        "signal-conformance",
        InitialWorld::Empty,
        required_features,
        contract,
        profiles,
    ))
    .expect("the signal conformance simulation starts")
}

fn envelope(simulation: &Simulation, ordinal: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        target_tick: simulation.next_tick(),
        ordinal,
        command,
    }
}

fn expect_created(simulation: &mut Simulation, command: Command) -> EntityId {
    let target_tick = simulation.next_tick();
    let report = simulation
        .step(&[CommandEnvelope {
            target_tick,
            ordinal: 0,
            command,
        }])
        .expect("the conformance fixture placement is valid");
    assert!(report.command_rejections.is_empty());
    let created = report.command_acceptances[0]
        .created_entity
        .expect("the placement creates an entity");
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

fn place_not(simulation: &mut Simulation, domain: RoutingDomain, origin: FixedVec2) -> GateId {
    GateId(expect_created(
        simulation,
        Command::PlaceGate(PlaceGateCommand {
            gate_type: GateType::Not,
            origin,
            routing_domain: domain,
        }),
    ))
}

fn place_wire(
    simulation: &mut Simulation,
    domain: RoutingDomain,
    points: Vec<FixedVec2>,
    endpoint_a: EndpointTarget,
    endpoint_b: EndpointTarget,
) -> WireId {
    WireId(expect_created(
        simulation,
        Command::PlaceWire(PlaceWireCommand {
            routing_domain: domain,
            points,
            endpoint_a,
            endpoint_b,
        }),
    ))
}

fn set_external(simulation: &mut Simulation, driver: DriverId, level: LogicLevel) {
    let command = Command::SetExternalDriver(SetExternalDriverCommand {
        driver,
        level,
        strength: LOGIC_STRENGTH,
    });
    let report = simulation
        .step(&[envelope(simulation, 0, command)])
        .expect("a live external Driver accepts Laboratory input");
    assert!(report.command_rejections.is_empty());
    assert_eq!(report.command_acceptances.len(), 1);
}

fn step_empty(simulation: &mut Simulation) {
    simulation.step(&[]).expect("the empty Tick succeeds");
}

fn settle_not_high(simulation: &mut Simulation, gate: GateId) {
    for _ in 0..16 {
        let state = simulation
            .gate_signal_state(gate)
            .expect("the NOT Gate remains live");
        if state.current_output == LogicLevel::High
            && state.desired_output == LogicLevel::High
            && state.pending_due_tick.is_none()
        {
            return;
        }
        step_empty(simulation);
    }
    panic!("the NOT Gate did not settle High within 16 Ticks");
}

#[test]
fn c01_not_transitions_at_t1_and_eight_wu_wire_arrives_at_t4() {
    let mut simulation = simulation_with_gate_delay(1);
    let domain = place_substrate(&mut simulation);
    let source = place_not(&mut simulation, domain, point(0, 0));
    // Output at +1/4 wu faces Input A at +(8 + 1/4) wu, giving exactly 8 wu.
    let downstream_origin = point(34 * CIRCUIT_PITCH, 0);
    let downstream = place_not(&mut simulation, domain, downstream_origin);
    let wire = place_wire(
        &mut simulation,
        domain,
        vec![point(CIRCUIT_PITCH, 0), point(33 * CIRCUIT_PITCH, 0)],
        EndpointTarget::GatePort(GatePortRef {
            gate: source,
            port: GatePort::Output,
        }),
        EndpointTarget::GatePort(GatePortRef {
            gate: downstream,
            port: GatePort::InputA,
        }),
    );
    let source_ports = simulation
        .gate_signal_ports(source)
        .expect("source signal ports are observable");
    let downstream_ports = simulation
        .gate_signal_ports(downstream)
        .expect("downstream signal ports are observable");

    settle_not_high(&mut simulation, source);
    settle_not_high(&mut simulation, downstream);

    // Route a post-topology High to the downstream Sink, then restore the source input to Low.
    // This establishes the C-01 LOW-input/High-output baseline without topology-sync behavior.
    set_external(
        &mut simulation,
        source_ports.input_a.external_driver,
        LogicLevel::High,
    );
    for _ in 0..4 {
        step_empty(&mut simulation);
    }
    set_external(
        &mut simulation,
        source_ports.input_a.external_driver,
        LogicLevel::Low,
    );
    for _ in 0..4 {
        step_empty(&mut simulation);
    }
    assert_eq!(
        simulation.driver_sample(source_ports.output).unwrap().level,
        LogicLevel::High
    );
    assert_eq!(
        simulation.sink_level(downstream_ports.input_a.sink),
        Some(LogicLevel::High)
    );

    let relative_t0 = simulation.next_tick();
    set_external(
        &mut simulation,
        source_ports.input_a.external_driver,
        LogicLevel::High,
    );
    assert_eq!(simulation.next_tick(), Tick(relative_t0.0 + 1));
    assert_eq!(
        simulation.driver_sample(source_ports.output).unwrap().level,
        LogicLevel::High,
        "the NOT transition is not early at relative t=0"
    );
    assert_eq!(
        simulation.sink_level(downstream_ports.input_a.sink),
        Some(LogicLevel::High)
    );

    step_empty(&mut simulation); // relative t=1
    assert_eq!(
        simulation.driver_sample(source_ports.output).unwrap().level,
        LogicLevel::Low,
        "the NOT internal transition occurs at relative t=1"
    );
    assert_eq!(
        simulation.sink_level(downstream_ports.input_a.sink),
        Some(LogicLevel::High),
        "the physical arrival must not bypass Wire delay"
    );
    assert_eq!(
        simulation.wire_signal_state(wire).unwrap().active.low,
        400,
        "Wire excitation observes the nominal-strength source output"
    );

    for relative_tick in 2..=3 {
        step_empty(&mut simulation);
        assert_eq!(
            simulation.sink_level(downstream_ports.input_a.sink),
            Some(LogicLevel::High),
            "downstream changed early at relative t={relative_tick}"
        );
    }
    step_empty(&mut simulation); // relative t=4
    assert_eq!(
        simulation.sink_level(downstream_ports.input_a.sink),
        Some(LogicLevel::Low),
        "8 wu has Wire delay 3, so a t=1 transition arrives at relative t=4"
    );
}

#[test]
fn c02_two_tick_pulse_is_filtered_and_reserved_energy_becomes_heat() {
    let mut simulation = simulation_with_gate_delay(3);
    let domain = place_substrate(&mut simulation);
    let gate = place_not(&mut simulation, domain, point(0, 0));
    let ports = simulation
        .gate_signal_ports(gate)
        .expect("NOT signal ports are observable");
    settle_not_high(&mut simulation, gate);

    let relative_t0 = simulation.next_tick();
    set_external(
        &mut simulation,
        ports.input_a.external_driver,
        LogicLevel::High,
    );
    let pending = simulation.gate_signal_state(gate).unwrap();
    assert_eq!(pending.current_output, LogicLevel::High);
    assert_eq!(pending.desired_output, LogicLevel::Low);
    assert_eq!(pending.pending_due_tick, Some(Tick(relative_t0.0 + 3)));
    assert_eq!(pending.pending_level, Some(LogicLevel::Low));
    assert_eq!(pending.pending_switch_energy, Some(Energy(1)));

    step_empty(&mut simulation); // relative t=1: the two-Tick High pulse remains active
    assert_eq!(
        simulation.driver_sample(ports.output).unwrap().level,
        LogicLevel::High
    );

    set_external(
        &mut simulation,
        ports.input_a.external_driver,
        LogicLevel::Low,
    ); // relative t=2: pulse ends before the delay-three transition is due
    let cancelled = simulation.gate_signal_state(gate).unwrap();
    assert_eq!(cancelled.current_output, LogicLevel::High);
    assert_eq!(cancelled.desired_output, LogicLevel::High);
    assert_eq!(cancelled.pending_due_tick, None);
    assert_eq!(cancelled.pending_level, None);
    assert_eq!(cancelled.pending_switch_energy, None);
    assert_eq!(cancelled.cancelled_switching_heat, HeatEnergy(1));

    let stale = simulation
        .step(&[])
        .expect("the cancelled transition is drained harmlessly at relative t=3");
    assert_eq!(stale.signal_counters.stale_driver_transitions, 1);
    assert_eq!(
        simulation.driver_sample(ports.output).unwrap().level,
        LogicLevel::High,
        "the filtered input pulse must not create an output pulse"
    );
    assert_eq!(
        simulation
            .gate_signal_state(gate)
            .unwrap()
            .cancelled_switching_heat,
        HeatEnergy(1),
        "exactly the reserved switch energy becomes canceled heat"
    );
}

#[test]
fn pending_transition_replacement_advances_generation_and_stales_the_old_event() {
    let mut simulation = simulation_with_gate_delay(3);
    let domain = place_substrate(&mut simulation);
    let gate = place_not(&mut simulation, domain, point(0, 0));
    let ports = simulation
        .gate_signal_ports(gate)
        .expect("NOT signal ports are observable");
    settle_not_high(&mut simulation, gate);

    let baseline = simulation
        .gate_signal_state(gate)
        .expect("the settled Gate remains live");
    assert_eq!(baseline.current_output, LogicLevel::High);
    assert_eq!(baseline.desired_output, LogicLevel::High);
    assert_eq!(baseline.pending_due_tick, None);

    let relative_t0 = simulation.next_tick();
    set_external(
        &mut simulation,
        ports.input_a.external_driver,
        LogicLevel::High,
    );
    let first = simulation
        .gate_signal_state(gate)
        .expect("the first pending transition is observable");
    assert_eq!(first.current_output, LogicLevel::High);
    assert_eq!(first.desired_output, LogicLevel::Low);
    assert_eq!(first.pending_generation, baseline.pending_generation + 1);
    assert_eq!(first.pending_due_tick, Some(Tick(relative_t0.0 + 3)));
    assert_eq!(first.pending_level, Some(LogicLevel::Low));
    assert_eq!(first.pending_switch_energy, Some(Energy(1)));
    assert_eq!(first.cancelled_switching_heat, HeatEnergy(0));

    set_external(
        &mut simulation,
        ports.input_a.external_driver,
        LogicLevel::X,
    );
    let replacement = simulation
        .gate_signal_state(gate)
        .expect("the replacement transition is observable");
    assert_eq!(replacement.current_output, LogicLevel::High);
    assert_eq!(replacement.desired_output, LogicLevel::X);
    assert_eq!(replacement.pending_generation, first.pending_generation + 1);
    assert_eq!(replacement.pending_due_tick, Some(Tick(relative_t0.0 + 4)));
    assert_eq!(replacement.pending_level, Some(LogicLevel::X));
    assert_eq!(replacement.pending_switch_energy, Some(Energy(1)));
    assert_eq!(replacement.cancelled_switching_heat, HeatEnergy(1));

    step_empty(&mut simulation); // relative t=2: neither event is due
    let stale = simulation
        .step(&[])
        .expect("the replaced transition drains harmlessly at relative t=3");
    assert_eq!(stale.completed_tick, Tick(relative_t0.0 + 3));
    assert_eq!(stale.signal_counters.stale_driver_transitions, 1);
    assert_eq!(
        simulation.driver_sample(ports.output).unwrap().level,
        LogicLevel::High,
        "the replaced Low transition must not become observable"
    );
    assert_eq!(
        simulation
            .gate_signal_state(gate)
            .unwrap()
            .pending_generation,
        replacement.pending_generation,
        "draining the stale event must not alter the replacement token"
    );

    let applied = simulation
        .step(&[])
        .expect("the replacement transition applies at relative t=4");
    assert_eq!(applied.completed_tick, Tick(relative_t0.0 + 4));
    assert_eq!(applied.signal_counters.stale_driver_transitions, 0);
    assert_eq!(applied.signal_counters.driver_transitions_applied, 1);
    assert_eq!(
        simulation.driver_sample(ports.output).unwrap().level,
        LogicLevel::X
    );
    let settled = simulation.gate_signal_state(gate).unwrap();
    assert_eq!(settled.current_output, LogicLevel::X);
    assert_eq!(settled.desired_output, LogicLevel::X);
    assert_eq!(settled.pending_due_tick, None);
    assert_eq!(settled.pending_level, None);
    assert_eq!(settled.pending_switch_energy, None);
    assert_eq!(settled.cancelled_switching_heat, HeatEnergy(1));
}

#[test]
fn c03_one_tick_pulse_survives_twelve_wu_transport_after_exactly_five_ticks() {
    let mut simulation = simulation_with_gate_delay(1);
    let domain = place_substrate(&mut simulation);
    let source = place_not(&mut simulation, domain, point(0, 0));
    let downstream_origin = point(0, 46 * CIRCUIT_PITCH);
    let downstream = place_not(&mut simulation, domain, downstream_origin);

    // Exit both left-facing Input A ports through the exterior. The three segments total
    // 1/4 + 11 1/2 + 1/4 = 12 wu, whose exact Stage 0 Wire delay is ceil(4.8) = 5.
    let source_input = point(-CIRCUIT_PITCH, 0);
    let outer_source = point(-2 * CIRCUIT_PITCH, 0);
    let outer_downstream = point(-2 * CIRCUIT_PITCH, 46 * CIRCUIT_PITCH);
    let downstream_input = point(-CIRCUIT_PITCH, 46 * CIRCUIT_PITCH);
    let wire = place_wire(
        &mut simulation,
        domain,
        vec![
            source_input,
            outer_source,
            outer_downstream,
            downstream_input,
        ],
        EndpointTarget::GatePort(GatePortRef {
            gate: source,
            port: GatePort::InputA,
        }),
        EndpointTarget::GatePort(GatePortRef {
            gate: downstream,
            port: GatePort::InputA,
        }),
    );
    let source_ports = simulation.gate_signal_ports(source).unwrap();
    let downstream_ports = simulation.gate_signal_ports(downstream).unwrap();
    settle_not_high(&mut simulation, source);
    settle_not_high(&mut simulation, downstream);
    assert_eq!(
        simulation.sink_level(downstream_ports.input_a.sink),
        Some(LogicLevel::Low)
    );

    let relative_t0 = simulation.next_tick();
    set_external(
        &mut simulation,
        source_ports.input_a.external_driver,
        LogicLevel::High,
    );
    assert_eq!(simulation.next_tick(), Tick(relative_t0.0 + 1));
    assert_eq!(
        simulation.sink_level(downstream_ports.input_a.sink),
        Some(LogicLevel::Low)
    );
    assert_eq!(
        simulation.wire_signal_state(wire).unwrap().active.high,
        u128::from(LOGIC_STRENGTH.0)
    );

    set_external(
        &mut simulation,
        source_ports.input_a.external_driver,
        LogicLevel::Low,
    ); // relative t=1 ends the source pulse
    assert_eq!(
        simulation.sink_level(downstream_ports.input_a.sink),
        Some(LogicLevel::Low)
    );

    for relative_tick in 2..=4 {
        step_empty(&mut simulation);
        assert_eq!(
            simulation.sink_level(downstream_ports.input_a.sink),
            Some(LogicLevel::Low),
            "transport pulse arrived early at relative t={relative_tick}"
        );
    }
    step_empty(&mut simulation); // relative t=5
    assert_eq!(
        simulation.sink_level(downstream_ports.input_a.sink),
        Some(LogicLevel::High),
        "the pulse leading edge arrives exactly five Ticks later"
    );
    step_empty(&mut simulation); // relative t=6
    assert_eq!(
        simulation.sink_level(downstream_ports.input_a.sink),
        Some(LogicLevel::Low),
        "the trailing edge remains one Tick behind the leading edge"
    );
}

#[test]
fn removing_a_source_resolves_downstream_once_and_reports_the_actual_dirty_count() {
    let mut simulation = simulation_with_gate_delay(1);
    let domain = place_substrate(&mut simulation);
    let source = place_not(&mut simulation, domain, point(0, 0));
    let downstream = place_not(&mut simulation, domain, point(8 * CIRCUIT_PITCH, 0));
    place_wire(
        &mut simulation,
        domain,
        vec![point(CIRCUIT_PITCH, 0), point(7 * CIRCUIT_PITCH, 0)],
        EndpointTarget::GatePort(GatePortRef {
            gate: source,
            port: GatePort::Output,
        }),
        EndpointTarget::GatePort(GatePortRef {
            gate: downstream,
            port: GatePort::InputA,
        }),
    );
    let source_ports = simulation.gate_signal_ports(source).unwrap();
    let downstream_ports = simulation.gate_signal_ports(downstream).unwrap();
    settle_not_high(&mut simulation, source);
    settle_not_high(&mut simulation, downstream);

    set_external(
        &mut simulation,
        source_ports.input_a.external_driver,
        LogicLevel::High,
    );
    for _ in 0..3 {
        step_empty(&mut simulation);
    }
    set_external(
        &mut simulation,
        source_ports.input_a.external_driver,
        LogicLevel::Low,
    );
    for _ in 0..4 {
        step_empty(&mut simulation);
    }
    assert_eq!(
        simulation.sink_level(downstream_ports.input_a.sink),
        Some(LogicLevel::High)
    );

    let report = simulation
        .step(&[envelope(
            &simulation,
            0,
            Command::RemoveEntity(aon_sim::RemoveEntityCommand {
                target: source.entity_id(),
            }),
        )])
        .expect("source removal and downstream passive resolution succeed");
    assert_eq!(
        simulation.sink_level(downstream_ports.input_a.sink),
        Some(LogicLevel::Low)
    );
    assert_eq!(report.signal_counters.sinks_resolved, 1);
    assert_eq!(report.signal_changes.len(), 1);
    assert_eq!(report.signal_changes[0].sink, downstream_ports.input_a.sink);
    assert_eq!(report.signal_changes[0].current, LogicLevel::Low);
}

#[test]
fn unchanged_external_sample_is_accepted_without_events_or_arrivals() {
    let mut simulation = simulation_with_gate_delay(1);
    let domain = place_substrate(&mut simulation);
    let gate = place_not(&mut simulation, domain, point(0, 0));
    let ports = simulation
        .gate_signal_ports(gate)
        .expect("Gate ports exist");
    settle_not_high(&mut simulation, gate);
    let before = simulation
        .driver_sample(ports.input_a.external_driver)
        .expect("external Driver exists");
    assert_eq!(before.level, LogicLevel::Low);
    assert_eq!(before.strength, DriveStrength(0));

    let report = simulation
        .step(&[envelope(
            &simulation,
            0,
            Command::SetExternalDriver(SetExternalDriverCommand {
                driver: ports.input_a.external_driver,
                level: before.level,
                strength: before.strength,
            }),
        )])
        .expect("same-value external command is an accepted no-op");
    assert_eq!(report.command_acceptances.len(), 1);
    assert!(report.command_rejections.is_empty());
    assert!(report.driver_changes.is_empty());
    assert!(report.signal_changes.is_empty());
    assert_eq!(report.signal_counters.driver_transitions_applied, 0);
    assert_eq!(report.signal_counters.signal_arrivals_applied, 0);
    assert_eq!(
        simulation.driver_sample(ports.input_a.external_driver),
        Some(before)
    );
}
