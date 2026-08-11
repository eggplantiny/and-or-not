#![forbid(unsafe_code)]

use aon_sim::{
    ArtifactBytes, PackageError, Replay, ReplayArtifact, ReplayError, Simulation, SimulationError,
    SimulationPackage, StateHash, decode_package, decode_replay_artifact, decode_scenario_manifest,
};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileErrorKind {
    NotFound,
    PermissionDenied,
    InvalidData,
    Other,
}

impl From<std::io::ErrorKind> for FileErrorKind {
    fn from(kind: std::io::ErrorKind) -> Self {
        match kind {
            std::io::ErrorKind::NotFound => Self::NotFound,
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            std::io::ErrorKind::InvalidData => Self::InvalidData,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Error)]
pub enum HeadlessError {
    #[error(
        "usage: aon-headless scenario <scenario-path> --ticks <non-negative-integer>\n       aon-headless replay <replay-path>"
    )]
    Usage,

    #[error("unable to read {artifact} `{path}`: {kind:?}")]
    File {
        artifact: &'static str,
        path: PathBuf,
        kind: FileErrorKind,
    },

    #[error(transparent)]
    Package(#[from] PackageError),

    #[error(transparent)]
    Simulation(#[from] SimulationError),

    #[error(transparent)]
    Replay(#[from] ReplayError),
}

impl HeadlessError {
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Usage => 2,
            Self::File { .. } | Self::Package(_) => 3,
            Self::Simulation(_) => 4,
            Self::Replay(_) => 5,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunTrace {
    scenario_id: String,
    completed_ticks: u64,
    checkpoints: Vec<StateHash>,
}

impl RunTrace {
    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }

    pub const fn completed_ticks(&self) -> u64 {
        self.completed_ticks
    }

    pub fn checkpoints(&self) -> &[StateHash] {
        &self.checkpoints
    }

    pub fn final_hash(&self) -> StateHash {
        self.checkpoints
            .last()
            .copied()
            .expect("a run trace always includes the initial state")
    }
}

pub fn load_package(scenario_path: impl AsRef<Path>) -> Result<SimulationPackage, HeadlessError> {
    let scenario_path = scenario_path.as_ref();
    let scenario_bytes = read_artifact(scenario_path, "scenario")?;
    let manifest = decode_scenario_manifest(&scenario_bytes)?;
    let base_directory = scenario_path.parent().unwrap_or_else(|| Path::new("."));

    let numeric_path = base_directory.join(manifest.profiles().numeric().path());
    let physical_scale_path = base_directory.join(manifest.profiles().physical_scale().path());
    let balance_path = base_directory.join(manifest.profiles().balance().path());

    let numeric_bytes = read_artifact(&numeric_path, "numeric profile")?;
    let physical_scale_bytes = read_artifact(&physical_scale_path, "physical-scale profile")?;
    let balance_bytes = read_artifact(&balance_path, "balance profile")?;

    decode_package(ArtifactBytes {
        scenario: &scenario_bytes,
        numeric_profile: &numeric_bytes,
        physical_scale_profile: &physical_scale_bytes,
        balance_profile: &balance_bytes,
    })
    .map_err(HeadlessError::from)
}

pub fn run_package(package: SimulationPackage, ticks: u64) -> Result<RunTrace, HeadlessError> {
    let mut simulation = Simulation::new(package)?;
    let scenario_id = simulation.scenario_id().to_owned();
    let capacity = usize::try_from(ticks)
        .ok()
        .and_then(|value| value.checked_add(1))
        .unwrap_or(0);
    let mut checkpoints = Vec::with_capacity(capacity);
    checkpoints.push(simulation.state_hash());

    for _ in 0..ticks {
        let report = simulation.step(&[])?;
        checkpoints.push(report.state_hash);
    }

    Ok(RunTrace {
        scenario_id,
        completed_ticks: ticks,
        checkpoints,
    })
}

/// Reads and strictly decodes a Replay artifact without loading its referenced Scenario.
pub fn load_replay(replay_path: impl AsRef<Path>) -> Result<ReplayArtifact, HeadlessError> {
    let replay_bytes = read_artifact(replay_path.as_ref(), "replay")?;
    decode_replay_artifact(&replay_bytes).map_err(HeadlessError::from)
}

/// Runs a decoded Replay against a package in a new, privately owned Simulation.
pub fn run_replay(package: SimulationPackage, replay: &Replay) -> Result<RunTrace, HeadlessError> {
    let mut simulation = Simulation::new(package)?;
    replay.validate_against(&simulation)?;

    let scenario_id = simulation.scenario_id().to_owned();
    let final_next_tick = replay.final_next_tick();
    let mut trace = Vec::new();
    trace.push(simulation.state_hash());

    let mut checkpoint_index = 0;
    verify_current_checkpoint(replay, &simulation, &mut checkpoint_index)?;

    while simulation.next_tick() < final_next_tick {
        let commands = replay
            .commands_for_tick(simulation.next_tick())
            .cloned()
            .collect::<Vec<_>>();
        let report = simulation.step(&commands)?;
        trace.push(report.state_hash);
        verify_current_checkpoint(replay, &simulation, &mut checkpoint_index)?;
    }

    replay.verify_trace(&trace)?;
    Ok(RunTrace {
        scenario_id,
        completed_ticks: final_next_tick.0,
        checkpoints: trace,
    })
}

/// Loads a Replay and its Scenario (relative to the Replay file), then executes it.
pub fn run_replay_file(replay_path: impl AsRef<Path>) -> Result<RunTrace, HeadlessError> {
    let replay_path = replay_path.as_ref();
    let artifact = load_replay(replay_path)?;
    let base_directory = replay_path.parent().unwrap_or_else(|| Path::new("."));
    let scenario_path = base_directory.join(artifact.scenario_path());
    let package = load_package(scenario_path)?;
    run_replay(package, artifact.replay())
}

pub fn run_scenario(
    scenario_path: impl AsRef<Path>,
    ticks: u64,
) -> Result<RunTrace, HeadlessError> {
    let package = load_package(scenario_path)?;
    run_package(package, ticks)
}

fn verify_current_checkpoint(
    replay: &Replay,
    simulation: &Simulation,
    checkpoint_index: &mut usize,
) -> Result<(), HeadlessError> {
    let Some(checkpoint) = replay.checkpoints().get(*checkpoint_index) else {
        return Ok(());
    };
    if checkpoint.next_tick != simulation.next_tick() {
        return Ok(());
    }

    let actual = simulation.state_hash();
    if actual != checkpoint.state_hash {
        return Err(ReplayError::CheckpointDivergence {
            next_tick: checkpoint.next_tick,
            expected: checkpoint.state_hash,
            actual,
        }
        .into());
    }
    *checkpoint_index += 1;
    Ok(())
}

fn read_artifact(path: &Path, artifact: &'static str) -> Result<Vec<u8>, HeadlessError> {
    fs::read(path).map_err(|error| HeadlessError::File {
        artifact,
        path: path.to_path_buf(),
        kind: FileErrorKind::from(error.kind()),
    })
}
