use aon_app::editor::{EditIntent, PendingCommands};
use aon_app::embedded_empty_package;
use aon_app::laboratory::LaboratorySession;
use aon_app::pacing::{HostRate, HostRunMode, PacingError, TickPacer};
use aon_app::probe::{MAX_SIGNAL_PROBES, PROBE_HISTORY_TICKS, ProbeError, ProbeRack, ProbeTarget};
use aon_sim::{
    Command, CommandEnvelope, EntityId, Fixed, FixedAabb, FixedVec2, GateId, GateType,
    PlaceFixedSubstrateCommand, PlaceGateCommand, RenderSnapshot, RoutingDomain, Simulation, Tick,
};
use std::time::Duration;

const WORLD_PITCH: i64 = 65_536;
const SUBSTRATE_HALF_EXTENT: i64 = 8 * WORLD_PITCH;

fn place_substrate() -> EditIntent {
    let bounds = FixedAabb::new(
        FixedVec2::new(Fixed(-SUBSTRATE_HALF_EXTENT), Fixed(-SUBSTRATE_HALF_EXTENT)),
        FixedVec2::new(Fixed(SUBSTRATE_HALF_EXTENT), Fixed(SUBSTRATE_HALF_EXTENT)),
    );
    EditIntent::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
        origin: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
        routing_area: bounds,
        footprint: bounds,
    })
}

fn place_not(origin_x: i64) -> EditIntent {
    EditIntent::PlaceGate(PlaceGateCommand {
        gate_type: GateType::Not,
        origin: FixedVec2::new(Fixed(origin_x), Fixed::ZERO),
        routing_domain: RoutingDomain::FixedSubstrate(EntityId(1)),
    })
}

#[test]
fn pacer_defaults_paused_and_uses_exact_quarter_normal_and_four_x_credit() {
    let mut paused = TickPacer::default();
    assert_eq!(paused.mode(), HostRunMode::Paused);
    assert_eq!(paused.rate(), HostRate::One);
    assert_eq!(paused.ticks_due(Duration::from_secs(10), 20), Ok(0));

    assert_eq!(paused.request_single_step(), Ok(()));
    assert_eq!(paused.request_single_step(), Ok(()));
    assert_eq!(paused.ticks_due(Duration::from_secs(10), 20), Ok(1));
    assert_eq!(paused.ticks_due(Duration::from_secs(10), 20), Ok(0));

    for (speed, expected_ticks) in [
        (HostRate::Quarter, 5),
        (HostRate::One, 20),
        (HostRate::Four, 80),
    ] {
        let mut pacer = TickPacer::default();
        pacer.set_mode(HostRunMode::Running);
        pacer.set_rate(speed);
        assert_eq!(
            pacer.ticks_due(Duration::from_secs(1), 20),
            Ok(expected_ticks)
        );
        assert_eq!(pacer.accumulated_credit().numerator, 0);
    }

    let mut split = TickPacer::default();
    split.set_mode(HostRunMode::Running);
    split.set_rate(HostRate::Quarter);
    let ticks = [
        Duration::from_millis(333),
        Duration::from_millis(333),
        Duration::from_millis(334),
    ]
    .into_iter()
    .map(|elapsed| split.ticks_due(elapsed, 20).expect("credit fits"))
    .sum::<u64>();
    assert_eq!(ticks, 5);
    assert_eq!(split.accumulated_credit().numerator, 0);

    let mut residual = TickPacer::default();
    residual.set_mode(HostRunMode::Running);
    assert_eq!(residual.ticks_due(Duration::from_millis(25), 20), Ok(0));
    residual.set_rate(HostRate::Quarter);
    assert_eq!(residual.ticks_due(Duration::from_millis(100), 20), Ok(1));
    assert_eq!(residual.accumulated_credit().numerator, 0);

    assert_eq!(
        residual.request_single_step(),
        Err(PacingError::SingleStepWhileRunning)
    );
    assert_eq!(
        residual.ticks_due(Duration::ZERO, 0),
        Err(PacingError::ZeroSimulationFrequency)
    );
}

#[test]
fn pending_commands_assign_stable_ordinals_without_preview_side_effects() {
    let mut pending = PendingCommands::default();
    let preview = pending.preview(Tick(3), place_not(0));
    assert_eq!(preview.target_tick(), Tick(3));
    assert_eq!(pending.next_ordinal(), 0);
    assert!(pending.is_empty());

    assert_eq!(pending.queue(Tick(3), place_not(0)), Ok(0));
    assert_eq!(pending.queue(Tick(4), place_not(65_536)), Ok(1));
    assert_eq!(pending.queue(Tick(3), place_not(131_072)), Ok(2));
    let tick_three = pending.commands_for_tick(Tick(3));
    assert_eq!(
        tick_three
            .iter()
            .map(|command| command.ordinal)
            .collect::<Vec<_>>(),
        [0, 2]
    );
    assert_eq!(pending.commands().len(), 3);

    assert_eq!(
        pending
            .drain_for_tick(Tick(3))
            .iter()
            .map(|command| command.ordinal)
            .collect::<Vec<_>>(),
        [0, 2]
    );
    assert_eq!(pending.commands().len(), 1);
    assert_eq!(pending.commands()[0].target_tick, Tick(4));
}

#[test]
fn paused_edit_single_step_matches_direct_core_and_reset_starts_a_new_session() {
    let package = embedded_empty_package().expect("embedded package is valid");
    let mut laboratory = LaboratorySession::new(package.clone()).expect("laboratory starts");
    let initial_hash = laboratory.state_hash();
    assert!(matches!(
        laboratory.add_probe(ProbeTarget::GateOutput(GateId(EntityId(1)))),
        Err(aon_app::laboratory::LaboratoryError::Probe(
            ProbeError::UnknownTarget
        ))
    ));

    let preview = laboratory.preview_edit(place_substrate());
    assert_eq!(preview.target_tick(), Tick(0));
    assert_eq!(
        laboratory
            .queue_edit(place_substrate())
            .expect("edit queues"),
        0
    );
    assert_eq!(laboratory.state_hash(), initial_hash);
    let mut before = RenderSnapshot::default();
    laboratory.render_snapshot(&mut before);
    assert_eq!(before.next_tick(), Tick(0));
    assert_eq!(before.primitive_count(), 0);

    laboratory
        .request_single_step()
        .expect("single-step intent is valid while paused");
    let reports = laboratory
        .advance_frame(Duration::from_secs(10))
        .expect("single step succeeds");
    assert_eq!(reports.len(), 1, "single-step intent advances exactly once");
    assert_eq!(reports[0].next_tick, Tick(1));
    assert_eq!(laboratory.pending_commands().commands().len(), 0);
    assert_eq!(laboratory.edit_log().len(), 1);
    assert_eq!(
        laboratory
            .advance_frame(Duration::from_secs(10))
            .expect("paused frame succeeds")
            .len(),
        0
    );

    let direct_command = CommandEnvelope {
        target_tick: Tick(0),
        ordinal: 0,
        command: Command::from(place_substrate()),
    };
    let mut direct = Simulation::new(package).expect("direct core starts");
    let direct_report = direct.step(&[direct_command]).expect("direct core steps");
    assert_eq!(laboratory.state_hash(), direct_report.state_hash, "C-25");

    assert_eq!(
        laboratory
            .queue_edit(place_not(262_144))
            .expect("edit queues"),
        1
    );
    laboratory
        .reset()
        .expect("reset creates a fresh Simulation");
    assert_eq!(laboratory.session_id().0, 1);
    assert_eq!(laboratory.state_hash(), initial_hash);
    assert_eq!(laboratory.pacer().mode(), HostRunMode::Paused);
    assert_eq!(laboratory.pacer().rate(), HostRate::One);
    assert!(laboratory.pending_commands().is_empty());
    assert!(laboratory.edit_log().is_empty());
    assert_eq!(laboratory.probes().traces().count(), 0);
    let mut after = RenderSnapshot::default();
    laboratory.render_snapshot(&mut after);
    assert_eq!(after.next_tick(), Tick(0));
    assert_eq!(after.primitive_count(), 0);
}

#[test]
fn probes_are_non_intervening_bounded_and_have_visible_panels() {
    let package = embedded_empty_package().expect("embedded package is valid");
    let mut observed = LaboratorySession::new(package.clone()).expect("laboratory starts");
    let mut unobserved = LaboratorySession::new(package).expect("laboratory starts");

    for (index, laboratory) in [&mut observed, &mut unobserved].into_iter().enumerate() {
        assert_eq!(
            laboratory
                .queue_edit(place_substrate())
                .expect("substrate queues"),
            0
        );
        laboratory
            .request_single_step()
            .expect("single step is valid");
        let substrate_reports = laboratory
            .advance_frame(Duration::ZERO)
            .expect("substrate placement succeeds");
        assert_eq!(substrate_reports.len(), 1);
        assert_eq!(substrate_reports[0].command_acceptances.len(), 1);

        assert_eq!(laboratory.queue_edit(place_not(0)).expect("edit queues"), 1);
        assert_eq!(
            laboratory
                .queue_edit(place_not(262_144))
                .expect("edit queues"),
            2
        );
        laboratory
            .request_single_step()
            .expect("single step is valid");
        let reports = laboratory
            .advance_frame(Duration::ZERO)
            .expect("placement step succeeds");
        assert_eq!(reports.len(), 1, "fixture {index} advances once");
    }
    assert_eq!(observed.state_hash(), unobserved.state_hash());

    let mut snapshot = RenderSnapshot::default();
    observed.render_snapshot(&mut snapshot);
    let mut targets = Vec::new();
    for gate in snapshot.gates() {
        targets.extend([
            ProbeTarget::Driver(gate.output_sample.driver_id),
            ProbeTarget::Driver(gate.input_a_external_sample.driver_id),
            ProbeTarget::Sink(gate.ports.input_a.sink),
            ProbeTarget::GateOutput(gate.id),
            ProbeTarget::GateInputA(gate.id),
        ]);
    }
    assert!(targets.len() > MAX_SIGNAL_PROBES);
    let mut ids = Vec::new();
    for &target in &targets[..MAX_SIGNAL_PROBES] {
        ids.push(observed.add_probe(target).expect("probe fits"));
    }
    assert!(matches!(
        observed.add_probe(targets[MAX_SIGNAL_PROBES]),
        Err(aon_app::laboratory::LaboratoryError::Probe(
            ProbeError::ProbeLimitReached {
                limit: MAX_SIGNAL_PROBES
            }
        ))
    ));

    for _ in 0..300 {
        let observed_report = observed.step_once().expect("observed step succeeds");
        let unobserved_report = unobserved.step_once().expect("plain step succeeds");
        assert_eq!(observed_report.state_hash, unobserved_report.state_hash);
    }
    assert_eq!(
        observed
            .probes()
            .trace(ids[0])
            .expect("probe exists")
            .history()
            .len(),
        PROBE_HISTORY_TICKS
    );
    assert_eq!(
        observed.probes().arrival_history().len(),
        PROBE_HISTORY_TICKS
    );

    let waveform = observed.probes().waveform_panel(8).to_text();
    assert!(waveform.contains("Waveform last 8"));
    assert!(waveform.contains("11111111"));
    let inspector = observed
        .probes()
        .inspector_panel(ids[0])
        .expect("probe exists")
        .to_text();
    assert!(inspector.contains("completed_tick=301"));
    assert!(inspector.contains("logic=HIGH"));
}

#[test]
fn core_arrival_and_revision_markers_are_exact_and_target_specific() {
    let package = embedded_empty_package().expect("embedded package is valid");
    let mut simulation = Simulation::new(package).expect("simulation starts");
    let substrate = CommandEnvelope {
        target_tick: Tick(0),
        ordinal: 0,
        command: Command::from(place_substrate()),
    };
    let substrate_report = simulation
        .step(&[substrate])
        .expect("substrate placement succeeds");
    assert_eq!(substrate_report.command_acceptances.len(), 1);
    let placement = CommandEnvelope {
        target_tick: Tick(1),
        ordinal: 1,
        command: Command::from(place_not(0)),
    };
    let placement_report = simulation
        .step(&[placement])
        .expect("gate placement succeeds");
    assert_eq!(placement_report.command_acceptances.len(), 1);
    let gate = GateId(
        placement_report.command_acceptances[0]
            .created_entity
            .expect("gate placement creates one entity"),
    );
    let ports = simulation
        .gate_signal_ports(gate)
        .expect("placed gate has ports");

    let mut rack = ProbeRack::default();
    let driver_probe = rack
        .add_validated(
            &simulation,
            ProbeTarget::Driver(ports.input_a.external_driver),
        )
        .expect("driver probe fits");
    let sink_probe = rack
        .add_validated(&simulation, ProbeTarget::Sink(ports.input_a.sink))
        .expect("sink probe fits");
    let sample = simulation
        .driver_sample(ports.input_a.external_driver)
        .expect("external input Driver remains live");
    assert!(
        placement_report
            .signal_arrivals
            .iter()
            .any(
                |arrival| arrival.source_driver == ports.input_a.external_driver
                    && arrival.sink == ports.input_a.sink
                    && arrival.kind == aon_sim::SignalArrivalKind::TopologySync
            ),
        "gate activation exposes a real due TopologySync observation"
    );
    rack.record_step(&simulation, &placement_report);

    let driver_markers = rack
        .trace(driver_probe)
        .and_then(|trace| trace.latest())
        .expect("driver sample exists")
        .markers;
    assert!(driver_markers.driver_revision.is_some());
    assert_eq!(
        driver_markers.revision_token(),
        Some(format!("r{}", sample.revision.0))
    );
    assert!(driver_markers.arrival_band.topology_sync);
    assert!(driver_markers.target_arrival);

    let sink_markers = rack
        .trace(sink_probe)
        .and_then(|trace| trace.latest())
        .expect("sink sample exists")
        .markers;
    assert_eq!(sink_markers.driver_revision, None);
    assert_eq!(sink_markers.arrival_band.token(), "S");
    assert!(sink_markers.target_arrival);
    let waveform = rack.waveform_panel(1).to_text();
    assert!(waveform.contains("arrivals S"));
    assert!(waveform.contains(&format!("r{}", sample.revision.0)));
}
