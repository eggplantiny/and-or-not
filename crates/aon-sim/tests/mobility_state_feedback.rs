use aon_sim::{
    ArtifactBytes, Command, CommandEnvelope, DriveStrength, DriverId, EndpointTarget, EntityId,
    Fixed, FixedAabb, FixedVec2, GateId, GatePort, GatePortRef, GateType, Heading,
    JunctionDecisionKind, JunctionId, LogicLevel, MobileId, MobilePort, MobilePortRef,
    PlaceGateCommand, PlaceJunctionCommand, PlaceMobileSubstrateCommand, PlaceWireCommand,
    RenderSnapshot, RoutingDomain, SetExternalDriverCommand, Simulation, SimulationPackage,
    StepReport, TrackPosition, WireId, decode_package,
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
const JUNCTION_X: i64 = 32 * WORLD_PITCH;
const B_X: i64 = 64 * WORLD_PITCH;
const ASSERTED_STRENGTH: DriveStrength = DriveStrength(100);
const RELEASED_STRENGTH: DriveStrength = DriveStrength(0);

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

const fn circuit_point(x: i64, y: i64) -> FixedVec2 {
    point(x * CIRCUIT_PITCH, y * CIRCUIT_PITCH)
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

fn step_commands(simulation: &mut Simulation, commands: Vec<Command>) -> StepReport {
    let target_tick = simulation.next_tick();
    let envelopes = commands
        .into_iter()
        .enumerate()
        .map(|(ordinal, command)| CommandEnvelope {
            target_tick,
            ordinal: ordinal as u64,
            command,
        })
        .collect::<Vec<_>>();
    let report = simulation.step(&envelopes).expect("scenario Tick succeeds");
    assert!(
        report.command_rejections.is_empty(),
        "scenario commands are accepted: {:?}",
        report.command_rejections
    );
    report
}

fn step_empty(simulation: &mut Simulation) -> StepReport {
    step_commands(simulation, Vec::new())
}

fn created_at(report: &StepReport, ordinal: u64) -> EntityId {
    report
        .command_acceptances
        .iter()
        .find(|acceptance| acceptance.ordinal == ordinal)
        .and_then(|acceptance| acceptance.created_entity)
        .expect("placement command creates an entity")
}

fn gate_command(gate_type: GateType, origin: FixedVec2, domain: RoutingDomain) -> Command {
    Command::PlaceGate(PlaceGateCommand {
        gate_type,
        origin,
        routing_domain: domain,
    })
}

fn wire_command(
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

fn gate_port(gate: GateId, port: GatePort) -> EndpointTarget {
    EndpointTarget::GatePort(GatePortRef { gate, port })
}

fn external_input(driver: DriverId, level: LogicLevel, strength: DriveStrength) -> Command {
    Command::SetExternalDriver(SetExternalDriverCommand {
        driver,
        level,
        strength,
    })
}

fn mobile_record(simulation: &Simulation, mobile: MobileId) -> aon_sim::MobileRenderRecord {
    let mut snapshot = RenderSnapshot::default();
    simulation.write_render_snapshot(&mut snapshot);
    snapshot
        .mobiles()
        .iter()
        .copied()
        .find(|record| record.id == mobile)
        .expect("mobile remains observable")
}

fn gate_is_quiescent(simulation: &Simulation, gate: GateId, level: LogicLevel) -> bool {
    simulation.gate_signal_state(gate).is_some_and(|state| {
        state.current_output == level
            && state.desired_output == level
            && state.pending_due_tick.is_none()
            && state.pending_level.is_none()
    })
}

struct StateMobileFixture {
    simulation: Simulation,
    junction: JunctionId,
    edge_a: WireId,
    edge_b: WireId,
    mobile: MobileId,
    q: GateId,
    qbar: GateId,
    set_driver: DriverId,
}

impl StateMobileFixture {
    fn new() -> Self {
        let mut simulation = Simulation::new(package()).expect("simulation starts");

        let junction_report = step_commands(
            &mut simulation,
            vec![Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: point(JUNCTION_X, 0),
            })],
        );
        let junction = JunctionId(created_at(&junction_report, 0));

        let track_report = step_commands(
            &mut simulation,
            vec![
                wire_command(
                    RoutingDomain::OpenWorld,
                    vec![point(0, 0), point(JUNCTION_X, 0)],
                    EndpointTarget::Free,
                    EndpointTarget::Junction(junction),
                ),
                wire_command(
                    RoutingDomain::OpenWorld,
                    vec![point(JUNCTION_X, 0), point(B_X, 0)],
                    EndpointTarget::Junction(junction),
                    EndpointTarget::Free,
                ),
            ],
        );
        let edge_a = WireId(created_at(&track_report, 0));
        let edge_b = WireId(created_at(&track_report, 1));

        let local_bounds = FixedAabb::new(circuit_point(-12, -12), circuit_point(12, 12));
        let mobile_report = step_commands(
            &mut simulation,
            vec![Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
                origin: point(0, 0),
                routing_area: local_bounds,
                footprint: local_bounds,
            })],
        );
        let mobile = MobileId(created_at(&mobile_report, 0));
        let domain = RoutingDomain::MobileSubstrate(mobile.entity_id());

        // Q = NOT(R OR Qbar), Qbar = NOT(S OR Q). The local Junction fans Q out to
        // both the Qbar inverter and the Mobile's intrinsic STOP sink.
        let gates_report = step_commands(
            &mut simulation,
            vec![
                gate_command(GateType::Or, circuit_point(0, -4), domain),
                gate_command(GateType::Not, circuit_point(4, -4), domain),
                gate_command(GateType::Or, circuit_point(0, 4), domain),
                gate_command(GateType::Not, circuit_point(4, 4), domain),
                Command::PlaceJunction(PlaceJunctionCommand {
                    routing_domain: domain,
                    position: circuit_point(2, 4),
                }),
            ],
        );
        let or_q = GateId(created_at(&gates_report, 0));
        let q = GateId(created_at(&gates_report, 1));
        let or_qbar = GateId(created_at(&gates_report, 2));
        let qbar = GateId(created_at(&gates_report, 3));
        let q_fanout = JunctionId(created_at(&gates_report, 4));
        let or_q_ports = simulation.gate_signal_ports(or_q).expect("OR Q ports");
        let or_qbar_ports = simulation
            .gate_signal_ports(or_qbar)
            .expect("OR Qbar ports");
        let reset_driver = or_q_ports.input_a.external_driver;
        let set_driver = or_qbar_ports.input_b.expect("OR input B").external_driver;

        let construction = step_commands(
            &mut simulation,
            vec![
                wire_command(
                    domain,
                    vec![circuit_point(1, -4), circuit_point(3, -4)],
                    gate_port(or_q, GatePort::Output),
                    gate_port(q, GatePort::InputA),
                ),
                wire_command(
                    domain,
                    vec![circuit_point(1, 4), circuit_point(2, 4)],
                    gate_port(or_qbar, GatePort::Output),
                    EndpointTarget::Junction(q_fanout),
                ),
                wire_command(
                    domain,
                    vec![circuit_point(2, 4), circuit_point(3, 4)],
                    EndpointTarget::Junction(q_fanout),
                    gate_port(qbar, GatePort::InputA),
                ),
                wire_command(
                    domain,
                    vec![circuit_point(2, 4), circuit_point(2, 10)],
                    EndpointTarget::Junction(q_fanout),
                    EndpointTarget::MobilePort(MobilePortRef {
                        mobile,
                        port: MobilePort::Stop,
                    }),
                ),
                wire_command(
                    domain,
                    vec![
                        circuit_point(5, -4),
                        circuit_point(6, -4),
                        circuit_point(6, 7),
                        circuit_point(-2, 7),
                        circuit_point(-2, 4),
                        point(-CIRCUIT_PITCH, 7 * CIRCUIT_PITCH / 2),
                    ],
                    gate_port(q, GatePort::Output),
                    gate_port(or_qbar, GatePort::InputA),
                ),
                wire_command(
                    domain,
                    vec![
                        circuit_point(5, 4),
                        circuit_point(7, 4),
                        circuit_point(7, -6),
                        circuit_point(-2, -6),
                        circuit_point(-2, -4),
                        point(-CIRCUIT_PITCH, -7 * CIRCUIT_PITCH / 2),
                    ],
                    gate_port(qbar, GatePort::Output),
                    gate_port(or_q, GatePort::InputB),
                ),
                external_input(reset_driver, LogicLevel::High, ASSERTED_STRENGTH),
            ],
        );
        assert_eq!(construction.command_acceptances.len(), 7);

        for _ in 0..32 {
            let mobile_state = mobile_record(&simulation, mobile);
            if gate_is_quiescent(&simulation, q, LogicLevel::Low)
                && gate_is_quiescent(&simulation, qbar, LogicLevel::High)
                && mobile_state.stop == LogicLevel::Low
            {
                break;
            }
            step_empty(&mut simulation);
        }
        assert!(gate_is_quiescent(&simulation, q, LogicLevel::Low));
        assert!(gate_is_quiescent(&simulation, qbar, LogicLevel::High));
        assert_eq!(mobile_record(&simulation, mobile).stop, LogicLevel::Low);

        step_commands(
            &mut simulation,
            vec![external_input(
                reset_driver,
                LogicLevel::Low,
                RELEASED_STRENGTH,
            )],
        );
        for _ in 0..8 {
            step_empty(&mut simulation);
        }
        assert!(gate_is_quiescent(&simulation, q, LogicLevel::Low));
        assert!(gate_is_quiescent(&simulation, qbar, LogicLevel::High));
        assert_eq!(mobile_record(&simulation, mobile).stop, LogicLevel::Low);

        Self {
            simulation,
            junction,
            edge_a,
            edge_b,
            mobile,
            q,
            qbar,
            set_driver,
        }
    }
}

#[test]
fn b_pulse_is_retained_by_gate_feedback_and_selects_stop_instead_of_return() {
    let mut stopped = StateMobileFixture::new();
    let mut returned = StateMobileFixture::new();
    assert_eq!(
        stopped.simulation.state_hash(),
        returned.simulation.state_hash()
    );

    let b_position = TrackPosition::Edge {
        edge: stopped.edge_b,
        offset: Fixed(B_X - JUNCTION_X),
        heading: Heading::Forward,
    };
    let mut crossed_outbound_junction = false;
    for _ in 0..96 {
        let stopped_report = step_empty(&mut stopped.simulation);
        let returned_report = step_empty(&mut returned.simulation);
        assert_eq!(stopped_report, returned_report);
        assert_eq!(
            stopped.simulation.state_hash(),
            returned.simulation.state_hash()
        );
        crossed_outbound_junction |= stopped_report.mobile_movements[0]
            .junction_decisions
            .iter()
            .any(|decision| {
                decision.junction == stopped.junction
                    && decision.incoming_edge == stopped.edge_a
                    && decision.selected_edge == Some(stopped.edge_b)
                    && decision.kind == JunctionDecisionKind::Straight
            });
        if stopped_report.mobile_movements[0].end == b_position {
            break;
        }
    }
    assert!(
        crossed_outbound_junction,
        "Mobile travels A -> Junction -> B"
    );
    assert_eq!(
        mobile_record(&stopped.simulation, stopped.mobile).track_position,
        b_position
    );
    assert_eq!(
        mobile_record(&returned.simulation, returned.mobile).track_position,
        b_position
    );

    // The external command is the retained fixture's B condition pulse. It supplies an input to
    // the ordinary gate network; movement still receives only STOP/LEFT/RIGHT Sink samples.
    step_commands(
        &mut stopped.simulation,
        vec![external_input(
            stopped.set_driver,
            LogicLevel::High,
            ASSERTED_STRENGTH,
        )],
    );
    step_empty(&mut returned.simulation);

    for _ in 0..32 {
        if gate_is_quiescent(&stopped.simulation, stopped.q, LogicLevel::High)
            && gate_is_quiescent(&stopped.simulation, stopped.qbar, LogicLevel::Low)
            && mobile_record(&stopped.simulation, stopped.mobile).stop == LogicLevel::High
        {
            break;
        }
        step_empty(&mut stopped.simulation);
        step_empty(&mut returned.simulation);
    }
    assert!(gate_is_quiescent(
        &stopped.simulation,
        stopped.q,
        LogicLevel::High
    ));
    assert!(gate_is_quiescent(
        &stopped.simulation,
        stopped.qbar,
        LogicLevel::Low
    ));
    let first_stop = mobile_record(&stopped.simulation, stopped.mobile);
    assert_eq!(first_stop.stop, LogicLevel::High);
    assert_eq!(
        (first_stop.left, first_stop.right),
        (LogicLevel::Low, LogicLevel::Low)
    );
    assert_ne!(
        first_stop.track_position, b_position,
        "signal delay permits a short return"
    );

    // Keep B asserted long enough for Q's ordinary feedback route to reach the opposite OR input.
    // This is signal propagation time, not a runtime latch/FSM transition.
    for _ in 0..16 {
        let stopped_report = step_empty(&mut stopped.simulation);
        step_empty(&mut returned.simulation);
        assert_eq!(stopped_report.mobile_movements[0].granted_budget, Fixed(0));
        assert_eq!(stopped_report.mobile_movements[0].consumed_budget, Fixed(0));
    }
    let stopped_at = mobile_record(&stopped.simulation, stopped.mobile);
    assert_eq!(stopped_at.track_position, first_stop.track_position);
    assert_eq!(stopped_at.stop, LogicLevel::High);

    // Release the B pulse. Q must remain High through physical OR/NOT/Wire feedback, so STOP
    // remains asserted without an FSM, memory primitive, destination, or path-planner command.
    let release = step_commands(
        &mut stopped.simulation,
        vec![external_input(
            stopped.set_driver,
            LogicLevel::Low,
            RELEASED_STRENGTH,
        )],
    );
    step_empty(&mut returned.simulation);
    assert_eq!(release.mobile_movements[0].start, stopped_at.track_position);
    assert_eq!(release.mobile_movements[0].end, stopped_at.track_position);
    assert_eq!(release.mobile_movements[0].granted_budget, Fixed(0));
    assert_eq!(release.mobile_movements[0].consumed_budget, Fixed(0));
    let stopped_b_input = stopped
        .simulation
        .driver_sample(stopped.set_driver)
        .expect("released B input remains observable");
    let returned_b_input = returned
        .simulation
        .driver_sample(returned.set_driver)
        .expect("control B input remains observable");
    assert_eq!(
        (stopped_b_input.level, stopped_b_input.strength),
        (returned_b_input.level, returned_b_input.strength),
        "both runs now have the same current B input; only past input differs"
    );

    let mut crossed_return_junction = false;
    for _ in 0..64 {
        let stopped_report = step_empty(&mut stopped.simulation);
        let returned_report = step_empty(&mut returned.simulation);
        let movement = &stopped_report.mobile_movements[0];
        assert_eq!(movement.start, stopped_at.track_position);
        assert_eq!(movement.end, stopped_at.track_position);
        assert_eq!(movement.controls.stop, LogicLevel::High);
        assert_eq!(movement.granted_budget, Fixed(0));
        assert_eq!(movement.consumed_budget, Fixed(0));
        assert!(gate_is_quiescent(
            &stopped.simulation,
            stopped.q,
            LogicLevel::High
        ));
        assert!(gate_is_quiescent(
            &stopped.simulation,
            stopped.qbar,
            LogicLevel::Low
        ));

        crossed_return_junction |= returned_report.mobile_movements[0]
            .junction_decisions
            .iter()
            .any(|decision| {
                decision.junction == returned.junction
                    && decision.incoming_edge == returned.edge_b
                    && decision.selected_edge == Some(returned.edge_a)
                    && decision.kind == JunctionDecisionKind::Straight
            });
    }
    assert!(
        crossed_return_junction,
        "without the retained B condition, the same Mobile returns through the Junction"
    );
    assert_eq!(
        mobile_record(&stopped.simulation, stopped.mobile).track_position,
        stopped_at.track_position
    );
    assert!(gate_is_quiescent(
        &returned.simulation,
        returned.q,
        LogicLevel::Low
    ));
    assert!(gate_is_quiescent(
        &returned.simulation,
        returned.qbar,
        LogicLevel::High
    ));
    assert_eq!(
        mobile_record(&returned.simulation, returned.mobile).stop,
        LogicLevel::Low
    );
    assert_ne!(
        mobile_record(&returned.simulation, returned.mobile).track_position,
        stopped_at.track_position
    );
}
