use aon_app::embedded_empty_package;
use aon_app::laboratory::{LaboratoryError, LaboratorySession};
use aon_app::pacing::HostRunMode;
use aon_app::probe::ProbeTarget;
use aon_sim::{
    Command, CommandEnvelope, EndpointTarget, EntityId, Fixed, FixedAabb, FixedVec2, GateType,
    HashCheckpoint, PlaceGateCommand, PlaceMobileSubstrateCommand, PlaceWireCommand,
    RenderSnapshot, Replay, ReplayArtifact, RoutingDomain, Simulation, SimulationPackage,
    StateHash, StateHashVersion, StepReport, Tick, decode_replay_artifact, encode_replay_artifact,
};

const WORLD_PITCH: i64 = 65_536;
const CIRCUIT_PITCH: i64 = 16_384;
const FINAL_NEXT_TICK: u64 = 16;

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

fn local_bounds() -> FixedAabb {
    FixedAabb::new(
        point(-8 * CIRCUIT_PITCH, -8 * CIRCUIT_PITCH),
        point(8 * CIRCUIT_PITCH, 8 * CIRCUIT_PITCH),
    )
}

fn mobility_commands() -> Vec<CommandEnvelope> {
    vec![
        CommandEnvelope {
            target_tick: Tick(0),
            ordinal: 0,
            command: Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![point(0, 0), point(32 * WORLD_PITCH, 0)],
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Free,
            }),
        },
        CommandEnvelope {
            target_tick: Tick(1),
            ordinal: 0,
            command: Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
                origin: point(WORLD_PITCH, 0),
                routing_area: local_bounds(),
                footprint: local_bounds(),
            }),
        },
        CommandEnvelope {
            target_tick: Tick(2),
            ordinal: 0,
            command: Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Not,
                origin: point(0, 0),
                routing_domain: RoutingDomain::MobileSubstrate(EntityId(2)),
            }),
        },
    ]
}

struct RecordedMobilityArtifact {
    package: SimulationPackage,
    json: Vec<u8>,
    trace: Vec<StateHash>,
    reports: Vec<StepReport>,
    final_snapshot: RenderSnapshot,
}

fn record_mobility_artifact() -> RecordedMobilityArtifact {
    let package = embedded_empty_package().expect("embedded package is valid");
    let mut recorder = Simulation::new(package.clone()).expect("recording simulation starts");
    let commands = mobility_commands();
    let mut checkpoints = vec![HashCheckpoint {
        next_tick: Tick(0),
        state_hash: recorder.state_hash(),
    }];
    let mut reports = Vec::new();

    for tick in 0..FINAL_NEXT_TICK {
        let batch = commands
            .iter()
            .filter(|command| command.target_tick == Tick(tick))
            .cloned()
            .collect::<Vec<_>>();
        let report = recorder.step(&batch).expect("mobility recording succeeds");
        assert!(
            report.command_rejections.is_empty(),
            "recorded mobility command is accepted at Tick {tick}: {:?}",
            report.command_rejections
        );
        checkpoints.push(HashCheckpoint {
            next_tick: report.next_tick,
            state_hash: report.state_hash,
        });
        reports.push(report);
    }

    let trace = checkpoints
        .iter()
        .map(|checkpoint| checkpoint.state_hash)
        .collect::<Vec<_>>();
    let replay = Replay::new(recorder.replay_header(), commands, checkpoints)
        .expect("recorded mobility Replay is valid");
    assert_eq!(
        replay.header().state_hash_version,
        StateHashVersion::V5,
        "mobility artifact must bind the complete canonical V5 state domain"
    );
    let artifact = ReplayArtifact::new("../scenarios/empty.json", replay)
        .expect("portable mobility Replay artifact path");
    let json = encode_replay_artifact(&artifact).expect("mobility Replay encodes as JSON");
    let mut final_snapshot = RenderSnapshot::default();
    recorder.write_render_snapshot(&mut final_snapshot);

    RecordedMobilityArtifact {
        package,
        json,
        trace,
        reports,
        final_snapshot,
    }
}

fn complete_playback(laboratory: &mut LaboratorySession) {
    laboratory.set_mode(HostRunMode::Running);
    while laboratory.next_tick() < Tick(FINAL_NEXT_TICK) {
        laboratory
            .step_once()
            .expect("mobility Replay Tick succeeds");
    }
    assert_eq!(laboratory.next_tick(), Tick(FINAL_NEXT_TICK));
    assert_eq!(laboratory.pacer().mode(), HostRunMode::Paused);
    assert_eq!(laboratory.step_once(), Err(LaboratoryError::ReplayComplete));
}

#[test]
fn mobility_replay_json_restart_repeats_full_v5_trace_reports_and_snapshot() {
    let recorded = record_mobility_artifact();
    let decoded = decode_replay_artifact(&recorded.json).expect("mobility Replay JSON decodes");
    assert_eq!(decoded.scenario_path(), "../scenarios/empty.json");
    assert_eq!(
        decoded.replay().header().state_hash_version,
        StateHashVersion::V5
    );
    assert_eq!(
        encode_replay_artifact(&decoded).expect("decoded Replay re-encodes"),
        recorded.json,
        "canonical JSON round-trip is byte-identical"
    );

    let mut laboratory = LaboratorySession::from_replay(recorded.package, decoded.replay().clone())
        .expect("decoded mobility Replay starts in LaboratorySession");
    complete_playback(&mut laboratory);
    let first_trace = laboratory.hash_trace().to_vec();
    let first_reports = laboratory.reports().to_vec();
    let first_snapshot = laboratory.latest_snapshot().clone();
    assert_eq!(first_trace, recorded.trace);
    assert_eq!(first_reports, recorded.reports);
    assert_eq!(first_snapshot, recorded.final_snapshot);

    laboratory
        .reset()
        .expect("Replay reset starts a fresh session");
    assert_eq!(laboratory.next_tick(), Tick(0));
    assert_eq!(laboratory.hash_trace(), &first_trace[..1]);
    assert!(laboratory.reports().is_empty());
    complete_playback(&mut laboratory);
    assert_eq!(laboratory.hash_trace(), first_trace);
    assert_eq!(laboratory.reports(), first_reports);
    assert_eq!(laboratory.latest_snapshot(), &first_snapshot);
}

fn queue_same_command(
    observed: &mut LaboratorySession,
    unobserved: &mut LaboratorySession,
    command: Command,
) {
    assert_eq!(
        observed
            .queue_command(command.clone())
            .expect("observed edit queues"),
        unobserved
            .queue_command(command)
            .expect("unobserved edit queues")
    );
    let observed_report = observed.step_once().expect("observed edit succeeds");
    let unobserved_report = unobserved.step_once().expect("plain edit succeeds");
    assert_eq!(observed_report, unobserved_report);
    assert_eq!(observed.latest_snapshot(), unobserved.latest_snapshot());
}

#[test]
fn stop_left_right_probes_do_not_change_multi_tick_reports_events_hashes_or_ticks() {
    let package = embedded_empty_package().expect("embedded package is valid");
    let mut observed = LaboratorySession::new(package.clone()).expect("observed session starts");
    let mut unobserved = LaboratorySession::new(package).expect("plain session starts");

    for envelope in mobility_commands() {
        queue_same_command(&mut observed, &mut unobserved, envelope.command);
    }
    let mobile = observed.latest_snapshot().mobiles()[0];
    assert_eq!(
        observed.latest_snapshot().mobiles(),
        unobserved.latest_snapshot().mobiles()
    );
    let targets = [
        ProbeTarget::Sink(mobile.ports.stop),
        ProbeTarget::Sink(mobile.ports.left),
        ProbeTarget::Sink(mobile.ports.right),
    ];
    let probes = targets.map(|target| {
        observed
            .add_probe(target)
            .expect("intrinsic mobility control sink is probeable")
    });

    let first_compared_tick = observed.next_tick();
    let mut saw_signal_event = false;
    for offset in 0..24 {
        let observed_report = observed.step_once().expect("observed Tick succeeds");
        let unobserved_report = unobserved.step_once().expect("plain Tick succeeds");
        assert_eq!(
            observed_report, unobserved_report,
            "full StepReport, including ordered signal events and movement, differs at offset {offset}"
        );
        assert_eq!(
            observed_report.completed_tick,
            Tick(first_compared_tick.0 + offset)
        );
        assert_eq!(observed.next_tick(), unobserved.next_tick());
        assert_eq!(observed.state_hash(), unobserved.state_hash());
        assert_eq!(observed.hash_trace(), unobserved.hash_trace());
        assert_eq!(observed.latest_snapshot(), unobserved.latest_snapshot());
        saw_signal_event |= !observed_report.driver_changes.is_empty()
            || !observed_report.signal_changes.is_empty()
            || !observed_report.signal_arrivals.is_empty();
    }
    assert!(
        saw_signal_event,
        "comparison must span real ordered signal activity, not only quiescent Ticks"
    );
    for (probe, target) in probes.into_iter().zip(targets) {
        let trace = observed.probes().trace(probe).expect("probe remains live");
        assert_eq!(trace.target(), target);
        assert_eq!(trace.history().len(), 24);
        for (offset, sample) in trace.history().iter().enumerate() {
            assert_eq!(
                sample.completed_tick,
                Tick(first_compared_tick.0 + u64::try_from(offset).expect("offset fits"))
            );
            assert_eq!(sample.next_tick, Tick(sample.completed_tick.0 + 1));
        }
    }
}
