use aon_headless::{HeadlessError, materialize_experiment_plan};
use serde_json::Value;
use std::collections::BTreeMap;
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
            "aon-headless-experiment-test-{}-{sequence}",
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

fn retained_plan() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/experiments/s1-m0-physical-scale-v1.json")
        .canonicalize()
        .expect("retained Experiment Plan exists")
}

#[test]
fn retained_plan_materializes_eight_profiles_and_sixteen_unique_runs() {
    let temporary = TemporaryDirectory::new();
    let first = temporary.path().join("first");
    let second = temporary.path().join("second");

    let first_summary =
        materialize_experiment_plan(retained_plan(), &first).expect("retained plan resolves");
    let second_summary =
        materialize_experiment_plan(retained_plan(), &second).expect("second resolution succeeds");

    assert_eq!(
        first_summary.experiment_id(),
        second_summary.experiment_id()
    );
    assert_eq!(
        first_summary.physical_scale_profile_count(),
        second_summary.physical_scale_profile_count()
    );
    assert_eq!(first_summary.run_count(), second_summary.run_count());
    assert_eq!(first_summary.physical_scale_profile_count(), 8);
    assert_eq!(first_summary.run_count(), 16);
    assert_eq!(directory_bytes(&first), directory_bytes(&second));

    let manifest: Value = serde_json::from_slice(
        &fs::read(first.join("runs.json")).expect("run manifest is readable"),
    )
    .expect("run manifest is valid JSON");
    let profiles = manifest["physicalScaleProfiles"]
        .as_array()
        .expect("profile references are an array");
    let runs = manifest["runs"].as_array().expect("runs are an array");
    assert_eq!(profiles.len(), 8);
    assert_eq!(runs.len(), 16);

    let profile_hashes = profiles
        .iter()
        .map(|profile| profile["profileHash"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    let run_ids = runs
        .iter()
        .map(|run| run["runId"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(profile_hashes.len(), 8);
    assert_eq!(run_ids.len(), 16);
    for profile_hash in profile_hashes {
        assert_eq!(
            runs.iter()
                .filter(|run| run["physicalScaleProfileHash"] == profile_hash)
                .count(),
            2
        );
    }
}

#[test]
fn cli_resolves_plan_relative_paths_outside_the_workspace() {
    let temporary = TemporaryDirectory::new();
    let output_directory = temporary.path().join("materialized");
    let output = Command::new(env!("CARGO_BIN_EXE_aon-headless"))
        .args([
            "experiment-plan",
            retained_plan().to_str().expect("plan path is UTF-8"),
            "--output",
            output_directory
                .to_str()
                .expect("temporary output path is UTF-8"),
        ])
        .current_dir(std::env::temp_dir())
        .output()
        .expect("headless process starts");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is UTF-8"),
        format!(
            "experiment_id = s1-m0-physical-scale-v1\nphysical_scale_profiles = 8\nruns = 16\nmanifest = {}\n",
            output_directory.join("runs.json").display()
        )
    );
    assert!(output.stderr.is_empty());
    assert!(output_directory.join("runs.json").is_file());
}

#[test]
fn invalid_plan_has_typed_exit_and_publishes_nothing() {
    let temporary = TemporaryDirectory::new();
    let plan_path = temporary.path().join("invalid.json");
    let output_directory = temporary.path().join("must-not-exist");
    let malformed = fs::read_to_string(retained_plan())
        .expect("retained plan is readable")
        .replacen("\"maxTicks\": 4096", "\"maxTicks\": 4096.0", 1);
    fs::write(&plan_path, malformed).expect("invalid fixture is writable");

    let output = Command::new(env!("CARGO_BIN_EXE_aon-headless"))
        .args([
            "experiment-plan",
            plan_path.to_str().expect("plan path is UTF-8"),
            "--output",
            output_directory
                .to_str()
                .expect("temporary output path is UTF-8"),
        ])
        .output()
        .expect("headless process starts");

    assert_eq!(output.status.code(), Some(6));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid Experiment Plan JSON"));
    assert!(!output_directory.exists());
    assert!(staging_directories(temporary.path()).is_empty());
}

#[test]
fn existing_output_is_never_overwritten() {
    let temporary = TemporaryDirectory::new();
    let output_directory = temporary.path().join("existing");
    fs::create_dir(&output_directory).expect("output fixture directory is creatable");
    let sentinel = output_directory.join("sentinel.txt");
    fs::write(&sentinel, b"keep\n").expect("sentinel is writable");

    assert!(matches!(
        materialize_experiment_plan(retained_plan(), &output_directory),
        Err(HeadlessError::ExperimentOutputExists { .. })
    ));
    assert_eq!(fs::read(&sentinel).unwrap(), b"keep\n");
    assert_eq!(fs::read_dir(&output_directory).unwrap().count(), 1);
}

fn directory_bytes(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    collect_directory_bytes(root, root, &mut files);
    files
}

fn collect_directory_bytes(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
    let mut entries = fs::read_dir(directory)
        .expect("output directory is readable")
        .map(|entry| entry.expect("output entry is readable").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_directory_bytes(root, &path, files);
        } else {
            files.insert(
                path.strip_prefix(root).unwrap().to_path_buf(),
                fs::read(&path).expect("output file is readable"),
            );
        }
    }
}

fn staging_directories(parent: &Path) -> Vec<PathBuf> {
    fs::read_dir(parent)
        .expect("temporary directory is readable")
        .map(|entry| entry.expect("temporary entry is readable").path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().contains(".aon-stage-"))
        })
        .collect()
}
