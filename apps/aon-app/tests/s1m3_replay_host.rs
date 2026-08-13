use aon_app::run_replay_host_harness;
use aon_headless::{load_package, load_replay, run_replay_file};
use aon_sim::{DemandId, DemandKind, Energy, EntityId, HeatEnergy, PowerHeatKind, WireId};
use std::path::PathBuf;

const WIRE_LOW_ID: WireId = WireId(EntityId(4));
const WIRE_HIGH_ID: WireId = WireId(EntityId(5));

fn replay_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/replays/s1-m3-c22-capacity-support-v1.json")
}

#[test]
fn retained_c22_v6_trace_and_reports_match_headless_and_bevy() {
    let replay_path = replay_path();
    let artifact = load_replay(&replay_path).expect("the retained C-22 Replay decodes");
    let scenario_path = replay_path
        .parent()
        .expect("the retained Replay has a parent")
        .join(artifact.scenario_path());
    let package = load_package(scenario_path).expect("the retained C-22 package loads");
    let headless = run_replay_file(&replay_path).expect("the retained Replay runs headlessly");

    for (presentation_updates, presenter_enabled) in [(0, false), (5, true)] {
        let bevy = run_replay_host_harness(
            package.clone(),
            artifact.replay().clone(),
            presentation_updates,
            presenter_enabled,
        )
        .expect("the retained C-22 Replay runs through Bevy FixedUpdate");
        assert_eq!(bevy.checkpoints(), headless.checkpoints());
        assert_eq!(bevy.reports(), headless.reports());
    }

    let power = headless.reports()[1]
        .power
        .as_ref()
        .expect("the C-22 evidence Tick reports Power");
    assert_eq!(
        power
            .load(DemandId::new(
                WIRE_LOW_ID.entity_id(),
                DemandKind::OvercapacitySupport,
            ))
            .expect("the lower-ID Wire support load exists")
            .granted,
        Energy(17)
    );
    assert_eq!(
        power
            .load(DemandId::new(
                WIRE_HIGH_ID.entity_id(),
                DemandKind::OvercapacitySupport,
            ))
            .expect("the higher-ID Wire support load exists")
            .granted,
        Energy(11)
    );
    assert_eq!(
        power
            .heat_contributions
            .iter()
            .filter(|heat| heat.kind == PowerHeatKind::OvercapacitySupport)
            .map(|heat| heat.energy)
            .collect::<Vec<_>>(),
        [HeatEnergy(4), HeatEnergy(3)]
    );
}
