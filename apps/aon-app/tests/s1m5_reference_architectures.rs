use aon_app::run_replay_host_harness;
use aon_headless::{load_package, load_replay, run_replay_file};
use aon_sim::{StepReport, Tick, decode_reference_pair_manifest};
use std::fs;
use std::path::PathBuf;

fn fixture(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn replay(name: &str) -> PathBuf {
    fixture(&format!("fixtures/replays/s1-m5/{name}"))
}

fn assert_final_response_window(name: &str, reports: &[StepReport]) {
    let stimulus = reports
        .iter()
        .find(|report| report.completed_tick == Tick(18))
        .expect("the retained host trace contains T18");
    assert!(
        stimulus.contacts.is_empty(),
        "`{name}` has no contact on the T18 stimulus Tick"
    );
    assert!(
        stimulus
            .power
            .as_ref()
            .expect("T18 contains Power sensing")
            .sense
            .iter()
            .filter(|sense| sense.sampled_presence)
            .count()
            >= 4,
        "`{name}` samples all four semantic stimuli at T18"
    );

    let response = reports
        .iter()
        .find(|report| report.completed_tick == Tick(19))
        .expect("the retained host trace contains T19");
    let positive_contacts = response
        .contacts
        .iter()
        .filter(|contact| contact.absorbed.0 > 0)
        .collect::<Vec<_>>();
    assert_eq!(
        positive_contacts.len(),
        4,
        "`{name}` has exactly four positive contacts at T19"
    );
    assert!(
        positive_contacts
            .iter()
            .all(|contact| contact.absorbed.0 == 1),
        "`{name}` absorbs exactly one Energy per T19 contact"
    );
    assert!(
        reports.iter().all(|report| report.destructions.is_empty()),
        "`{name}` has no destruction through the complete host trace"
    );
    assert_eq!(
        reports.last().expect("the trace is nonempty").next_tick,
        Tick(20)
    );
}

#[test]
fn retained_s1m5_complete_reports_and_v7_hashes_match_headless_and_bevy() {
    let pair_bytes = fs::read(fixture("fixtures/experiments/s1-m5-reference-pair-v1.json"))
        .expect("the retained Pair exists");
    let pair = decode_reference_pair_manifest(&pair_bytes).expect("the retained Pair decodes");
    assert_eq!(pair.build_end_tick(), Tick(18));
    assert_eq!(pair.measurement_start_tick(), Tick(18));
    assert_eq!(pair.max_ticks(), Tick(20));

    for name in ["brute-v1.json", "computed-v1.json"] {
        let path = replay(name);
        let artifact = load_replay(&path).expect("the retained S1-M5 Replay strictly decodes");
        let scenario_path = path
            .parent()
            .expect("the Replay has a parent")
            .join(artifact.scenario_path());
        let package = load_package(&scenario_path).expect("the retained Scenario package loads");
        let headless = run_replay_file(&path).expect("the retained Replay runs headlessly");
        assert_final_response_window(name, headless.reports());

        for (presentation_updates, presenter_enabled) in [(0, false), (5, true)] {
            let bevy = run_replay_host_harness(
                package.clone(),
                artifact.replay().clone(),
                presentation_updates,
                presenter_enabled,
            )
            .expect("the retained Replay runs through Bevy FixedUpdate");
            assert_final_response_window(name, bevy.reports());
            assert_eq!(
                bevy.checkpoints(),
                headless.checkpoints(),
                "presentation scheduling must not change `{name}` State V7 checkpoints"
            );
            assert_eq!(
                bevy.reports(),
                headless.reports(),
                "presentation scheduling must not change `{name}` complete StepReports"
            );
        }
    }
}
