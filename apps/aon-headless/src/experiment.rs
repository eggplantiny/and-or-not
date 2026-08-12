use crate::{FileErrorKind, HeadlessError, read_artifact};
use aon_sim::{
    ExperimentArtifactBytes, ExperimentPlanArtifact, ExperimentRunSpec, ResolvedExperimentPlan,
    decode_experiment_plan_artifact, encode_physical_scale_profile,
    resolve_experiment_plan_artifact,
};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const EXPERIMENT_RUN_MANIFEST_SCHEMA_VERSION_V1: u32 = 1;

static NEXT_STAGING_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExperimentMaterializationSummary {
    experiment_id: String,
    physical_scale_profile_count: usize,
    run_count: usize,
    manifest_path: PathBuf,
}

impl ExperimentMaterializationSummary {
    pub fn experiment_id(&self) -> &str {
        &self.experiment_id
    }

    pub const fn physical_scale_profile_count(&self) -> usize {
        self.physical_scale_profile_count
    }

    pub const fn run_count(&self) -> usize {
        self.run_count
    }

    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }
}

pub fn materialize_experiment_plan(
    plan_path: impl AsRef<Path>,
    output_directory: impl AsRef<Path>,
) -> Result<ExperimentMaterializationSummary, HeadlessError> {
    let plan_path = plan_path.as_ref();
    let output_directory = output_directory.as_ref();
    let plan_bytes = read_artifact(plan_path, "experiment plan")?;
    let artifact = decode_experiment_plan_artifact(&plan_bytes)?;
    let base_directory = plan_path.parent().unwrap_or_else(|| Path::new("."));

    let scenario_bytes = read_reference(base_directory, artifact.scenario().path(), "scenario")?;
    let base_profile_bytes = read_reference(
        base_directory,
        artifact.base_physical_scale_profile().path(),
        "base physical-scale profile",
    )?;
    let numeric_profile_bytes = artifact
        .numeric_profiles()
        .iter()
        .map(|reference| read_reference(base_directory, reference.path(), "numeric profile"))
        .collect::<Result<Vec<_>, _>>()?;
    let balance_profile_bytes = artifact
        .balance_profiles()
        .iter()
        .map(|reference| read_reference(base_directory, reference.path(), "balance profile"))
        .collect::<Result<Vec<_>, _>>()?;
    let numeric_profile_slices = numeric_profile_bytes
        .iter()
        .map(Vec::as_slice)
        .collect::<Vec<_>>();
    let balance_profile_slices = balance_profile_bytes
        .iter()
        .map(Vec::as_slice)
        .collect::<Vec<_>>();
    let resolved = resolve_experiment_plan_artifact(
        &artifact,
        ExperimentArtifactBytes {
            scenario: &scenario_bytes,
            base_physical_scale_profile: &base_profile_bytes,
            numeric_profiles: &numeric_profile_slices,
            balance_profiles: &balance_profile_slices,
        },
    )?;

    let output = encode_materialized_experiment(&artifact, &resolved)?;
    publish_materialized_experiment(output_directory, &output)?;

    Ok(ExperimentMaterializationSummary {
        experiment_id: artifact.experiment_id().to_owned(),
        physical_scale_profile_count: resolved.physical_scale_profiles().len(),
        run_count: resolved.runs().len(),
        manifest_path: output_directory.join("runs.json"),
    })
}

struct EncodedExperiment {
    profiles: Vec<(String, Vec<u8>)>,
    manifest: Vec<u8>,
}

fn encode_materialized_experiment(
    artifact: &ExperimentPlanArtifact,
    resolved: &ResolvedExperimentPlan,
) -> Result<EncodedExperiment, HeadlessError> {
    let profiles = resolved
        .physical_scale_profiles()
        .iter()
        .map(|resolved_profile| {
            Ok((
                format!("{}.json", resolved_profile.profile_hash()),
                encode_physical_scale_profile(resolved_profile.profile())?,
            ))
        })
        .collect::<Result<Vec<_>, HeadlessError>>()?;
    let manifest = RunManifestWire {
        schema_version: EXPERIMENT_RUN_MANIFEST_SCHEMA_VERSION_V1,
        hash_algorithm_id: artifact.hash_algorithm_id().as_str(),
        experiment_id: artifact.experiment_id(),
        scenario_artifact_hash: artifact.scenario().artifact_hash().to_string(),
        physical_scale_profiles: resolved
            .physical_scale_profiles()
            .iter()
            .map(|profile| PhysicalScaleReferenceWire {
                profile_hash: profile.profile_hash().to_string(),
                path: format!("profiles/{}.json", profile.profile_hash()),
            })
            .collect(),
        runs: resolved.runs().iter().map(RunWire::from).collect(),
    };
    let mut manifest = serde_json::to_vec_pretty(&manifest)?;
    manifest.push(b'\n');
    Ok(EncodedExperiment { profiles, manifest })
}

fn publish_materialized_experiment(
    output_directory: &Path,
    output: &EncodedExperiment,
) -> Result<(), HeadlessError> {
    if output_directory.exists() {
        return Err(HeadlessError::ExperimentOutputExists {
            path: output_directory.to_path_buf(),
        });
    }
    let output_name = output_directory.file_name().ok_or_else(|| {
        HeadlessError::InvalidExperimentOutputDirectory {
            path: output_directory.to_path_buf(),
        }
    })?;
    let parent = output_directory
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| output_io_error("create output parent", parent, error))?;

    let staging_directory = create_staging_directory(parent, output_name)?;
    let publish_result = (|| {
        let profiles_directory = staging_directory.join("profiles");
        fs::create_dir(&profiles_directory).map_err(|error| {
            output_io_error(
                "create profile output directory",
                &profiles_directory,
                error,
            )
        })?;
        for (name, bytes) in &output.profiles {
            let path = profiles_directory.join(name);
            fs::write(&path, bytes)
                .map_err(|error| output_io_error("write physical-scale profile", &path, error))?;
        }
        let manifest_path = staging_directory.join("runs.json");
        fs::write(&manifest_path, &output.manifest)
            .map_err(|error| output_io_error("write run manifest", &manifest_path, error))?;
        fs::rename(&staging_directory, output_directory).map_err(|error| {
            output_io_error("publish experiment output", output_directory, error)
        })?;
        Ok(())
    })();
    if publish_result.is_err() {
        let _ = fs::remove_dir_all(&staging_directory);
    }
    publish_result
}

fn create_staging_directory(
    parent: &Path,
    output_name: &std::ffi::OsStr,
) -> Result<PathBuf, HeadlessError> {
    for _ in 0..128 {
        let sequence = NEXT_STAGING_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let name = format!(
            ".{}.aon-stage-{}-{sequence}",
            output_name.to_string_lossy(),
            std::process::id()
        );
        let path = parent.join(name);
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(output_io_error(
                    "create experiment staging directory",
                    &path,
                    error,
                ));
            }
        }
    }
    Err(HeadlessError::ExperimentOutputStagingExhausted {
        parent: parent.to_path_buf(),
    })
}

fn read_reference(
    base_directory: &Path,
    reference: &str,
    artifact: &'static str,
) -> Result<Vec<u8>, HeadlessError> {
    read_artifact(&base_directory.join(reference), artifact)
}

fn output_io_error(action: &'static str, path: &Path, error: std::io::Error) -> HeadlessError {
    HeadlessError::ExperimentOutputIo {
        action,
        path: path.to_path_buf(),
        kind: FileErrorKind::from(error.kind()),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunManifestWire<'a> {
    schema_version: u32,
    hash_algorithm_id: &'static str,
    experiment_id: &'a str,
    scenario_artifact_hash: String,
    physical_scale_profiles: Vec<PhysicalScaleReferenceWire>,
    runs: Vec<RunWire>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PhysicalScaleReferenceWire {
    profile_hash: String,
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunWire {
    run_id: String,
    scenario_artifact_hash: String,
    design_artifact_hash: String,
    semantics_version: String,
    numeric_profile_hash: String,
    physical_scale_profile_hash: String,
    balance_profile_hash: String,
    long_wire: LongWireWire,
    seed: String,
    max_ticks: u64,
    metric_set_id: String,
}

impl From<&ExperimentRunSpec> for RunWire {
    fn from(run: &ExperimentRunSpec) -> Self {
        let contract = run.contract();
        let design = run.design();
        Self {
            run_id: run.run_id().to_string(),
            scenario_artifact_hash: run.scenario_artifact_hash().to_string(),
            design_artifact_hash: run.design_artifact_hash().to_string(),
            semantics_version: contract.semantics_version.as_str().to_owned(),
            numeric_profile_hash: contract.numeric_profile_hash.to_string(),
            physical_scale_profile_hash: contract.physical_scale_profile_hash.to_string(),
            balance_profile_hash: contract.balance_profile_hash.to_string(),
            long_wire: LongWireWire {
                start: PointWire::from(design.start()),
                end: PointWire::from(design.end()),
            },
            seed: run.seed().to_string(),
            max_ticks: run.max_ticks(),
            metric_set_id: run.metric_set_id().to_owned(),
        }
    }
}

#[derive(Serialize)]
struct LongWireWire {
    start: PointWire,
    end: PointWire,
}

#[derive(Serialize)]
struct PointWire {
    x: i64,
    y: i64,
}

impl From<aon_sim::FixedVec2> for PointWire {
    fn from(point: aon_sim::FixedVec2) -> Self {
        Self {
            x: point.x.0,
            y: point.y.0,
        }
    }
}
