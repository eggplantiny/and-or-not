use aon_headless::run_scenario;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn empty_scenario() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/scenarios/empty.json")
        .canonicalize()
        .expect("empty scenario path exists")
}

#[test]
fn runner_supports_zero_one_and_one_hundred_ticks() {
    for ticks in [0, 1, 100] {
        let first = run_scenario(empty_scenario(), ticks).expect("scenario succeeds");
        let second = run_scenario(empty_scenario(), ticks).expect("scenario is repeatable");

        assert_eq!(first, second);
        assert_eq!(first.completed_ticks(), ticks);
        assert_eq!(first.checkpoints().len(), ticks as usize + 1);
    }
}

#[test]
fn cli_runs_outside_the_workspace_and_matches_library_hash() {
    let expected = run_scenario(empty_scenario(), 100).expect("library run succeeds");
    let output = Command::new(env!("CARGO_BIN_EXE_aon-headless"))
        .args([
            "scenario",
            empty_scenario().to_str().expect("path is UTF-8"),
            "--ticks",
            "100",
        ])
        .current_dir(std::env::temp_dir())
        .output()
        .expect("headless process starts");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(stdout.contains("scenario = empty\n"));
    assert!(stdout.contains("completed_ticks = 100\n"));
    assert!(stdout.contains(&format!("state_hash = {}\n", expected.final_hash())));
    assert!(output.stderr.is_empty());
}

#[test]
fn usage_error_has_stable_exit_code() {
    let output = Command::new(env!("CARGO_BIN_EXE_aon-headless"))
        .output()
        .expect("headless process starts");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("error: usage:"));
}

#[test]
fn missing_artifact_has_stable_exit_code() {
    let output = Command::new(env!("CARGO_BIN_EXE_aon-headless"))
        .args(["scenario", "/definitely/missing/aon.json", "--ticks", "1"])
        .output()
        .expect("headless process starts");

    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("NotFound"));
}

#[test]
fn malformed_artifact_has_stable_exit_code_without_a_panic() {
    let malformed_path = std::env::temp_dir().join(format!(
        "aon-malformed-scenario-{}.json",
        std::process::id()
    ));
    fs::write(&malformed_path, b"{").expect("temporary malformed fixture is writable");

    let output = Command::new(env!("CARGO_BIN_EXE_aon-headless"))
        .args([
            "scenario",
            malformed_path.to_str().expect("path is UTF-8"),
            "--ticks",
            "1",
        ])
        .output()
        .expect("headless process starts");
    fs::remove_file(&malformed_path).expect("temporary fixture is removable");

    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid JSON for scenario"));
    assert!(!stderr.contains("panicked"));
    assert!(!stderr.contains("backtrace"));
}
