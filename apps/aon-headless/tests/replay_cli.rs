use aon_headless::{HeadlessError, load_package, load_replay, run_replay, run_replay_file};
use aon_sim::{
    HashCheckpoint, Replay, ReplayArtifact, ReplayError, Simulation, StateHash, Tick,
    encode_replay_artifact,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMPORARY_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn new() -> Self {
        let sequence = NEXT_TEMPORARY_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "aon-headless-replay-test-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("unique temporary directory is creatable");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct ReplayFixture {
    _temporary_directory: TemporaryDirectory,
    replay_path: PathBuf,
    package_path: PathBuf,
    expected_trace: Vec<StateHash>,
}

impl ReplayFixture {
    fn new(final_next_tick: u64, divergent_final_checkpoint: bool) -> Self {
        let temporary_directory = TemporaryDirectory::new();
        copy_package_artifacts(temporary_directory.path());

        let package_path = temporary_directory
            .path()
            .join("fixtures/scenarios/empty.json");
        let package = load_package(&package_path).expect("copied package loads");
        let mut simulation = Simulation::new(package).expect("empty simulation starts");
        let header = simulation.replay_header();
        let mut expected_trace = vec![simulation.state_hash()];
        for _ in 0..final_next_tick {
            expected_trace.push(
                simulation
                    .step(&[])
                    .expect("empty simulation advances")
                    .state_hash,
            );
        }

        let mut final_hash = *expected_trace.last().expect("trace has an initial state");
        if divergent_final_checkpoint && final_next_tick != 0 {
            final_hash = expected_trace[0];
            assert_ne!(final_hash, *expected_trace.last().unwrap());
        }
        let mut checkpoints = vec![HashCheckpoint {
            next_tick: Tick(0),
            state_hash: expected_trace[0],
        }];
        if final_next_tick != 0 {
            checkpoints.push(HashCheckpoint {
                next_tick: Tick(final_next_tick),
                state_hash: final_hash,
            });
        }

        let replay = Replay::new_v2(header, Vec::new(), Vec::new(), checkpoints)
            .expect("Replay shape is valid");
        let artifact = ReplayArtifact::new("../scenarios/empty.json", replay)
            .expect("portable relative Scenario path is valid");
        let replay_path = temporary_directory
            .path()
            .join("fixtures/replays/empty.replay.json");
        fs::write(
            &replay_path,
            encode_replay_artifact(&artifact).expect("Replay encodes"),
        )
        .expect("Replay fixture is writable");

        Self {
            _temporary_directory: temporary_directory,
            replay_path,
            package_path,
            expected_trace,
        }
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn copy_package_artifacts(destination_root: &Path) {
    for relative_path in [
        "fixtures/scenarios/empty.json",
        "profiles/numeric/v1.json",
        "profiles/physical-scale/stage0-alpha.json",
        "profiles/balance/stage0-alpha.json",
    ] {
        let source = workspace_root().join(relative_path);
        let destination = destination_root.join(relative_path);
        fs::create_dir_all(destination.parent().expect("artifact has a parent"))
            .expect("artifact directory is creatable");
        fs::copy(source, destination).expect("package artifact is copyable");
    }
    fs::create_dir_all(destination_root.join("fixtures/replays"))
        .expect("Replay directory is creatable");
}

#[test]
fn replay_file_resolves_scenario_relative_to_it_and_returns_a_complete_trace() {
    let fixture = ReplayFixture::new(3, false);

    let artifact = load_replay(&fixture.replay_path).expect("Replay decodes independently");
    assert_eq!(artifact.scenario_path(), "../scenarios/empty.json");

    let trace = run_replay_file(&fixture.replay_path).expect("Replay file runs");
    assert_eq!(trace.scenario_id(), "empty");
    assert_eq!(trace.completed_ticks(), 3);
    assert_eq!(trace.checkpoints(), fixture.expected_trace);
    assert_eq!(trace.checkpoints().len(), 4);
}

#[test]
fn replay_runner_constructs_a_fresh_simulation_and_validates_the_header() {
    let fixture = ReplayFixture::new(1, false);
    let artifact = load_replay(&fixture.replay_path).expect("Replay decodes");
    let package = load_package(&fixture.package_path).expect("package loads");

    let trace = run_replay(package.clone(), artifact.replay()).expect("matching Replay runs");
    assert_eq!(trace.checkpoints(), fixture.expected_trace);

    let mut wrong_header = *artifact.replay().header();
    wrong_header.initial_state_hash = fixture.expected_trace[1];
    let mismatched = Replay::new_v2(
        wrong_header,
        Vec::new(),
        Vec::new(),
        vec![HashCheckpoint {
            next_tick: Tick(0),
            state_hash: wrong_header.initial_state_hash,
        }],
    )
    .expect("mismatched Replay is structurally valid");
    assert!(matches!(
        run_replay(package, &mismatched),
        Err(HeadlessError::Replay(ReplayError::ContractMismatch { .. }))
    ));
}

#[test]
fn replay_runner_reports_the_first_checkpoint_divergence() {
    let fixture = ReplayFixture::new(1, true);

    assert!(matches!(
        run_replay_file(&fixture.replay_path),
        Err(HeadlessError::Replay(ReplayError::CheckpointDivergence {
            next_tick: Tick(1),
            ..
        }))
    ));
}

#[test]
fn replay_cli_runs_outside_the_workspace_and_matches_the_library_trace() {
    let fixture = ReplayFixture::new(3, false);
    let output = Command::new(env!("CARGO_BIN_EXE_aon-headless"))
        .args([
            "replay",
            fixture.replay_path.to_str().expect("path is UTF-8"),
        ])
        .current_dir(std::env::temp_dir())
        .output()
        .expect("headless process starts");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert_eq!(
        stdout,
        format!(
            "scenario = empty\ncompleted_ticks = 3\nstate_hash = {}\n",
            fixture.expected_trace[3]
        )
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn malformed_replay_has_a_typed_exit_code_without_a_panic() {
    let temporary_directory = TemporaryDirectory::new();
    let replay_path = temporary_directory.path().join("malformed.replay.json");
    fs::write(&replay_path, b"{").expect("malformed fixture is writable");

    let output = Command::new(env!("CARGO_BIN_EXE_aon-headless"))
        .args(["replay", replay_path.to_str().expect("path is UTF-8")])
        .output()
        .expect("headless process starts");

    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid Replay JSON"));
    assert!(!stderr.contains("panicked"));
    assert!(!stderr.contains("backtrace"));
}
