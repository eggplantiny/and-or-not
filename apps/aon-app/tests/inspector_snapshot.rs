use aon_app::embedded_empty_package;
use aon_app::inspector::{
    ArrivalSelection, CommandResultKey, InspectorHostState, InspectorInput, InspectorRate,
    InspectorSelection, InspectorTarget, inspector_lines, inspector_panel,
};
use aon_sim::{
    BindPortCommand, Command, CommandEnvelope, EntityId, Fixed, FixedAabb, FixedVec2, GateId,
    GatePort, GatePortRef, GateType, JunctionId, PlaceFixedSubstrateCommand, PlaceGateCommand,
    PlaceJunctionCommand, PlaceWireCommand, RenderSnapshot, RoutingDomain, SignalArrivalKind,
    Simulation, StepReport, Tick, WireEnd, WireId,
};

const CIRCUIT_PITCH: i64 = 16_384;
const WORLD_PITCH: i64 = 65_536;

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

fn envelope(tick: Tick, ordinal: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        target_tick: tick,
        ordinal,
        command,
    }
}

struct Fixture {
    simulation: Simulation,
    snapshot: RenderSnapshot,
    reports: Vec<StepReport>,
    substrate: EntityId,
    and_gate: GateId,
    not_gate: GateId,
    junction: JunctionId,
    wire: WireId,
    rejected_command: CommandResultKey,
}

fn fixture() -> Fixture {
    let mut simulation =
        Simulation::new(embedded_empty_package().expect("embedded package is valid"))
            .expect("simulation starts");
    let bounds = FixedAabb::new(
        point(-4 * WORLD_PITCH, -4 * WORLD_PITCH),
        point(4 * WORLD_PITCH, 4 * WORLD_PITCH),
    );
    let substrate_report = simulation
        .step(&[envelope(
            simulation.next_tick(),
            0,
            Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                origin: point(0, 0),
                routing_area: bounds,
                footprint: bounds,
            }),
        )])
        .expect("substrate placement succeeds");
    let substrate = substrate_report.command_acceptances[0]
        .created_entity
        .expect("placement creates a substrate");
    let domain = RoutingDomain::FixedSubstrate(substrate);

    let gate_report = simulation
        .step(&[
            envelope(
                simulation.next_tick(),
                30,
                Command::PlaceJunction(PlaceJunctionCommand {
                    routing_domain: domain,
                    position: point(0, 4 * CIRCUIT_PITCH),
                }),
            ),
            envelope(
                simulation.next_tick(),
                20,
                Command::PlaceGate(PlaceGateCommand {
                    gate_type: GateType::Not,
                    origin: point(4 * CIRCUIT_PITCH, 0),
                    routing_domain: domain,
                }),
            ),
            envelope(
                simulation.next_tick(),
                10,
                Command::PlaceGate(PlaceGateCommand {
                    gate_type: GateType::And,
                    origin: point(0, 0),
                    routing_domain: domain,
                }),
            ),
        ])
        .expect("gate and junction placements succeed");
    let and_gate = GateId(
        gate_report.command_acceptances[0]
            .created_entity
            .expect("AND placement creates a gate"),
    );
    let not_gate = GateId(
        gate_report.command_acceptances[1]
            .created_entity
            .expect("NOT placement creates a gate"),
    );
    let junction = JunctionId(
        gate_report.command_acceptances[2]
            .created_entity
            .expect("junction placement creates an entity"),
    );
    assert!(
        gate_report
            .signal_arrivals
            .iter()
            .any(|arrival| arrival.kind == SignalArrivalKind::TopologySync),
        "a selected raw Topology Sync Arrival is available"
    );

    let wire_report = simulation
        .step(&[envelope(
            simulation.next_tick(),
            40,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: domain,
                points: vec![point(CIRCUIT_PITCH, 0), point(3 * CIRCUIT_PITCH, 0)],
                endpoint_a: aon_sim::EndpointTarget::GatePort(GatePortRef {
                    gate: and_gate,
                    port: GatePort::Output,
                }),
                endpoint_b: aon_sim::EndpointTarget::GatePort(GatePortRef {
                    gate: not_gate,
                    port: GatePort::InputA,
                }),
            }),
        )])
        .expect("bound wire placement succeeds");
    let wire = WireId(
        wire_report.command_acceptances[0]
            .created_entity
            .expect("wire placement creates an entity"),
    );

    let rejected_report = simulation
        .step(&[envelope(
            simulation.next_tick(),
            77,
            Command::BindPort(BindPortCommand {
                wire,
                end: WireEnd::A,
                target: aon_sim::EndpointTarget::GatePort(GatePortRef {
                    gate: not_gate,
                    port: GatePort::InputB,
                }),
            }),
        )])
        .expect("invalid binding is a typed command rejection");
    assert_eq!(rejected_report.command_rejections.len(), 1);
    let rejected_command = CommandResultKey {
        completed_tick: rejected_report.completed_tick,
        ordinal: 77,
    };

    for _ in 0..6 {
        simulation.step(&[]).expect("empty Tick succeeds");
    }
    let mut snapshot = RenderSnapshot::default();
    simulation.write_render_snapshot(&mut snapshot);

    Fixture {
        simulation,
        snapshot,
        reports: vec![substrate_report, gate_report, wire_report, rejected_report],
        substrate,
        and_gate,
        not_gate,
        junction,
        wire,
        rejected_command,
    }
}

fn panel_for(
    fixture: &Fixture,
    target: InspectorTarget,
    latest_command: Option<CommandResultKey>,
    selected_arrival: Option<ArrivalSelection>,
) -> String {
    inspector_panel(InspectorInput {
        snapshot: &fixture.snapshot,
        retained_reports: &fixture.reports,
        host_state: InspectorHostState::Faulted,
        rate: InspectorRate::Four,
        selection: Some(InspectorSelection {
            target,
            latest_command,
        }),
        selected_arrival,
    })
    .to_text()
}

#[test]
fn inspector_exposes_the_frozen_session_gate_and_arrival_fields() {
    let fixture = fixture();
    let before_hash = fixture.simulation.state_hash();
    let before_tick = fixture.simulation.next_tick();
    let arrival_tick = fixture.reports[1].completed_tick;
    let selected_arrival = fixture.reports[1]
        .signal_arrivals
        .iter()
        .position(|arrival| arrival.kind == SignalArrivalKind::TopologySync)
        .expect("fixture has a Topology Sync Arrival");

    let text = panel_for(
        &fixture,
        InspectorTarget::GatePort(GatePortRef {
            gate: fixture.and_gate,
            port: GatePort::InputB,
        }),
        Some(CommandResultKey {
            completed_tick: fixture.reports[1].completed_tick,
            ordinal: 10,
        }),
        Some(ArrivalSelection {
            completed_tick: arrival_tick,
            observation_index: selected_arrival,
        }),
    );

    assert!(text.contains(&format!(
        "session.scenario_id={}",
        fixture.snapshot.scenario_id()
    )));
    assert!(text.contains(&format!(
        "session.next_tick={}",
        fixture.snapshot.next_tick().0
    )));
    assert!(text.contains("session.completed_tick=3"));
    assert!(text.contains(&format!(
        "session.state_hash={}",
        fixture.snapshot.state_hash()
    )));
    assert!(text.contains("session.semantics=aon-semantics-v1"));
    assert!(text.contains("session.numeric_profile_hash="));
    assert!(text.contains("session.physical_scale_profile_hash="));
    assert!(text.contains("session.balance_profile_hash="));
    assert!(text.contains("session.topology_revision="));
    assert!(text.contains("session.host_state=FAULTED"));
    assert!(text.contains("session.rate=4x"));

    assert!(text.contains(&format!("gate.id={}", fixture.and_gate.entity_id().0)));
    assert!(text.contains("gate.type=AND"));
    assert!(text.contains("gate.origin=(0,0)"));
    assert!(text.contains(&format!(
        "gate.domain=fixed-substrate:{}",
        fixture.substrate.0
    )));
    assert!(text.contains("gate.input_a.sink="));
    assert!(text.contains("gate.input_a.level="));
    assert!(text.contains("gate.input_a.external=driver:"));
    assert!(text.contains("gate.input_b.sink="));
    assert!(text.contains("gate.input_b.level="));
    assert!(text.contains("gate.input_b.external=driver:"));
    assert!(text.contains("gate.output=driver:"));
    assert!(text.contains("gate.current_output="));
    assert!(text.contains("gate.desired_output="));
    assert!(text.contains("gate.pending_generation="));
    assert!(text.contains("gate.pending_due_tick="));
    assert!(text.contains("gate.pending_level="));
    assert!(text.contains("gate.pending_energy="));
    assert!(text.contains("gate.cancelled_heat="));
    assert!(text.contains("command.outcome=accepted"));
    assert!(text.contains("command.ordinal=10"));

    assert!(text.contains("arrival.kind=TOPOLOGY_SYNC"));
    assert!(text.contains("arrival.source_driver="));
    assert!(text.contains("arrival.sink="));
    assert!(text.contains("arrival.sample=driver:"));
    assert!(text.contains(" level:"));
    assert!(text.contains(" strength:"));
    assert!(text.contains(" revision:"));
    assert!(text.contains(" emitted_at:"));
    assert!(text.contains("arrival.counters.applied="));
    assert!(text.contains("arrival.counters.topology_sync_staged="));
    assert!(text.contains("arrival.counters.invalid_path="));
    assert!(text.contains("arrival.counters.stale_revision="));
    assert!(text.contains("arrival.counters.idempotent="));

    for forbidden in [
        "cpu",
        "memory",
        "latch",
        "oscillator",
        "router",
        "controller",
    ] {
        assert!(!text.to_ascii_lowercase().contains(forbidden));
    }
    assert_eq!(fixture.simulation.state_hash(), before_hash);
    assert_eq!(fixture.simulation.next_tick(), before_tick);
}

#[test]
fn every_structural_selection_and_rejected_command_is_rendered_from_snapshot_data() {
    let fixture = fixture();

    let wire = panel_for(
        &fixture,
        InspectorTarget::WireEnd {
            wire: fixture.wire,
            end: WireEnd::A,
        },
        Some(fixture.rejected_command),
        None,
    );
    assert!(wire.contains(&format!("wire.id={}", fixture.wire.entity_id().0)));
    assert!(wire.contains("wire.domain=fixed-substrate:"));
    assert!(wire.contains("wire.points=[(16384,0),(49152,0)]"));
    assert!(wire.contains("wire.endpoint_a=gate-port:"));
    assert!(wire.contains("wire.endpoint_b=gate-port:"));
    assert!(wire.contains("wire.connection_generation="));
    assert!(wire.contains("wire.active_drive=high:"));
    assert!(wire.contains("wire.previous_drive=high:"));
    assert!(wire.contains("wire.active_level="));
    assert!(wire.contains("wire.previous_level="));
    assert!(wire.contains("command.outcome=rejected"));
    assert!(wire.contains("command.ordinal=77"));
    assert!(wire.contains("command.reason=InvalidPort"));

    let junction = panel_for(
        &fixture,
        InspectorTarget::Entity(fixture.junction.entity_id()),
        None,
        None,
    );
    assert!(junction.contains(&format!("junction.id={}", fixture.junction.entity_id().0)));
    assert!(junction.contains("junction.domain=fixed-substrate:"));
    assert!(junction.contains("junction.position=(0,65536)"));
    assert!(junction.contains("junction.connection_generation="));
    assert!(junction.contains("command.outcome=accepted"));

    let substrate = panel_for(
        &fixture,
        InspectorTarget::Entity(fixture.substrate),
        None,
        None,
    );
    assert!(substrate.contains(&format!("fixed_substrate.id={}", fixture.substrate.0)));
    assert!(substrate.contains("fixed_substrate.origin=(0,0)"));
    assert!(substrate.contains("fixed_substrate.routing_area=min:"));
    assert!(substrate.contains("fixed_substrate.footprint=min:"));

    let unary = panel_for(
        &fixture,
        InspectorTarget::Entity(fixture.not_gate.entity_id()),
        None,
        None,
    );
    assert!(unary.contains("gate.type=NOT"));
    assert!(unary.contains("gate.input_b.sink=-"));
    assert!(unary.contains("gate.input_b.level=-"));
    assert!(unary.contains("gate.input_b.external=-"));
}

#[test]
fn report_keys_survive_ring_relocation_and_missing_items_are_explicit() {
    let fixture = fixture();
    let arrival = fixture.reports[1].signal_arrivals[0].clone();
    let mut relocated = fixture.reports.clone();
    relocated.rotate_left(1);
    let lines = inspector_lines(InspectorInput {
        snapshot: &fixture.snapshot,
        retained_reports: &relocated,
        host_state: InspectorHostState::Paused,
        rate: InspectorRate::Quarter,
        selection: None,
        selected_arrival: Some(ArrivalSelection {
            completed_tick: fixture.reports[1].completed_tick,
            observation_index: 0,
        }),
    });
    assert!(lines.iter().any(|line| {
        line == &format!(
            "arrival.source_driver={}",
            arrival.source_driver.entity_id().0
        )
    }));
    assert!(lines.iter().any(|line| line == "selection=none"));
    assert!(lines.iter().any(|line| line == "command=none"));

    let missing = inspector_panel(InspectorInput {
        snapshot: &fixture.snapshot,
        retained_reports: &fixture.reports,
        host_state: InspectorHostState::Running,
        rate: InspectorRate::One,
        selection: Some(InspectorSelection {
            target: InspectorTarget::Entity(EntityId(999_999)),
            latest_command: Some(CommandResultKey {
                completed_tick: Tick(999_999),
                ordinal: 9,
            }),
        }),
        selected_arrival: Some(ArrivalSelection {
            completed_tick: Tick(999_999),
            observation_index: 9,
        }),
    })
    .to_text();
    assert!(missing.contains("selection.live=false"));
    assert!(missing.contains("command=not-retained"));
    assert!(missing.contains("arrival=not-retained"));
    assert!(missing.contains("session.host_state=RUNNING"));
    assert!(missing.contains("session.rate=1x"));
}
