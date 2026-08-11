use aon_app::{embedded_empty_package, run_host_harness, run_paced_host_harness};
use aon_headless::{load_package, run_scenario};
use aon_sim::StateHash;
use std::path::PathBuf;
use std::time::Duration;

fn empty_scenario() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/scenarios/empty.json")
        .canonicalize()
        .expect("empty scenario path exists")
}

fn assert_trace_equal(headless: &[StateHash], bevy: &[StateHash]) {
    assert_eq!(
        headless.len(),
        bevy.len(),
        "trace lengths differ: headless={}, bevy={}",
        headless.len(),
        bevy.len()
    );

    for (index, (headless_hash, bevy_hash)) in headless.iter().zip(bevy).enumerate() {
        assert_eq!(
            headless_hash, bevy_hash,
            "first divergence at checkpoint {index}: headless={headless_hash}, bevy={bevy_hash}"
        );
    }
}

#[test]
fn embedded_bevy_artifacts_match_filesystem_artifacts() {
    let embedded = embedded_empty_package().expect("embedded package is valid");
    let filesystem = load_package(empty_scenario()).expect("filesystem package is valid");

    let embedded_trace = run_host_harness(embedded, 0, 0, true).expect("host succeeds");
    let filesystem_trace = run_host_harness(filesystem, 0, 0, true).expect("host succeeds");
    assert_eq!(embedded_trace, filesystem_trace);
}

#[test]
fn headless_and_bevy_fixed_update_have_identical_traces() {
    for ticks in [0, 1, 100] {
        let headless = run_scenario(empty_scenario(), ticks).expect("headless succeeds");

        for presentation_updates in [0, 1, 7] {
            let package = load_package(empty_scenario()).expect("package loads");
            let bevy = run_host_harness(package, ticks, presentation_updates, true)
                .expect("Bevy harness succeeds");
            assert_trace_equal(headless.checkpoints(), bevy.checkpoints());
        }

        let package = load_package(empty_scenario()).expect("package loads");
        let without_presenter =
            run_host_harness(package, ticks, 7, false).expect("Bevy harness succeeds");
        assert_trace_equal(headless.checkpoints(), without_presenter.checkpoints());
    }
}

#[test]
fn native_pacing_preserves_all_tick_debt_across_long_frames() {
    let headless = run_scenario(empty_scenario(), 20).expect("headless succeeds");

    let one_long_frame = run_paced_host_harness(
        load_package(empty_scenario()).expect("package loads"),
        &[Duration::from_secs(1)],
    )
    .expect("paced Bevy host succeeds");
    assert_trace_equal(headless.checkpoints(), one_long_frame.checkpoints());

    let ten_short_frames = run_paced_host_harness(
        load_package(empty_scenario()).expect("package loads"),
        &[Duration::from_millis(100); 10],
    )
    .expect("paced Bevy host succeeds");
    assert_trace_equal(headless.checkpoints(), ten_short_frames.checkpoints());
}
