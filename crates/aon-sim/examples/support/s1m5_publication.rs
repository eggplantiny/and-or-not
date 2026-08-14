//! Filesystem publication and strict no-runtime verification for retained S1-M5 artifacts.
//!
//! The generator owns simulation and metric collection. This module accepts only their final
//! bytes, validates every cross-artifact reference before publication, atomically replaces each
//! retained file in `--write` mode, and then reads and verifies the checked-in bytes again.

use aon_sim::{
    ArtifactHash, ReferenceArchitectureArtifact, ReferenceArchitectureFormatVersion,
    ReferenceArchitecturePairManifest, ReferenceArchitectureRole, ReferenceDesignBinding,
    ReferenceExperimentPlanV2, ReferenceMetricArtifact, ReferenceMetricSetArtifact, ReplayArtifact,
    ScenarioManifest, decode_reference_architecture_artifact, decode_reference_experiment_plan_v2,
    decode_reference_metric_artifact, decode_reference_metric_set_artifact,
    decode_reference_pair_manifest, decode_replay_artifact, decode_scenario_manifest,
    encode_reference_architecture_artifact, encode_reference_experiment_plan_v2,
    encode_reference_metric_artifact, encode_reference_metric_set_artifact,
    encode_reference_pair_manifest, encode_replay_artifact,
    reference_architecture_command_log_hash, validate_reference_metric_bindings,
};
use std::fmt::Display;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const S1M5_SCENARIO_PATH: &str = "fixtures/scenarios/s1-m5-reference-architectures-v1.json";
pub const S1M5_BRUTE_DESIGN_PATH: &str = "fixtures/designs/s1-m5-brute-v2.json";
pub const S1M5_COMPUTED_DESIGN_PATH: &str = "fixtures/designs/s1-m5-computed-v2.json";
pub const S1M5_PAIR_PATH: &str = "fixtures/experiments/s1-m5-reference-pair-v1.json";
pub const S1M5_PLAN_PATH: &str = "fixtures/experiments/s1-m5-reference-plan-v2.json";
pub const S1M5_METRIC_SET_PATH: &str = "fixtures/metrics/s1-m5/reference-metric-set-v1.json";
pub const S1M5_BRUTE_REPLAY_PATH: &str = "fixtures/replays/s1-m5/brute-v1.json";
pub const S1M5_COMPUTED_REPLAY_PATH: &str = "fixtures/replays/s1-m5/computed-v1.json";
pub const S1M5_BRUTE_METRIC_PATH: &str = "fixtures/metrics/s1-m5/brute-v1.json";
pub const S1M5_COMPUTED_METRIC_PATH: &str = "fixtures/metrics/s1-m5/computed-v1.json";

const EXPECTED_PAIR_ID: &str = "s1-m5-reference-pair-v1";
const EXPECTED_EXPERIMENT_ID: &str = "s1-m5-reference-experiment-v2";
const EXPECTED_SCENARIO_ID: &str = "s1-m5-reference-architectures-v1";
const PAIR_SCENARIO_LOCATOR: &str = "../scenarios/s1-m5-reference-architectures-v1.json";
const PAIR_BRUTE_DESIGN_LOCATOR: &str = "../designs/s1-m5-brute-v2.json";
const PAIR_COMPUTED_DESIGN_LOCATOR: &str = "../designs/s1-m5-computed-v2.json";
const PLAN_PAIR_LOCATOR: &str = "s1-m5-reference-pair-v1.json";
const REPLAY_SCENARIO_LOCATOR: &str = "../../scenarios/s1-m5-reference-architectures-v1.json";

/// All generated retained bytes. The role-specific fields make accidental Brute/Computed swaps
/// impossible to hide behind artifact ordering.
#[derive(Clone, Copy)]
pub struct S1M5RetainedBytes<'a> {
    pub scenario: &'a [u8],
    pub brute_design: &'a [u8],
    pub computed_design: &'a [u8],
    pub pair: &'a [u8],
    pub plan: &'a [u8],
    pub metric_set: &'a [u8],
    pub brute_replay: &'a [u8],
    pub computed_replay: &'a [u8],
    pub brute_metric: &'a [u8],
    pub computed_metric: &'a [u8],
}

impl<'a> S1M5RetainedBytes<'a> {
    fn entries(self) -> [(&'static str, &'a [u8]); 10] {
        [
            (S1M5_SCENARIO_PATH, self.scenario),
            (S1M5_BRUTE_DESIGN_PATH, self.brute_design),
            (S1M5_COMPUTED_DESIGN_PATH, self.computed_design),
            (S1M5_PAIR_PATH, self.pair),
            (S1M5_PLAN_PATH, self.plan),
            (S1M5_METRIC_SET_PATH, self.metric_set),
            (S1M5_BRUTE_REPLAY_PATH, self.brute_replay),
            (S1M5_COMPUTED_REPLAY_PATH, self.computed_replay),
            (S1M5_BRUTE_METRIC_PATH, self.brute_metric),
            (S1M5_COMPUTED_METRIC_PATH, self.computed_metric),
        ]
    }
}

/// Stable semantic identities produced by the final verification pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct S1M5PublicationReport {
    pub scenario_hash: ArtifactHash,
    pub brute_design_hash: ArtifactHash,
    pub computed_design_hash: ArtifactHash,
    pub pair_hash: ArtifactHash,
    pub metric_set_hash: ArtifactHash,
    pub brute_metric_hash: ArtifactHash,
    pub computed_metric_hash: ArtifactHash,
}

#[derive(Debug, Error)]
pub enum S1M5PublicationError {
    #[error("unable to {action} `{path}`: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("retained artifact `{artifact}` is invalid: {message}")]
    InvalidArtifact {
        artifact: &'static str,
        message: String,
    },
    #[error("retained artifact `{artifact}` is not its canonical public re-encoding")]
    NonCanonicalEncoding { artifact: &'static str },
    #[error("checked-in artifact `{artifact}` differs from the deterministic generated bytes")]
    CheckedInBytesMismatch { artifact: &'static str },
    #[error("S1-M5 retained-artifact coherence failure: {0}")]
    Coherence(&'static str),
    #[error("publication failed: {primary}; rollback also failed: {rollback}")]
    RollbackFailed { primary: String, rollback: String },
    #[error("publication committed, but obsolete backup cleanup failed: {details}")]
    BackupCleanupFailed { details: String },
}

/// Validates `generated`, optionally stages and transactionally publishes the complete bundle,
/// then reads, byte-compares, and strictly validates the complete checked-in set.
pub fn publish_or_verify_s1m5(
    workspace_root: &Path,
    generated: S1M5RetainedBytes<'_>,
    write: bool,
) -> Result<S1M5PublicationReport, S1M5PublicationError> {
    let generated_report = validate_retained_bundle(generated)?;
    let publication = write
        .then(|| PublicationTransaction::stage_and_commit(workspace_root, generated))
        .transpose()?;

    let verification = verify_checked_bundle(workspace_root, generated, generated_report);
    match (publication, verification) {
        (None, result) => result,
        (Some(publication), Ok(report)) => {
            publication.finish()?;
            Ok(report)
        }
        (Some(publication), Err(primary)) => {
            let rollback = publication.rollback();
            Err(with_rollback(primary, rollback))
        }
    }
}

fn verify_checked_bundle(
    workspace_root: &Path,
    generated: S1M5RetainedBytes<'_>,
    generated_report: S1M5PublicationReport,
) -> Result<S1M5PublicationReport, S1M5PublicationError> {
    let checked = OwnedRetainedBytes::read(workspace_root)?;
    let checked_borrowed = checked.borrowed();
    for ((relative_path, expected), (_, actual)) in generated
        .entries()
        .into_iter()
        .zip(checked_borrowed.entries())
    {
        if actual != expected {
            return Err(S1M5PublicationError::CheckedInBytesMismatch {
                artifact: relative_path,
            });
        }
    }
    let checked_report = validate_retained_bundle(checked_borrowed)?;
    if checked_report != generated_report {
        return Err(S1M5PublicationError::Coherence(
            "checked-in semantic hashes differ from the pre-publication validation",
        ));
    }
    Ok(checked_report)
}

fn validate_retained_bundle(
    bytes: S1M5RetainedBytes<'_>,
) -> Result<S1M5PublicationReport, S1M5PublicationError> {
    let scenario = strict_scenario(bytes.scenario)?;
    let brute_design = strict_design(S1M5_BRUTE_DESIGN_PATH, bytes.brute_design)?;
    let computed_design = strict_design(S1M5_COMPUTED_DESIGN_PATH, bytes.computed_design)?;
    let pair = strict_pair(bytes.pair)?;
    let plan = strict_plan(bytes.plan)?;
    let metric_set = strict_metric_set(bytes.metric_set)?;
    let brute_replay = strict_replay(S1M5_BRUTE_REPLAY_PATH, bytes.brute_replay)?;
    let computed_replay = strict_replay(S1M5_COMPUTED_REPLAY_PATH, bytes.computed_replay)?;
    let brute_metric = strict_metric(S1M5_BRUTE_METRIC_PATH, bytes.brute_metric, &metric_set)?;
    let computed_metric = strict_metric(
        S1M5_COMPUTED_METRIC_PATH,
        bytes.computed_metric,
        &metric_set,
    )?;
    validate_paired_v2_designs(&brute_design, &computed_design)?;

    let scenario_hash = artifact_result(S1M5_SCENARIO_PATH, scenario.canonical_hash())?;
    let brute_design_hash = artifact_result(S1M5_BRUTE_DESIGN_PATH, brute_design.semantic_hash())?;
    let computed_design_hash =
        artifact_result(S1M5_COMPUTED_DESIGN_PATH, computed_design.semantic_hash())?;
    let pair_hash = artifact_result(S1M5_PAIR_PATH, pair.semantic_hash())?;
    let metric_set_hash = artifact_result(S1M5_METRIC_SET_PATH, metric_set.semantic_hash())?;

    ensure(
        scenario.scenario_id() == EXPECTED_SCENARIO_ID
            && pair.scenario_id() == EXPECTED_SCENARIO_ID
            && pair.pair_id() == EXPECTED_PAIR_ID
            && plan.experiment_id() == EXPECTED_EXPERIMENT_ID,
        "Scenario, Pair, or Experiment identity is not the frozen S1-M5 identity",
    )?;
    ensure(
        pair.scenario().path() == PAIR_SCENARIO_LOCATOR
            && pair.scenario().artifact_hash() == scenario_hash,
        "Pair Scenario locator/hash does not bind the retained Scenario",
    )?;
    ensure(
        pair.contract().semantics_version == scenario.semantics_version()
            && pair.contract().numeric_profile_hash == scenario.profiles().numeric().profile_hash()
            && pair.contract().physical_scale_profile_hash
                == scenario.profiles().physical_scale().profile_hash()
            && pair.contract().balance_profile_hash == scenario.profiles().balance().profile_hash(),
        "Pair contract does not bind the Scenario profile hashes",
    )?;
    ensure(
        profile_reference_matches(pair.numeric_profile(), scenario.profiles().numeric())
            && profile_reference_matches(
                pair.physical_scale_profile(),
                scenario.profiles().physical_scale(),
            )
            && profile_reference_matches(pair.balance_profile(), scenario.profiles().balance()),
        "Pair profile references do not exactly match the Scenario references",
    )?;

    validate_design_binding(
        &pair,
        ReferenceArchitectureRole::Brute,
        PAIR_BRUTE_DESIGN_LOCATOR,
        brute_design_hash,
    )?;
    validate_design_binding(
        &pair,
        ReferenceArchitectureRole::Computed,
        PAIR_COMPUTED_DESIGN_LOCATOR,
        computed_design_hash,
    )?;
    artifact_result(
        S1M5_METRIC_SET_PATH,
        validate_reference_metric_bindings(&pair, &metric_set, &brute_design, &computed_design),
    )?;
    ensure(
        pair.metric_set_id() == metric_set.metric_set_id()
            && pair.metric_set_hash() == metric_set_hash,
        "Pair Metric Set identity does not bind the retained Metric Set",
    )?;

    artifact_result(S1M5_PLAN_PATH, plan.validate_against_pair(&pair))?;
    ensure(
        plan.pair().path() == PLAN_PAIR_LOCATOR
            && plan.pair().artifact_hash() == pair_hash
            && plan.scenario() == pair.scenario()
            && plan.metric_set_id() == metric_set.metric_set_id()
            && plan.metric_set_hash() == metric_set_hash,
        "Experiment Plan locators or hashes do not bind the Pair/Scenario/Metric Set",
    )?;
    let runs = artifact_result(S1M5_PLAN_PATH, plan.resolve(&pair))?;

    validate_replay(&pair, ReferenceArchitectureRole::Brute, &brute_replay)?;
    validate_replay(&pair, ReferenceArchitectureRole::Computed, &computed_replay)?;
    ensure(
        brute_replay.replay().header() == computed_replay.replay().header()
            && brute_replay.replay().world_inputs() == computed_replay.replay().world_inputs()
            && brute_replay.replay().world_inputs().is_empty()
            && brute_replay.replay().final_next_tick()
                == computed_replay.replay().final_next_tick(),
        "Brute and Computed Replays do not share the exact initial header, empty input stream, and boundary",
    )?;

    validate_metric_for_role(
        &pair,
        &runs,
        ReferenceArchitectureRole::Brute,
        &brute_replay,
        &brute_metric,
    )?;
    validate_metric_for_role(
        &pair,
        &runs,
        ReferenceArchitectureRole::Computed,
        &computed_replay,
        &computed_metric,
    )?;
    let brute_metric_hash = artifact_result(
        S1M5_BRUTE_METRIC_PATH,
        brute_metric.semantic_hash(&metric_set),
    )?;
    let computed_metric_hash = artifact_result(
        S1M5_COMPUTED_METRIC_PATH,
        computed_metric.semantic_hash(&metric_set),
    )?;

    Ok(S1M5PublicationReport {
        scenario_hash,
        brute_design_hash,
        computed_design_hash,
        pair_hash,
        metric_set_hash,
        brute_metric_hash,
        computed_metric_hash,
    })
}

fn strict_scenario(bytes: &[u8]) -> Result<ScenarioManifest, S1M5PublicationError> {
    let scenario = artifact_result(S1M5_SCENARIO_PATH, decode_scenario_manifest(bytes))?;
    // Scenario manifests intentionally have no public typed encoder. A strict typed decode plus
    // lossless canonical JSON re-encoding still rejects alternate formatting and map ordering.
    let value: serde_json::Value =
        artifact_result(S1M5_SCENARIO_PATH, serde_json::from_slice(bytes))?;
    let mut encoded = artifact_result(S1M5_SCENARIO_PATH, serde_json::to_vec_pretty(&value))?;
    encoded.push(b'\n');
    ensure_canonical(S1M5_SCENARIO_PATH, bytes, &encoded)?;
    Ok(scenario)
}

fn strict_design(
    artifact: &'static str,
    bytes: &[u8],
) -> Result<ReferenceArchitectureArtifact, S1M5PublicationError> {
    let source = artifact_result(artifact, std::str::from_utf8(bytes))?;
    let design = artifact_result(artifact, decode_reference_architecture_artifact(source))?;
    let encoded = artifact_result(artifact, encode_reference_architecture_artifact(&design))?;
    ensure_canonical(artifact, bytes, encoded.as_bytes())?;
    Ok(design)
}

fn validate_paired_v2_designs(
    brute: &ReferenceArchitectureArtifact,
    computed: &ReferenceArchitectureArtifact,
) -> Result<(), S1M5PublicationError> {
    ensure(
        brute.format_version == ReferenceArchitectureFormatVersion::V2
            && computed.format_version == ReferenceArchitectureFormatVersion::V2,
        "retained -v2 design paths must contain Reference Architecture v2 artifacts",
    )?;
    let brute_stage_count = brute
        .materialization_schedule
        .as_ref()
        .map(|schedule| schedule.binding_batches.len());
    let computed_stage_count = computed
        .materialization_schedule
        .as_ref()
        .map(|schedule| schedule.binding_batches.len());
    ensure(
        matches!(
            (brute_stage_count, computed_stage_count),
            (Some(brute_count), Some(computed_count))
                if brute_count > 0 && brute_count == computed_count
        ),
        "retained Brute and Computed v2 designs must share one nonempty binding-stage grammar",
    )
}

fn strict_pair(bytes: &[u8]) -> Result<ReferenceArchitecturePairManifest, S1M5PublicationError> {
    let pair = artifact_result(S1M5_PAIR_PATH, decode_reference_pair_manifest(bytes))?;
    let encoded = artifact_result(S1M5_PAIR_PATH, encode_reference_pair_manifest(&pair))?;
    ensure_canonical(S1M5_PAIR_PATH, bytes, &encoded)?;
    Ok(pair)
}

fn strict_plan(bytes: &[u8]) -> Result<ReferenceExperimentPlanV2, S1M5PublicationError> {
    let plan = artifact_result(S1M5_PLAN_PATH, decode_reference_experiment_plan_v2(bytes))?;
    let encoded = artifact_result(S1M5_PLAN_PATH, encode_reference_experiment_plan_v2(&plan))?;
    ensure_canonical(S1M5_PLAN_PATH, bytes, &encoded)?;
    Ok(plan)
}

fn strict_metric_set(bytes: &[u8]) -> Result<ReferenceMetricSetArtifact, S1M5PublicationError> {
    let metric_set = artifact_result(
        S1M5_METRIC_SET_PATH,
        decode_reference_metric_set_artifact(bytes),
    )?;
    let encoded = artifact_result(
        S1M5_METRIC_SET_PATH,
        encode_reference_metric_set_artifact(&metric_set),
    )?;
    ensure_canonical(S1M5_METRIC_SET_PATH, bytes, &encoded)?;
    Ok(metric_set)
}

fn strict_replay(
    artifact: &'static str,
    bytes: &[u8],
) -> Result<ReplayArtifact, S1M5PublicationError> {
    let replay = artifact_result(artifact, decode_replay_artifact(bytes))?;
    let encoded = artifact_result(artifact, encode_replay_artifact(&replay))?;
    ensure_canonical(artifact, bytes, &encoded)?;
    ensure(
        replay.scenario_path() == REPLAY_SCENARIO_LOCATOR,
        "Replay scenario locator is not the exact frozen relative path",
    )?;
    Ok(replay)
}

fn strict_metric(
    artifact: &'static str,
    bytes: &[u8],
    metric_set: &ReferenceMetricSetArtifact,
) -> Result<ReferenceMetricArtifact, S1M5PublicationError> {
    let metric = artifact_result(
        artifact,
        decode_reference_metric_artifact(bytes, metric_set),
    )?;
    let encoded = artifact_result(
        artifact,
        encode_reference_metric_artifact(&metric, metric_set),
    )?;
    ensure_canonical(artifact, bytes, &encoded)?;
    Ok(metric)
}

fn validate_design_binding(
    pair: &ReferenceArchitecturePairManifest,
    role: ReferenceArchitectureRole,
    expected_path: &'static str,
    expected_hash: ArtifactHash,
) -> Result<(), S1M5PublicationError> {
    let binding = design_binding(pair, role)?;
    ensure(
        binding.design.path() == expected_path && binding.design.artifact_hash() == expected_hash,
        "Pair design locator/hash does not bind the role-specific retained design",
    )
}

fn validate_replay(
    pair: &ReferenceArchitecturePairManifest,
    role: ReferenceArchitectureRole,
    replay: &ReplayArtifact,
) -> Result<(), S1M5PublicationError> {
    let binding = design_binding(pair, role)?;
    let value = replay.replay();
    let header = value.header();
    ensure(
        header.semantics_version == pair.contract().semantics_version
            && header.numeric_profile_hash == pair.contract().numeric_profile_hash
            && header.physical_scale_profile_hash == pair.contract().physical_scale_profile_hash
            && header.balance_profile_hash == pair.contract().balance_profile_hash
            && header.seed == pair.seed()
            && header.hash_algorithm_id == pair.hash_algorithm_id(),
        "Replay Header does not bind the Pair contract and Seed",
    )?;
    ensure(
        value.final_next_tick() == pair.max_ticks(),
        "Replay final boundary does not equal Pair maxTicks",
    )?;
    let command_log_hash = artifact_result(
        role_artifact(role, S1M5_BRUTE_REPLAY_PATH, S1M5_COMPUTED_REPLAY_PATH),
        reference_architecture_command_log_hash(value.commands()),
    )?;
    ensure(
        command_log_hash == binding.command_log_hash,
        "Replay Command Log hash does not equal the Pair design binding",
    )
}

fn validate_metric_for_role(
    pair: &ReferenceArchitecturePairManifest,
    runs: &[aon_sim::ReferenceExperimentRunV2; 2],
    role: ReferenceArchitectureRole,
    replay: &ReplayArtifact,
    metric: &ReferenceMetricArtifact,
) -> Result<(), S1M5PublicationError> {
    let binding = design_binding(pair, role)?;
    let run = runs
        .iter()
        .find(|candidate| candidate.design.role == role)
        .ok_or(S1M5PublicationError::Coherence(
            "Experiment Plan did not resolve exactly one run for each role",
        ))?;
    let boundaries = metric.result.boundaries;
    ensure(
        metric.run_id == run.run_id
            && metric.metric_set_id == pair.metric_set_id()
            && metric.metric_set_hash == pair.metric_set_hash(),
        "Metric Artifact does not bind the role-specific Run ID and Metric Set",
    )?;
    ensure(
        boundaries.build_end_tick == pair.build_end_tick()
            && boundaries.measurement_start_tick == pair.measurement_start_tick()
            && boundaries.max_ticks == pair.max_ticks()
            && boundaries.final_next_tick == replay.replay().final_next_tick(),
        "Metric boundaries do not equal the Pair and Replay boundaries",
    )?;
    ensure(
        metric.result.static_inventory.command_log_hash == binding.command_log_hash,
        "Metric static inventory Command Log hash does not equal the Pair design binding",
    )
}

fn design_binding(
    pair: &ReferenceArchitecturePairManifest,
    role: ReferenceArchitectureRole,
) -> Result<&ReferenceDesignBinding, S1M5PublicationError> {
    pair.designs()
        .iter()
        .find(|binding| binding.role == role)
        .ok_or(S1M5PublicationError::Coherence(
            "Pair does not contain exactly the required Brute and Computed roles",
        ))
}

fn profile_reference_matches(
    pair: &aon_sim::ReferenceProfileReference,
    scenario: &aon_sim::ProfileReference,
) -> bool {
    pair.path() == scenario.path()
        && pair.profile_id() == scenario.profile_id()
        && pair.profile_hash() == scenario.profile_hash()
}

fn role_artifact(
    role: ReferenceArchitectureRole,
    brute: &'static str,
    computed: &'static str,
) -> &'static str {
    match role {
        ReferenceArchitectureRole::Brute => brute,
        ReferenceArchitectureRole::Computed => computed,
    }
}

fn ensure(value: bool, message: &'static str) -> Result<(), S1M5PublicationError> {
    if value {
        Ok(())
    } else {
        Err(S1M5PublicationError::Coherence(message))
    }
}

fn ensure_canonical(
    artifact: &'static str,
    source: &[u8],
    encoded: &[u8],
) -> Result<(), S1M5PublicationError> {
    if source == encoded {
        Ok(())
    } else {
        Err(S1M5PublicationError::NonCanonicalEncoding { artifact })
    }
}

fn artifact_result<T, E: Display>(
    artifact: &'static str,
    result: Result<T, E>,
) -> Result<T, S1M5PublicationError> {
    result.map_err(|error| S1M5PublicationError::InvalidArtifact {
        artifact,
        message: error.to_string(),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DestinationDisposition {
    Unchanged,
    New,
    Existing,
}

struct PublicationPlan<'a> {
    relative_path: &'static str,
    destination: PathBuf,
    bytes: &'a [u8],
    disposition: DestinationDisposition,
}

struct PublicationEntry {
    relative_path: &'static str,
    destination: PathBuf,
    disposition: DestinationDisposition,
    staged: Option<PathBuf>,
    backup: Option<PathBuf>,
    committed: bool,
}

struct PublicationTransaction {
    entries: Vec<PublicationEntry>,
}

impl PublicationTransaction {
    fn stage_and_commit(
        workspace_root: &Path,
        generated: S1M5RetainedBytes<'_>,
    ) -> Result<Self, S1M5PublicationError> {
        // Preflight every destination before allocating any temporary file or moving any retained
        // artifact. Exact existing bytes are intentionally left untouched, preserving timestamps
        // and allowing repeatable Windows `--write` runs.
        let mut plans = Vec::with_capacity(10);
        for (relative_path, bytes) in generated.entries() {
            let destination = workspace_root.join(relative_path);
            let parent = destination.parent().ok_or(S1M5PublicationError::Coherence(
                "retained path has no parent directory",
            ))?;
            fs::create_dir_all(parent).map_err(|source| S1M5PublicationError::Io {
                action: "create parent directory for",
                path: PathBuf::from(relative_path),
                source,
            })?;
            let disposition = match fs::read(&destination) {
                Ok(existing) if existing == bytes => DestinationDisposition::Unchanged,
                Ok(_) => DestinationDisposition::Existing,
                Err(source) if source.kind() == io::ErrorKind::NotFound => {
                    DestinationDisposition::New
                }
                Err(source) => {
                    return Err(S1M5PublicationError::Io {
                        action: "prevalidate existing artifact",
                        path: PathBuf::from(relative_path),
                        source,
                    });
                }
            };
            plans.push(PublicationPlan {
                relative_path,
                destination,
                bytes,
                disposition,
            });
        }

        // Stage every changed member before the first destination move. Consequently, disk-full,
        // access, and serialization-size failures cannot leave a partially published bundle.
        let mut transaction = Self {
            entries: Vec::with_capacity(plans.len()),
        };
        for (ordinal, plan) in plans.into_iter().enumerate() {
            let staged = if plan.disposition == DestinationDisposition::Unchanged {
                None
            } else {
                match stage_file(&plan.destination, plan.relative_path, ordinal, plan.bytes) {
                    Ok(staged) => Some(staged),
                    Err(primary) => {
                        return Err(with_rollback(primary, transaction.rollback()));
                    }
                }
            };
            transaction.entries.push(PublicationEntry {
                relative_path: plan.relative_path,
                destination: plan.destination,
                disposition: plan.disposition,
                staged,
                backup: None,
                committed: false,
            });
        }

        for index in 0..transaction.entries.len() {
            if let Err(primary) = transaction.commit_entry(index) {
                return Err(with_rollback(primary, transaction.rollback()));
            }
        }
        Ok(transaction)
    }

    fn commit_entry(&mut self, index: usize) -> Result<(), S1M5PublicationError> {
        let entry = &mut self.entries[index];
        if entry.disposition == DestinationDisposition::Unchanged {
            return Ok(());
        }

        if entry.disposition == DestinationDisposition::Existing {
            let backup =
                unused_sidecar_path(&entry.destination, entry.relative_path, index, "backup")?;
            fs::rename(&entry.destination, &backup).map_err(|source| S1M5PublicationError::Io {
                action: "move existing artifact to transaction backup",
                path: PathBuf::from(entry.relative_path),
                source,
            })?;
            entry.backup = Some(backup);
        } else if entry.destination.exists() {
            return Err(S1M5PublicationError::Coherence(
                "new retained destination appeared during publication",
            ));
        }

        let staged = entry
            .staged
            .as_ref()
            .ok_or(S1M5PublicationError::Coherence(
                "changed retained artifact has no staged file",
            ))?;
        fs::rename(staged, &entry.destination).map_err(|source| S1M5PublicationError::Io {
            action: "commit staged artifact",
            path: PathBuf::from(entry.relative_path),
            source,
        })?;
        entry.staged = None;
        entry.committed = true;
        Ok(())
    }

    /// Restores prior destinations in reverse commit order. A failed restoration deliberately
    /// leaves its same-directory backup in place and reports that exact path for manual recovery.
    fn rollback(mut self) -> Vec<String> {
        let mut failures = Vec::new();
        for entry in self.entries.iter_mut().rev() {
            if entry.committed {
                if let Some(backup) = entry.backup.take() {
                    match remove_file_if_exists(&entry.destination) {
                        Ok(()) => {
                            if let Err(source) = fs::rename(&backup, &entry.destination) {
                                failures.push(format!(
                                    "restore `{}` from `{}`: {source}",
                                    entry.relative_path,
                                    backup.display()
                                ));
                            }
                        }
                        Err(source) => failures.push(format!(
                            "remove newly committed `{}` before restore: {source}",
                            entry.relative_path
                        )),
                    }
                } else if let Err(source) = remove_file_if_exists(&entry.destination) {
                    failures.push(format!(
                        "remove newly created `{}`: {source}",
                        entry.relative_path
                    ));
                }
            } else if let Some(backup) = entry.backup.take()
                && let Err(source) = fs::rename(&backup, &entry.destination)
            {
                failures.push(format!(
                    "restore uncommitted `{}` from `{}`: {source}",
                    entry.relative_path,
                    backup.display()
                ));
            }
        }
        for entry in &mut self.entries {
            if let Some(staged) = entry.staged.take()
                && let Err(source) = remove_file_if_exists(&staged)
            {
                failures.push(format!(
                    "remove staged file `{}` for `{}`: {source}",
                    staged.display(),
                    entry.relative_path
                ));
            }
        }
        failures
    }

    /// Removes backups only after the freshly checked-in bundle has been reread and strictly
    /// verified. Until then, every replaced destination remains recoverable.
    fn finish(mut self) -> Result<(), S1M5PublicationError> {
        let mut failures = Vec::new();
        for entry in &mut self.entries {
            if let Some(backup) = entry.backup.take()
                && let Err(source) = remove_file_if_exists(&backup)
            {
                failures.push(format!(
                    "remove `{}` for `{}`: {source}",
                    backup.display(),
                    entry.relative_path
                ));
            }
            if let Some(staged) = entry.staged.take()
                && let Err(source) = remove_file_if_exists(&staged)
            {
                failures.push(format!(
                    "remove unexpected staged file `{}` for `{}`: {source}",
                    staged.display(),
                    entry.relative_path
                ));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(S1M5PublicationError::BackupCleanupFailed {
                details: failures.join("; "),
            })
        }
    }
}

fn stage_file(
    destination: &Path,
    relative_path: &'static str,
    ordinal: usize,
    bytes: &[u8],
) -> Result<PathBuf, S1M5PublicationError> {
    let parent = destination.parent().ok_or(S1M5PublicationError::Coherence(
        "retained path has no parent directory",
    ))?;
    let file_name = destination_file_name(destination)?;
    let mut selected = None;
    for attempt in 0_u16..=u16::MAX {
        let candidate = parent.join(format!(
            ".{file_name}.s1m5-stage-{}-{ordinal}-{attempt}",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                selected = Some((candidate, file));
                break;
            }
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(S1M5PublicationError::Io {
                    action: "create staged publication file for",
                    path: PathBuf::from(relative_path),
                    source,
                });
            }
        }
    }
    let (staged_path, mut staged) = selected.ok_or(S1M5PublicationError::Coherence(
        "unable to allocate a unique staged publication file",
    ))?;
    let write_result = staged.write_all(bytes).and_then(|()| staged.sync_all());
    drop(staged);
    if let Err(source) = write_result {
        let _ = fs::remove_file(&staged_path);
        return Err(S1M5PublicationError::Io {
            action: "write staged publication file for",
            path: PathBuf::from(relative_path),
            source,
        });
    }
    Ok(staged_path)
}

fn unused_sidecar_path(
    destination: &Path,
    relative_path: &'static str,
    ordinal: usize,
    kind: &'static str,
) -> Result<PathBuf, S1M5PublicationError> {
    let parent = destination.parent().ok_or(S1M5PublicationError::Coherence(
        "retained path has no parent directory",
    ))?;
    let file_name = destination_file_name(destination)?;
    for attempt in 0_u16..=u16::MAX {
        let candidate = parent.join(format!(
            ".{file_name}.s1m5-{kind}-{}-{ordinal}-{attempt}",
            std::process::id()
        ));
        match fs::symlink_metadata(&candidate) {
            Ok(_) => {}
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(candidate),
            Err(source) => {
                return Err(S1M5PublicationError::Io {
                    action: "inspect transaction sidecar for",
                    path: PathBuf::from(relative_path),
                    source,
                });
            }
        }
    }
    Err(S1M5PublicationError::Coherence(
        "unable to allocate a unique publication sidecar path",
    ))
}

fn destination_file_name(destination: &Path) -> Result<&str, S1M5PublicationError> {
    destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(S1M5PublicationError::Coherence(
            "retained path has no UTF-8 file name",
        ))
}

fn remove_file_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(source),
    }
}

fn with_rollback(primary: S1M5PublicationError, rollback: Vec<String>) -> S1M5PublicationError {
    if rollback.is_empty() {
        primary
    } else {
        S1M5PublicationError::RollbackFailed {
            primary: primary.to_string(),
            rollback: rollback.join("; "),
        }
    }
}

struct OwnedRetainedBytes {
    scenario: Vec<u8>,
    brute_design: Vec<u8>,
    computed_design: Vec<u8>,
    pair: Vec<u8>,
    plan: Vec<u8>,
    metric_set: Vec<u8>,
    brute_replay: Vec<u8>,
    computed_replay: Vec<u8>,
    brute_metric: Vec<u8>,
    computed_metric: Vec<u8>,
}

impl OwnedRetainedBytes {
    fn read(workspace_root: &Path) -> Result<Self, S1M5PublicationError> {
        let read = |relative_path: &'static str| {
            fs::read(workspace_root.join(relative_path)).map_err(|source| {
                S1M5PublicationError::Io {
                    action: "read checked-in artifact",
                    path: PathBuf::from(relative_path),
                    source,
                }
            })
        };
        Ok(Self {
            scenario: read(S1M5_SCENARIO_PATH)?,
            brute_design: read(S1M5_BRUTE_DESIGN_PATH)?,
            computed_design: read(S1M5_COMPUTED_DESIGN_PATH)?,
            pair: read(S1M5_PAIR_PATH)?,
            plan: read(S1M5_PLAN_PATH)?,
            metric_set: read(S1M5_METRIC_SET_PATH)?,
            brute_replay: read(S1M5_BRUTE_REPLAY_PATH)?,
            computed_replay: read(S1M5_COMPUTED_REPLAY_PATH)?,
            brute_metric: read(S1M5_BRUTE_METRIC_PATH)?,
            computed_metric: read(S1M5_COMPUTED_METRIC_PATH)?,
        })
    }

    fn borrowed(&self) -> S1M5RetainedBytes<'_> {
        S1M5RetainedBytes {
            scenario: &self.scenario,
            brute_design: &self.brute_design,
            computed_design: &self.computed_design,
            pair: &self.pair,
            plan: &self.plan,
            metric_set: &self.metric_set,
            brute_replay: &self.brute_replay,
            computed_replay: &self.computed_replay,
            brute_metric: &self.brute_metric,
            computed_metric: &self.computed_metric,
        }
    }
}
