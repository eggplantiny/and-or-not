use aon_sim::{
    BalanceProfile, Command, CommandEnvelope, DriveStrength, DriverId, EndpointTarget, EntityId,
    FIXED_ONE, Fixed, FixedAabb, FixedVec2, GateId, GatePort, GatePortRef, GateType, InitialWorld,
    LogicLevel, NumericProfile, PhysicalScaleProfile, PlaceFixedSubstrateCommand, PlaceGateCommand,
    PlaceWireCommand, ProfileBundle, RoutingDomain, SetExternalDriverCommand, Simulation,
    SimulationContract, SimulationPackage, StageFeatureSet, StepReport, Tick, polyline_length,
};

const P: i64 = aon_sim::REFERENCE_CIRCUIT_ROUTING_PITCH.0;
const SUBSTRATE_HALF_EXTENT: i64 = 32 * FIXED_ONE;
const EXTERNAL_HIGH_STRENGTH: DriveStrength = DriveStrength(100);
const RELEASED_STRENGTH: DriveStrength = DriveStrength(0);

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

const fn pitch(x: i64, y: i64) -> FixedVec2 {
    point(x * P, y * P)
}

fn simulation() -> Simulation {
    let profiles = ProfileBundle {
        numeric: NumericProfile::reference_v1("feedback-conformance"),
        physical_scale: PhysicalScaleProfile::stage0_alpha("feedback-conformance"),
        balance: BalanceProfile::stage0_alpha("feedback-conformance"),
    };
    let contract = SimulationContract::from_profiles(&profiles).expect("profiles are valid");
    let mut required_features = StageFeatureSet::none();
    required_features.signal = true;
    Simulation::new(SimulationPackage::new(
        "feedback-conformance",
        InitialWorld::Empty,
        required_features,
        contract,
        profiles,
    ))
    .expect("the feedback conformance simulation starts")
}

fn step_commands(
    simulation: &mut Simulation,
    mut commands: Vec<(u64, Command)>,
    reverse_input_slice_order: bool,
) -> StepReport {
    // The host may build an equivalent command slice in a different insertion order. Ordinals,
    // rather than Vec layout, define the logical command batch.
    if reverse_input_slice_order {
        commands.reverse();
    }
    let target_tick = simulation.next_tick();
    let envelopes = commands
        .into_iter()
        .map(|(ordinal, command)| CommandEnvelope {
            target_tick,
            ordinal,
            command,
        })
        .collect::<Vec<_>>();
    let report = simulation
        .step(&envelopes)
        .expect("the feedback fixture Tick succeeds");
    assert!(
        report.command_rejections.is_empty(),
        "feedback fixture geometry and commands must be accepted: {:?}",
        report.command_rejections
    );
    report
}

fn step_empty(simulation: &mut Simulation) -> StepReport {
    step_commands(simulation, Vec::new(), false)
}

fn created_at(report: &StepReport, ordinal: u64) -> EntityId {
    report
        .command_acceptances
        .iter()
        .find(|acceptance| acceptance.ordinal == ordinal)
        .and_then(|acceptance| acceptance.created_entity)
        .expect("the placement command creates an entity")
}

fn place_substrate(simulation: &mut Simulation) -> RoutingDomain {
    let bounds = FixedAabb::new(
        point(-SUBSTRATE_HALF_EXTENT, -SUBSTRATE_HALF_EXTENT),
        point(SUBSTRATE_HALF_EXTENT, SUBSTRATE_HALF_EXTENT),
    );
    let report = step_commands(
        simulation,
        vec![(
            0,
            Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                origin: point(0, 0),
                routing_area: bounds,
                footprint: bounds,
            }),
        )],
        false,
    );
    assert_eq!(report.completed_tick, Tick(0));
    RoutingDomain::FixedSubstrate(created_at(&report, 0))
}

fn gate_command(gate_type: GateType, origin: FixedVec2, routing_domain: RoutingDomain) -> Command {
    Command::PlaceGate(PlaceGateCommand {
        gate_type,
        origin,
        routing_domain,
    })
}

fn wire_command(
    routing_domain: RoutingDomain,
    points: Vec<FixedVec2>,
    gate_a: GateId,
    port_a: GatePort,
    gate_b: GateId,
    port_b: GatePort,
) -> Command {
    Command::PlaceWire(PlaceWireCommand {
        routing_domain,
        points,
        endpoint_a: EndpointTarget::GatePort(GatePortRef {
            gate: gate_a,
            port: port_a,
        }),
        endpoint_b: EndpointTarget::GatePort(GatePortRef {
            gate: gate_b,
            port: port_b,
        }),
    })
}

fn set_external_command(driver: DriverId, level: LogicLevel, strength: DriveStrength) -> Command {
    Command::SetExternalDriver(SetExternalDriverCommand {
        driver,
        level,
        strength,
    })
}

fn record_output_edges(report: &StepReport, output: DriverId, edges: &mut Vec<(u64, LogicLevel)>) {
    assert_eq!(
        report.signal_counters.stale_driver_transitions, 0,
        "the fixture must not lose an oscillator transition"
    );
    edges.extend(
        report
            .driver_changes
            .iter()
            .filter(|change| change.driver == output)
            .map(|change| (report.completed_tick.0, change.current.level)),
    );
}

fn assert_quiescent_gate(simulation: &Simulation, gate: GateId, expected: LogicLevel) {
    let state = simulation
        .gate_signal_state(gate)
        .expect("the fixture Gate remains live");
    assert_eq!(state.current_output, expected);
    assert_eq!(state.desired_output, expected);
    assert_eq!(state.pending_due_tick, None);
    assert_eq!(state.pending_level, None);
    assert_eq!(
        simulation
            .driver_sample(state.ports.output)
            .expect("the Gate output remains observable")
            .level,
        expected
    );
}

#[test]
fn c05_one_not_ring_has_exact_two_d_period_without_deleted_edges() {
    let mut simulation = simulation();
    let domain = place_substrate(&mut simulation);

    let gate_report = step_commands(
        &mut simulation,
        vec![(0, gate_command(GateType::Not, pitch(0, 0), domain))],
        false,
    );
    assert_eq!(gate_report.completed_tick, Tick(1));
    let gate = GateId(created_at(&gate_report, 0));
    let ports = simulation
        .gate_signal_ports(gate)
        .expect("the NOT Gate exposes its signal ports");
    let startup = simulation
        .gate_signal_state(gate)
        .expect("the NOT Gate startup state is observable");
    assert_eq!(startup.current_output, LogicLevel::Low);
    assert_eq!(startup.desired_output, LogicLevel::High);
    assert_eq!(startup.pending_due_tick, Some(Tick(2)));
    assert_eq!(startup.pending_level, Some(LogicLevel::High));
    let initial_output = simulation
        .driver_sample(ports.output)
        .expect("the NOT output exists");
    assert_eq!(initial_output.level, LogicLevel::Low);
    assert_eq!(initial_output.strength, DriveStrength(0));
    assert_eq!(initial_output.revision.0, 0);

    let route = vec![
        pitch(1, 0),
        pitch(2, 0),
        pitch(2, 2),
        pitch(-2, 2),
        pitch(-2, 0),
        pitch(-1, 0),
    ];
    assert_eq!(polyline_length(&route), Ok(Fixed(10 * P)));
    let wire_report = step_commands(
        &mut simulation,
        vec![(
            0,
            wire_command(
                domain,
                route,
                gate,
                GatePort::Output,
                gate,
                GatePort::InputA,
            ),
        )],
        false,
    );
    assert_eq!(wire_report.completed_tick, Tick(2));
    assert_eq!(wire_report.command_acceptances.len(), 1);

    // L=2.5 wu gives Wire delay 1. Load=(one Sink + ceil(2.5))=4 gives Gate delay 1,
    // hence one loop edge has D=2 and a same-polarity period of 2D=4.
    let mut edges = Vec::new();
    record_output_edges(&wire_report, ports.output, &mut edges);
    while simulation.next_tick().0 <= 20 {
        let report = step_empty(&mut simulation);
        record_output_edges(&report, ports.output, &mut edges);
    }

    assert_eq!(
        edges,
        vec![
            (2, LogicLevel::High),
            (4, LogicLevel::Low),
            (6, LogicLevel::High),
            (8, LogicLevel::Low),
            (10, LogicLevel::High),
            (12, LogicLevel::Low),
            (14, LogicLevel::High),
            (16, LogicLevel::Low),
            (18, LogicLevel::High),
            (20, LogicLevel::Low),
        ]
    );
    for level in [LogicLevel::High, LogicLevel::Low] {
        let phase_ticks = edges
            .iter()
            .filter_map(|(tick, edge_level)| (*edge_level == level).then_some(*tick))
            .collect::<Vec<_>>();
        assert!(
            phase_ticks.windows(2).all(|ticks| ticks[1] - ticks[0] == 4),
            "each same-polarity period is exactly 2D"
        );
    }
}

fn assert_not_branches_exchange_equivalent(
    simulation: &Simulation,
    branch_a: GateId,
    branch_b: GateId,
) {
    let state_a = simulation
        .gate_signal_state(branch_a)
        .expect("branch A remains live");
    let state_b = simulation
        .gate_signal_state(branch_b)
        .expect("branch B remains live");
    assert_eq!(state_a.current_output, state_b.current_output);
    assert_eq!(state_a.desired_output, state_b.desired_output);
    assert_eq!(state_a.pending_generation, state_b.pending_generation);
    assert_eq!(state_a.pending_due_tick, state_b.pending_due_tick);
    assert_eq!(state_a.pending_level, state_b.pending_level);
    assert_eq!(state_a.pending_switch_energy, state_b.pending_switch_energy);
    assert_eq!(
        state_a.cancelled_switching_heat,
        state_b.cancelled_switching_heat
    );

    let output_a = simulation
        .driver_sample(state_a.ports.output)
        .expect("branch A output remains live");
    let output_b = simulation
        .driver_sample(state_b.ports.output)
        .expect("branch B output remains live");
    assert_eq!(output_a.level, output_b.level);
    assert_eq!(output_a.strength, output_b.strength);
    assert_eq!(output_a.revision, output_b.revision);
    assert_eq!(output_a.emitted_at, output_b.emitted_at);
    assert_eq!(
        simulation.sink_level(state_a.ports.input_a.sink),
        simulation.sink_level(state_b.ports.input_a.sink)
    );
}

#[test]
fn c06_symmetric_startup_is_independent_of_input_slice_order() {
    let mut declared_order = simulation();
    let mut reversed_order = simulation();
    let domain = place_substrate(&mut declared_order);
    let reversed_domain = place_substrate(&mut reversed_order);
    assert_eq!(domain, reversed_domain);
    assert_eq!(declared_order.state_hash(), reversed_order.state_hash());

    let gate_commands = vec![
        (0, gate_command(GateType::Not, pitch(0, 0), domain)),
        (1, gate_command(GateType::Not, pitch(8, 0), domain)),
    ];
    let declared_gates = step_commands(&mut declared_order, gate_commands.clone(), false);
    let reversed_gates = step_commands(&mut reversed_order, gate_commands, true);
    assert_eq!(declared_gates, reversed_gates);
    assert_eq!(declared_order.state_hash(), reversed_order.state_hash());
    let branch_a = GateId(created_at(&declared_gates, 0));
    let branch_b = GateId(created_at(&declared_gates, 1));
    assert_ne!(branch_a, branch_b);
    assert_not_branches_exchange_equivalent(&declared_order, branch_a, branch_b);
    assert_not_branches_exchange_equivalent(&reversed_order, branch_a, branch_b);

    let a_to_b = vec![
        pitch(1, 0),
        pitch(2, 0),
        pitch(2, 6),
        pitch(6, 6),
        pitch(6, 0),
        pitch(7, 0),
    ];
    let b_to_a = vec![
        pitch(9, 0),
        pitch(10, 0),
        pitch(10, -2),
        pitch(-2, -2),
        pitch(-2, 0),
        pitch(-1, 0),
    ];
    assert_eq!(polyline_length(&a_to_b), Ok(Fixed(18 * P)));
    assert_eq!(polyline_length(&b_to_a), Ok(Fixed(18 * P)));
    let wire_commands = vec![
        (
            0,
            wire_command(
                domain,
                a_to_b,
                branch_a,
                GatePort::Output,
                branch_b,
                GatePort::InputA,
            ),
        ),
        (
            1,
            wire_command(
                domain,
                b_to_a,
                branch_b,
                GatePort::Output,
                branch_a,
                GatePort::InputA,
            ),
        ),
    ];
    let declared_wires = step_commands(&mut declared_order, wire_commands.clone(), false);
    let reversed_wires = step_commands(&mut reversed_order, wire_commands, true);
    assert_eq!(declared_wires, reversed_wires);
    assert_eq!(declared_order.state_hash(), reversed_order.state_hash());
    assert_not_branches_exchange_equivalent(&declared_order, branch_a, branch_b);
    assert_not_branches_exchange_equivalent(&reversed_order, branch_a, branch_b);

    let ports_a = declared_order
        .gate_signal_ports(branch_a)
        .expect("branch A ports remain observable");
    let ports_b = declared_order
        .gate_signal_ports(branch_b)
        .expect("branch B ports remain observable");
    let mut edges_a = Vec::new();
    let mut edges_b = Vec::new();
    record_output_edges(&declared_wires, ports_a.output, &mut edges_a);
    record_output_edges(&declared_wires, ports_b.output, &mut edges_b);

    while declared_order.next_tick().0 <= 20 {
        let declared_report = step_empty(&mut declared_order);
        let reversed_report = step_empty(&mut reversed_order);
        assert_eq!(declared_report, reversed_report);
        assert_eq!(declared_order.state_hash(), reversed_order.state_hash());
        assert_not_branches_exchange_equivalent(&declared_order, branch_a, branch_b);
        assert_not_branches_exchange_equivalent(&reversed_order, branch_a, branch_b);
        record_output_edges(&declared_report, ports_a.output, &mut edges_a);
        record_output_edges(&declared_report, ports_b.output, &mut edges_b);
    }

    let expected = vec![
        (2, LogicLevel::High),
        (5, LogicLevel::Low),
        (8, LogicLevel::High),
        (11, LogicLevel::Low),
        (14, LogicLevel::High),
        (17, LogicLevel::Low),
        (20, LogicLevel::High),
    ];
    assert_eq!(edges_a, expected);
    assert_eq!(edges_b, expected);
    assert_eq!(declared_order.state_hash(), reversed_order.state_hash());
}

#[test]
fn explicit_nor_style_set_reset_emerges_from_only_or_not_and_wire() {
    let mut simulation = simulation();
    let domain = place_substrate(&mut simulation);

    // Q = NOT(R OR Qbar), Qbar = NOT(S OR Q). All four Gates share the placement Tick.
    let gate_report = step_commands(
        &mut simulation,
        vec![
            (0, gate_command(GateType::Or, pitch(0, -4), domain)),
            (1, gate_command(GateType::Not, pitch(4, -4), domain)),
            (2, gate_command(GateType::Or, pitch(0, 4), domain)),
            (3, gate_command(GateType::Not, pitch(4, 4), domain)),
        ],
        false,
    );
    assert_eq!(gate_report.completed_tick, Tick(1));
    assert_eq!(gate_report.command_acceptances.len(), 4);
    let or_q = GateId(created_at(&gate_report, 0));
    let q = GateId(created_at(&gate_report, 1));
    let or_qbar = GateId(created_at(&gate_report, 2));
    let qbar = GateId(created_at(&gate_report, 3));
    let or_q_ports = simulation
        .gate_signal_ports(or_q)
        .expect("the Q-side OR ports exist");
    let q_ports = simulation
        .gate_signal_ports(q)
        .expect("the Q NOT ports exist");
    let or_qbar_ports = simulation
        .gate_signal_ports(or_qbar)
        .expect("the Qbar-side OR ports exist");
    let qbar_ports = simulation
        .gate_signal_ports(qbar)
        .expect("the Qbar NOT ports exist");
    let reset_driver = or_q_ports.input_a.external_driver;
    let set_driver = or_qbar_ports
        .input_b
        .expect("OR has Input B")
        .external_driver;

    let or_q_to_q = vec![pitch(1, -4), pitch(3, -4)];
    let or_qbar_to_qbar = vec![pitch(1, 4), pitch(3, 4)];
    let q_to_or_qbar = vec![
        pitch(5, -4),
        pitch(6, -4),
        pitch(6, 7),
        pitch(-2, 7),
        pitch(-2, 4),
        point(-P, 7 * P / 2),
    ];
    let qbar_to_or_q = vec![
        pitch(5, 4),
        pitch(7, 4),
        pitch(7, -6),
        pitch(-2, -6),
        pitch(-2, -4),
        point(-P, -7 * P / 2),
    ];
    assert_eq!(polyline_length(&or_q_to_q), Ok(Fixed(2 * P)));
    assert_eq!(polyline_length(&or_qbar_to_qbar), Ok(Fixed(2 * P)));
    assert_eq!(polyline_length(&q_to_or_qbar), Ok(Fixed(395_150)));
    assert_eq!(polyline_length(&qbar_to_or_q), Ok(Fixed(395_150)));

    // The two feedback routes deliberately cross at one nonconnecting point; no Junction or
    // special Latch primitive is present. Set is asserted in the same Tick as the four Wires.
    let construction = step_commands(
        &mut simulation,
        vec![
            (
                0,
                wire_command(
                    domain,
                    or_q_to_q,
                    or_q,
                    GatePort::Output,
                    q,
                    GatePort::InputA,
                ),
            ),
            (
                1,
                wire_command(
                    domain,
                    or_qbar_to_qbar,
                    or_qbar,
                    GatePort::Output,
                    qbar,
                    GatePort::InputA,
                ),
            ),
            (
                2,
                wire_command(
                    domain,
                    q_to_or_qbar,
                    q,
                    GatePort::Output,
                    or_qbar,
                    GatePort::InputA,
                ),
            ),
            (
                3,
                wire_command(
                    domain,
                    qbar_to_or_q,
                    qbar,
                    GatePort::Output,
                    or_q,
                    GatePort::InputB,
                ),
            ),
            (
                4,
                set_external_command(set_driver, LogicLevel::High, EXTERNAL_HIGH_STRENGTH),
            ),
        ],
        false,
    );
    assert_eq!(construction.completed_tick, Tick(2));
    assert_eq!(construction.command_acceptances.len(), 5);

    let mut q_edges = Vec::new();
    let mut qbar_edges = Vec::new();
    record_output_edges(&construction, q_ports.output, &mut q_edges);
    record_output_edges(&construction, qbar_ports.output, &mut qbar_edges);
    while simulation.next_tick().0 <= 14 {
        let report = step_empty(&mut simulation);
        record_output_edges(&report, q_ports.output, &mut q_edges);
        record_output_edges(&report, qbar_ports.output, &mut qbar_edges);
    }
    assert_quiescent_gate(&simulation, q, LogicLevel::High);
    assert_quiescent_gate(&simulation, qbar, LogicLevel::Low);

    let release_set = step_commands(
        &mut simulation,
        vec![(
            0,
            set_external_command(set_driver, LogicLevel::Low, RELEASED_STRENGTH),
        )],
        false,
    );
    assert_eq!(release_set.completed_tick, Tick(15));
    record_output_edges(&release_set, q_ports.output, &mut q_edges);
    record_output_edges(&release_set, qbar_ports.output, &mut qbar_edges);
    assert!(
        release_set
            .driver_changes
            .iter()
            .all(|change| change.driver != q_ports.output && change.driver != qbar_ports.output)
    );
    assert_quiescent_gate(&simulation, q, LogicLevel::High);
    assert_quiescent_gate(&simulation, qbar, LogicLevel::Low);
    for _ in 16..=19 {
        let report = step_empty(&mut simulation);
        record_output_edges(&report, q_ports.output, &mut q_edges);
        record_output_edges(&report, qbar_ports.output, &mut qbar_edges);
        assert!(report
            .driver_changes
            .iter()
            .all(|change| change.driver != q_ports.output && change.driver != qbar_ports.output));
        assert_quiescent_gate(&simulation, q, LogicLevel::High);
        assert_quiescent_gate(&simulation, qbar, LogicLevel::Low);
    }

    let reset = step_commands(
        &mut simulation,
        vec![(
            0,
            set_external_command(reset_driver, LogicLevel::High, EXTERNAL_HIGH_STRENGTH),
        )],
        false,
    );
    assert_eq!(reset.completed_tick, Tick(20));
    record_output_edges(&reset, q_ports.output, &mut q_edges);
    record_output_edges(&reset, qbar_ports.output, &mut qbar_edges);
    while simulation.next_tick().0 <= 32 {
        let report = step_empty(&mut simulation);
        record_output_edges(&report, q_ports.output, &mut q_edges);
        record_output_edges(&report, qbar_ports.output, &mut qbar_edges);
    }
    assert_quiescent_gate(&simulation, q, LogicLevel::Low);
    assert_quiescent_gate(&simulation, qbar, LogicLevel::High);

    let release_reset = step_commands(
        &mut simulation,
        vec![(
            0,
            set_external_command(reset_driver, LogicLevel::Low, RELEASED_STRENGTH),
        )],
        false,
    );
    assert_eq!(release_reset.completed_tick, Tick(33));
    record_output_edges(&release_reset, q_ports.output, &mut q_edges);
    record_output_edges(&release_reset, qbar_ports.output, &mut qbar_edges);
    assert!(
        release_reset
            .driver_changes
            .iter()
            .all(|change| change.driver != q_ports.output && change.driver != qbar_ports.output)
    );
    assert_quiescent_gate(&simulation, q, LogicLevel::Low);
    assert_quiescent_gate(&simulation, qbar, LogicLevel::High);
    for _ in 34..=45 {
        let report = step_empty(&mut simulation);
        record_output_edges(&report, q_ports.output, &mut q_edges);
        record_output_edges(&report, qbar_ports.output, &mut qbar_edges);
        assert!(report
            .driver_changes
            .iter()
            .all(|change| change.driver != q_ports.output && change.driver != qbar_ports.output));
        assert_quiescent_gate(&simulation, q, LogicLevel::Low);
        assert_quiescent_gate(&simulation, qbar, LogicLevel::High);
    }

    assert_eq!(
        q_edges,
        vec![
            (2, LogicLevel::High),
            (8, LogicLevel::Low),
            (12, LogicLevel::High),
            (24, LogicLevel::Low),
        ]
    );
    assert_eq!(
        qbar_edges,
        vec![
            (2, LogicLevel::High),
            (6, LogicLevel::Low),
            (30, LogicLevel::High),
        ]
    );
}
