use aon_app::editor::EditIntent;
use aon_app::embedded_empty_package;
use aon_app::host_action::{HostAction, HostActionQueue};
use aon_app::laboratory::{
    LaboratoryError, LaboratoryFault, LaboratorySession, LaboratorySessionMode,
};
use aon_app::pacing::{HostRate, HostRunMode, PacingError};
use aon_app::presenter::{PickTarget, ViewMode};
use aon_app::probe::ProbeTarget;
use aon_sim::{
    BindPortCommand, Command, CommandEnvelope, DriveStrength, EndpointTarget, EntityId, Fixed,
    FixedAabb, FixedVec2, GateType, HashCheckpoint, JunctionId, LogicLevel,
    PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceJunctionCommand,
    PlaceMobileSubstrateCommand, PlaceWireCommand, RemoveEntityCommand, Replay, ReplayError,
    RoutingDomain, SetExternalDriverCommand, Simulation, StateHash, Tick, WireEnd, WireId,
};
use std::time::Duration;

const PITCH: i64 = 65_536;

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

fn bounds() -> FixedAabb {
    FixedAabb::new(point(-8 * PITCH, -8 * PITCH), point(8 * PITCH, 8 * PITCH))
}

fn substrate_command() -> Command {
    Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
        origin: point(0, 0),
        routing_area: bounds(),
        footprint: bounds(),
    })
}

fn gate_command(domain: RoutingDomain) -> Command {
    Command::PlaceGate(PlaceGateCommand {
        gate_type: GateType::Not,
        origin: point(0, 0),
        routing_domain: domain,
    })
}

fn mobile_command() -> Command {
    Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
        origin: point(32 * PITCH, 0),
        routing_area: bounds(),
        footprint: bounds(),
    })
}

fn recorded_replay(final_next_tick: u64) -> (Replay, Vec<StateHash>) {
    let package = embedded_empty_package().expect("embedded package is valid");
    let mut recorder = Simulation::new(package).expect("recording simulation starts");
    let commands = vec![CommandEnvelope {
        target_tick: Tick(0),
        ordinal: 0,
        command: substrate_command(),
    }];
    let mut checkpoints = vec![HashCheckpoint {
        next_tick: Tick(0),
        state_hash: recorder.state_hash(),
    }];
    for tick in 0..final_next_tick {
        let batch = commands
            .iter()
            .filter(|command| command.target_tick == Tick(tick))
            .cloned()
            .collect::<Vec<_>>();
        let report = recorder.step(&batch).expect("recording step succeeds");
        checkpoints.push(HashCheckpoint {
            next_tick: report.next_tick,
            state_hash: report.state_hash,
        });
    }
    let trace = checkpoints
        .iter()
        .map(|checkpoint| checkpoint.state_hash)
        .collect::<Vec<_>>();
    let replay = Replay::new(recorder.replay_header(), commands, checkpoints)
        .expect("recorded Replay is valid");
    (replay, trace)
}

#[test]
fn supported_editor_commands_execute_as_current_tick_envelopes_and_mobile_consumes_no_ordinal() {
    let package = embedded_empty_package().expect("embedded package is valid");
    let mut laboratory = LaboratorySession::new(package).expect("Laboratory starts");
    let initial_hash = laboratory.state_hash();

    queue_and_expect_acceptance(&mut laboratory, substrate_command(), 0);
    assert!(matches!(
        laboratory.queue_command(mobile_command()),
        Err(LaboratoryError::EditScope(_))
    ));
    assert_eq!(laboratory.pending_commands().next_ordinal(), 1);
    assert_eq!(laboratory.edit_log().len(), 1);

    let domain = RoutingDomain::FixedSubstrate(EntityId(1));
    queue_and_expect_acceptance(&mut laboratory, gate_command(domain), 1);
    queue_and_expect_acceptance(
        &mut laboratory,
        Command::PlaceJunction(PlaceJunctionCommand {
            routing_domain: domain,
            position: point(4 * PITCH, 0),
        }),
        2,
    );
    queue_and_expect_acceptance(
        &mut laboratory,
        Command::PlaceWire(PlaceWireCommand {
            routing_domain: domain,
            points: vec![point(4 * PITCH, 0), point(4 * PITCH, PITCH)],
            endpoint_a: EndpointTarget::Free,
            endpoint_b: EndpointTarget::Free,
        }),
        3,
    );
    queue_and_expect_acceptance(
        &mut laboratory,
        Command::BindPort(BindPortCommand {
            wire: WireId(EntityId(4)),
            end: WireEnd::A,
            target: EndpointTarget::Junction(JunctionId(EntityId(3))),
        }),
        4,
    );
    queue_and_expect_acceptance(
        &mut laboratory,
        Command::BindPort(BindPortCommand {
            wire: WireId(EntityId(4)),
            end: WireEnd::A,
            target: EndpointTarget::Free,
        }),
        5,
    );
    let external_driver = laboratory.latest_snapshot().gates()[0]
        .input_a_external_sample
        .driver_id;
    queue_and_expect_acceptance(
        &mut laboratory,
        Command::SetExternalDriver(SetExternalDriverCommand {
            driver: external_driver,
            level: LogicLevel::High,
            strength: DriveStrength(1),
        }),
        6,
    );
    queue_and_expect_acceptance(
        &mut laboratory,
        Command::RemoveEntity(RemoveEntityCommand {
            target: EntityId(4),
        }),
        7,
    );

    assert_ne!(laboratory.state_hash(), initial_hash);
    assert!(laboratory.pending_commands().is_empty());
    assert_eq!(laboratory.pending_commands().next_ordinal(), 8);
    assert_eq!(laboratory.edit_log().len(), 8);
}

fn queue_and_expect_acceptance(
    laboratory: &mut LaboratorySession,
    command: Command,
    expected_ordinal: u64,
) {
    let target_tick = laboratory.next_tick();
    assert_eq!(laboratory.queue_command(command), Ok(expected_ordinal));
    let envelope = laboratory
        .edit_log()
        .last()
        .expect("accepted host edit is retained");
    assert_eq!(envelope.target_tick, target_tick);
    assert_eq!(envelope.ordinal, expected_ordinal);
    let report = laboratory.step_once().expect("Core step succeeds");
    assert_eq!(report.command_acceptances.len(), 1);
    assert!(report.command_rejections.is_empty());
    assert_eq!(report.command_acceptances[0].ordinal, expected_ordinal);
}

#[test]
fn fifo_actions_make_reset_a_real_session_boundary_and_keep_rejections_in_order() {
    let package = embedded_empty_package().expect("embedded package is valid");
    let mut laboratory = LaboratorySession::new(package).expect("Laboratory starts");
    let initial_hash = laboratory.state_hash();
    let mut actions = HostActionQueue::default();
    actions.push(HostAction::QueueEdit(substrate_command()));
    actions.push(HostAction::Reset);
    actions.push(HostAction::QueueEdit(substrate_command()));
    actions.push(HostAction::QueueEdit(mobile_command()));

    let results = laboratory.drain_host_actions(&mut actions);
    assert_eq!(results.len(), 4);
    assert!(results[0].is_ok());
    assert!(results[1].is_ok());
    assert!(results[2].is_ok());
    assert!(matches!(results[3], Err(LaboratoryError::EditScope(_))));
    assert!(actions.is_empty());
    assert_eq!(laboratory.session_id().0, 1);
    assert_eq!(laboratory.pending_commands().next_ordinal(), 1);
    assert_eq!(laboratory.pending_commands().commands().len(), 1);
    assert_eq!(laboratory.pending_commands().commands()[0].ordinal, 0);
    assert_eq!(laboratory.edit_log().len(), 1);
    assert_eq!(laboratory.state_hash(), initial_hash);
}

#[test]
fn single_step_coalesces_stays_paused_and_running_rejection_does_not_mutate_core() {
    let package = embedded_empty_package().expect("embedded package is valid");
    let mut laboratory = LaboratorySession::new(package).expect("Laboratory starts");
    let mut actions = HostActionQueue::default();
    actions.push(HostAction::SingleStep);
    actions.push(HostAction::SingleStep);
    assert!(
        laboratory
            .drain_host_actions(&mut actions)
            .into_iter()
            .all(|result| result.is_ok())
    );

    let reports = laboratory
        .advance_frame(Duration::from_secs(5))
        .expect("coalesced step succeeds");
    assert_eq!(reports.len(), 1);
    assert_eq!(laboratory.next_tick(), Tick(1));
    assert_eq!(laboratory.pacer().mode(), HostRunMode::Paused);
    let hash_before_rejection = laboratory.state_hash();

    laboratory.set_mode(HostRunMode::Running);
    assert!(matches!(
        laboratory.request_single_step(),
        Err(LaboratoryError::Pacing(PacingError::SingleStepWhileRunning))
    ));
    assert_eq!(laboratory.state_hash(), hash_before_rejection);
    assert_eq!(laboratory.next_tick(), Tick(1));
}

fn paced_trace(rate: HostRate, frames: &[Duration]) -> Vec<StateHash> {
    let package = embedded_empty_package().expect("embedded package is valid");
    let mut laboratory = LaboratorySession::new(package).expect("Laboratory starts");
    laboratory.set_rate(rate);
    laboratory.set_mode(HostRunMode::Running);
    for &elapsed in frames {
        laboratory
            .advance_frame(elapsed)
            .expect("rational pacing succeeds");
    }
    laboratory.hash_trace().to_vec()
}

#[test]
fn rational_rates_preserve_every_tick_hash_across_partitions_and_long_frames() {
    let cases = [
        (
            HostRate::Quarter,
            Duration::from_secs(4),
            vec![Duration::from_millis(400); 10],
        ),
        (
            HostRate::One,
            Duration::from_secs(1),
            vec![Duration::from_millis(100); 10],
        ),
        (
            HostRate::Four,
            Duration::from_millis(250),
            vec![Duration::from_millis(25); 10],
        ),
    ];
    for (rate, long_frame, partitions) in cases {
        let long = paced_trace(rate, &[long_frame]);
        let split = paced_trace(rate, &partitions);
        assert_eq!(long.len(), 21, "rate={rate:?}");
        assert_eq!(long, split, "rate={rate:?}");
    }
}

#[test]
fn interactive_reset_clears_every_session_scoped_host_observation() {
    let package = embedded_empty_package().expect("embedded package is valid");
    let mut laboratory = LaboratorySession::new(package).expect("Laboratory starts");
    let initial_hash = laboratory.state_hash();
    laboratory
        .queue_command(substrate_command())
        .expect("substrate queues");
    laboratory
        .step_once()
        .expect("substrate placement succeeds");
    laboratory
        .queue_command(gate_command(RoutingDomain::FixedSubstrate(EntityId(1))))
        .expect("gate queues");
    laboratory.step_once().expect("gate placement succeeds");

    let gate = laboratory.latest_snapshot().gates()[0];
    laboratory
        .add_probe(ProbeTarget::GateOutput(gate.id))
        .expect("live probe target is accepted");
    laboratory
        .set_preview_edit(EditIntent::PlaceGate(PlaceGateCommand {
            gate_type: GateType::Not,
            origin: point(PITCH, 0),
            routing_domain: RoutingDomain::FixedSubstrate(EntityId(1)),
        }))
        .expect("interactive preview is accepted");
    laboratory
        .set_view(ViewMode::Circuit {
            substrate: EntityId(1),
        })
        .expect("live substrate can be inspected");
    laboratory.set_selection(Some(PickTarget::Entity(EntityId(1))));
    laboratory.set_hover(Some(PickTarget::Entity(gate.id.entity_id())));
    laboratory
        .queue_command(gate_command(RoutingDomain::FixedSubstrate(EntityId(1))))
        .expect("pending edit queues");
    laboratory.set_mode(HostRunMode::Running);
    assert!(
        laboratory
            .advance_frame(Duration::from_millis(25))
            .expect("fractional frame succeeds")
            .is_empty()
    );
    assert_ne!(laboratory.pacer().accumulated_credit().numerator, 0);

    laboratory.reset().expect("fresh reset succeeds");
    assert_eq!(laboratory.session_id().0, 1);
    assert_eq!(
        laboratory.session_mode(),
        LaboratorySessionMode::Interactive
    );
    assert_eq!(laboratory.next_tick(), Tick(0));
    assert_eq!(laboratory.state_hash(), initial_hash);
    assert_eq!(laboratory.hash_trace(), &[initial_hash]);
    assert!(laboratory.reports().is_empty());
    assert!(laboratory.pending_commands().is_empty());
    assert_eq!(laboratory.pending_commands().next_ordinal(), 0);
    assert!(laboratory.edit_log().is_empty());
    assert_eq!(laboratory.probes().traces().count(), 0);
    assert!(laboratory.probes().arrival_history().is_empty());
    assert_eq!(laboratory.pacer().mode(), HostRunMode::Paused);
    assert_eq!(laboratory.pacer().rate(), HostRate::One);
    assert_eq!(laboratory.pacer().accumulated_credit().numerator, 0);
    assert!(!laboratory.pacer().single_step_requested());
    assert_eq!(laboratory.view(), ViewMode::Network);
    assert_eq!(laboratory.selection(), None);
    assert_eq!(laboratory.hover(), None);
    assert_eq!(laboratory.preview(), None);
    assert_eq!(laboratory.fault(), None);
}

#[test]
fn replay_is_read_only_stops_at_final_and_restart_repeats_the_complete_trace() {
    let package = embedded_empty_package().expect("embedded package is valid");
    let (replay, expected_trace) = recorded_replay(3);
    let mut laboratory =
        LaboratorySession::from_replay(package, replay).expect("Replay playback starts");
    assert_eq!(
        laboratory.session_mode(),
        LaboratorySessionMode::ReplayPlayback
    );
    let initial_hash = laboratory.state_hash();

    for command in [substrate_command(), mobile_command()] {
        assert_eq!(
            laboratory.queue_command(command),
            Err(LaboratoryError::PlaybackReadOnly)
        );
    }
    assert_eq!(laboratory.pending_commands().next_ordinal(), 0);
    assert!(laboratory.pending_commands().is_empty());
    assert!(laboratory.edit_log().is_empty());

    laboratory
        .apply_host_action(HostAction::SetRate(HostRate::Four))
        .expect("rate remains an observation control");
    laboratory
        .apply_host_action(HostAction::Select(PickTarget::Entity(EntityId(1))))
        .expect("selection remains available");
    laboratory
        .apply_host_action(HostAction::Resume)
        .expect("playback can resume");
    let reports = laboratory
        .advance_frame(Duration::from_secs(1))
        .expect("long frame reaches only the final boundary");
    assert_eq!(reports.len(), 3);
    assert_eq!(laboratory.next_tick(), Tick(3));
    assert_eq!(laboratory.pacer().mode(), HostRunMode::Paused);
    assert_eq!(laboratory.hash_trace(), expected_trace);
    assert!(!laboratory.is_faulted());

    laboratory.reset().expect("Replay reset restarts playback");
    assert_eq!(laboratory.session_id().0, 1);
    assert_eq!(laboratory.state_hash(), initial_hash);
    assert_eq!(laboratory.next_tick(), Tick(0));
    assert_eq!(laboratory.hash_trace(), &[initial_hash]);
    assert!(laboratory.reports().is_empty());
    assert!(laboratory.pending_commands().is_empty());
    assert_eq!(laboratory.pending_commands().next_ordinal(), 0);
    assert_eq!(laboratory.selection(), None);
    assert_eq!(laboratory.pacer().mode(), HostRunMode::Paused);
    assert_eq!(laboratory.pacer().rate(), HostRate::One);
    assert_eq!(
        laboratory.session_mode(),
        LaboratorySessionMode::ReplayPlayback
    );

    laboratory.set_mode(HostRunMode::Running);
    laboratory
        .advance_frame(Duration::from_secs(1))
        .expect("restarted playback completes");
    assert_eq!(laboratory.hash_trace(), expected_trace);
}

#[test]
fn replay_checkpoint_divergence_faults_and_pauses_before_any_later_tick() {
    let package = embedded_empty_package().expect("embedded package is valid");
    let simulation = Simulation::new(package.clone()).expect("simulation starts");
    let initial_hash = simulation.state_hash();
    let wrong_hash = if initial_hash == StateHash::default() {
        StateHash::from_hex(&"01".repeat(32)).expect("alternate hash parses")
    } else {
        StateHash::default()
    };
    let replay = Replay::new(
        simulation.replay_header(),
        Vec::new(),
        vec![
            HashCheckpoint {
                next_tick: Tick(0),
                state_hash: initial_hash,
            },
            HashCheckpoint {
                next_tick: Tick(1),
                state_hash: wrong_hash,
            },
            HashCheckpoint {
                next_tick: Tick(2),
                state_hash: wrong_hash,
            },
        ],
    )
    .expect("wrong golden hashes still form a valid Replay shape");
    let mut laboratory =
        LaboratorySession::from_replay(package, replay).expect("header validates at Tick 0");
    laboratory.set_mode(HostRunMode::Running);

    let error = laboratory
        .advance_frame(Duration::from_secs(1))
        .expect_err("first declared checkpoint diverges");
    assert!(matches!(
        error,
        LaboratoryError::Fatal(LaboratoryFault::Replay(ReplayError::CheckpointDivergence {
            next_tick: Tick(1),
            ..
        }))
    ));
    assert_eq!(laboratory.next_tick(), Tick(1));
    assert_eq!(laboratory.pacer().mode(), HostRunMode::Paused);
    assert!(laboratory.is_faulted());
    let fault_hash = laboratory.state_hash();

    laboratory.set_mode(HostRunMode::Running);
    assert_eq!(laboratory.pacer().mode(), HostRunMode::Paused);
    assert!(matches!(
        laboratory.advance_frame(Duration::from_secs(1)),
        Err(LaboratoryError::SessionFaulted { .. })
    ));
    assert_eq!(laboratory.next_tick(), Tick(1));
    assert_eq!(laboratory.state_hash(), fault_hash);

    laboratory.reset().expect("Replay restart clears the fault");
    assert_eq!(laboratory.next_tick(), Tick(0));
    assert_eq!(laboratory.state_hash(), initial_hash);
    assert!(!laboratory.is_faulted());
    assert_eq!(laboratory.pacer().mode(), HostRunMode::Paused);
}
