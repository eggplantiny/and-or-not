#![forbid(unsafe_code)]

use aon_sim::{
    ArtifactBytes, PackageError, Simulation, SimulationError, SimulationPackage, StateHash,
    decode_package, decode_scenario_manifest,
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
    #[error("usage: aon-headless scenario <scenario-path> --ticks <non-negative-integer>")]
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
}

impl HeadlessError {
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Usage => 2,
            Self::File { .. } | Self::Package(_) => 3,
            Self::Simulation(_) => 4,
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

pub fn run_scenario(
    scenario_path: impl AsRef<Path>,
    ticks: u64,
) -> Result<RunTrace, HeadlessError> {
    let package = load_package(scenario_path)?;
    run_package(package, ticks)
}

fn read_artifact(path: &Path, artifact: &'static str) -> Result<Vec<u8>, HeadlessError> {
    fs::read(path).map_err(|error| HeadlessError::File {
        artifact,
        path: path.to_path_buf(),
        kind: FileErrorKind::from(error.kind()),
    })
}
