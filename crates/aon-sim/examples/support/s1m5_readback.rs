//! Executable, read-only verification of the checked-in S1-M5 retained bundle.
//!
//! Publication verification proves that the ten JSON artifacts are strict, canonical, and
//! mutually bound. This module closes the executable half of that contract: it rematerializes
//! both decoded designs, executes the retained Replay command/checkpoint streams, independently
//! observes that their paired final barrier is signal-quiescent, and rebuilds both metric
//! artifacts from fresh reports. No function in this module writes to the workspace.

use super::s1m5_publication::{
    S1M5_BRUTE_DESIGN_PATH, S1M5_BRUTE_METRIC_PATH, S1M5_BRUTE_REPLAY_PATH,
    S1M5_COMPUTED_DESIGN_PATH, S1M5_COMPUTED_METRIC_PATH, S1M5_COMPUTED_REPLAY_PATH,
    S1M5_METRIC_SET_PATH, S1M5_PAIR_PATH, S1M5_PLAN_PATH, S1M5_SCENARIO_PATH, S1M5PublicationError,
    S1M5PublicationReport, S1M5RetainedBytes, publish_or_verify_s1m5,
};
use super::s1m5_trace::{S1M5TraceBuildError, S1M5TraceBuildInput, build_s1m5_trace_artifacts};
use aon_sim::{
    ArtifactBytes, MaterializedReferenceArchitecture, ReferenceArchitectureArtifact,
    ReferenceArchitectureError, ReferenceArchitectureRole, ReferenceArchitectureScenarioResolution,
    ReferenceExperimentError, ReferenceExperimentRunV2, ReferenceMetricArtifact,
    ReferenceMetricBoundaries, ReferenceMetricError, ReferenceMetricSetArtifact, ReplayArtifact,
    ReplayError, RunStatus, ScenarioManifest, Simulation, SimulationError, SimulationPackage,
    StateHash, Tick, decode_package, decode_reference_architecture_artifact,
    decode_reference_experiment_plan_v2, decode_reference_metric_artifact,
    decode_reference_metric_set_artifact, decode_reference_pair_manifest, decode_replay_artifact,
    decode_scenario_manifest, materialize_reference_architecture_pair,
};
use std::fmt::Display;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Profile bytes used to construct the package around the checked-in Scenario.
#[derive(Clone, Copy)]
pub struct S1M5ReadbackProfileBytes<'a> {
    pub numeric: &'a [u8],
    pub physical_scale: &'a [u8],
    pub balance: &'a [u8],
}

/// Stable boundaries and terminal Replay hashes returned by executable readback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct S1M5ReadbackReport {
    pub publication: S1M5PublicationReport,
    pub build_end_tick: Tick,
    pub measurement_start_tick: Tick,
    pub max_ticks: Tick,
    pub brute_final_state_hash: StateHash,
    pub computed_final_state_hash: StateHash,
    pub brute_checkpoint_count: usize,
    pub computed_checkpoint_count: usize,
}

#[derive(Debug, Error)]
pub enum S1M5ReadbackError {
    #[error("unable to read checked-in artifact `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Package(#[from] aon_sim::PackageError),
    #[error(transparent)]
    Publication(#[from] S1M5PublicationError),
    #[error(transparent)]
    Architecture(#[from] ReferenceArchitectureError),
    #[error(transparent)]
    Experiment(#[from] ReferenceExperimentError),
    #[error(transparent)]
    Metric(#[from] ReferenceMetricError),
    #[error(transparent)]
    Replay(#[from] ReplayError),
    #[error(transparent)]
    Simulation(#[from] SimulationError),
    #[error(transparent)]
    Trace(#[from] S1M5TraceBuildError),
    #[error("checked-in artifact `{artifact}` is invalid: {message}")]
    InvalidArtifact {
        artifact: &'static str,
        message: String,
    },
    #[error("S1-M5 executable readback coherence failure: {0}")]
    Coherence(&'static str),
    #[error("the {role:?} retained Replay command stream differs from rematerialization")]
    ReplayCommandMismatch { role: ReferenceArchitectureRole },
    #[error("the {role:?} Replay materialization acceptances diverged at {tick:?}")]
    ReplayAcceptanceMismatch {
        role: ReferenceArchitectureRole,
        tick: Tick,
    },
    #[error("the rebuilt {role:?} {artifact} differs from the exact checked-in artifact")]
    RebuiltArtifactMismatch {
        role: ReferenceArchitectureRole,
        artifact: &'static str,
    },
}

/// Strictly builds the package from the checked-in Scenario plus supplied Profile bytes, then
/// performs the complete executable readback verification.
pub fn verify_checked_s1m5_bundle_with_profiles(
    workspace_root: &Path,
    profiles: S1M5ReadbackProfileBytes<'_>,
) -> Result<S1M5ReadbackReport, S1M5ReadbackError> {
    let checked = CheckedRetainedBytes::read(workspace_root)?;
    let package = decode_package(ArtifactBytes {
        scenario: &checked.scenario,
        numeric_profile: profiles.numeric,
        physical_scale_profile: profiles.physical_scale,
        balance_profile: profiles.balance,
    })?;
    verify_loaded_bundle(workspace_root, &package, &checked)
}

fn verify_loaded_bundle(
    workspace_root: &Path,
    package: &SimulationPackage,
    checked: &CheckedRetainedBytes,
) -> Result<S1M5ReadbackReport, S1M5ReadbackError> {
    // Reuse the publication layer for all ten strict decode/canonical encoding and locator/hash
    // checks. Passing the bytes just read as the expected set keeps this pass read-only while its
    // second read also detects a bundle that changed during verification.
    let publication = publish_or_verify_s1m5(workspace_root, checked.borrowed(), false)?;
    let decoded = DecodedRetainedBundle::decode(checked)?;
    validate_package_identity(package, &decoded.scenario)?;

    let pristine = Simulation::new(package.clone())?;
    let scenario_resolution = scenario_resolution(&pristine)?;
    let (
        (brute_simulation, brute_materialization),
        (computed_simulation, computed_materialization),
    ) = materialize_reference_architecture_pair(
        (
            Simulation::new(package.clone())?,
            &decoded.brute_design,
            &scenario_resolution,
        ),
        (
            Simulation::new(package.clone())?,
            &decoded.computed_design,
            &scenario_resolution,
        ),
    )?;
    if brute_materialization.build_end_tick != computed_materialization.build_end_tick {
        return Err(S1M5ReadbackError::Coherence(
            "decoded design materializations do not share the lockstep buildEndTick",
        ));
    }
    let common_build_end = brute_materialization.build_end_tick;
    if brute_simulation.next_tick() != common_build_end
        || computed_simulation.next_tick() != common_build_end
        || common_build_end != decoded.pair.build_end_tick()
    {
        return Err(S1M5ReadbackError::Coherence(
            "decoded design materialization does not reproduce Pair buildEndTick",
        ));
    }
    let brute_quiescence = brute_simulation.signal_quiescence_snapshot()?;
    let computed_quiescence = computed_simulation.signal_quiescence_snapshot()?;
    if !brute_quiescence.is_quiescent() || !computed_quiescence.is_quiescent() {
        return Err(S1M5ReadbackError::Coherence(
            "decoded paired v2 final barrier is not quiescent at buildEndTick",
        ));
    }
    let measurement_start_tick = common_build_end;
    if measurement_start_tick != decoded.pair.measurement_start_tick() {
        return Err(S1M5ReadbackError::Coherence(
            "Pair measurementStartTick does not equal the observed final-barrier buildEndTick",
        ));
    }

    validate_replay_commands(
        ReferenceArchitectureRole::Brute,
        &decoded.brute_replay,
        &brute_materialization,
    )?;
    validate_replay_commands(
        ReferenceArchitectureRole::Computed,
        &decoded.computed_replay,
        &computed_materialization,
    )?;
    let brute_execution = execute_retained_replay(
        package,
        ReferenceArchitectureRole::Brute,
        &decoded.brute_replay,
        &brute_materialization,
    )?;
    let computed_execution = execute_retained_replay(
        package,
        ReferenceArchitectureRole::Computed,
        &decoded.computed_replay,
        &computed_materialization,
    )?;

    let runs = decoded.plan.resolve(&decoded.pair)?;
    let boundaries = ReferenceMetricBoundaries {
        build_end_tick: common_build_end,
        measurement_start_tick,
        max_ticks: decoded.pair.max_ticks(),
    };
    boundaries.validate()?;
    rebuild_role_artifacts(
        package,
        &decoded.scenario,
        &scenario_resolution,
        &decoded.metric_set,
        &runs,
        boundaries,
        ReferenceArchitectureRole::Brute,
        &decoded.brute_design,
        &brute_materialization,
        &decoded.brute_replay,
        &decoded.brute_metric,
        &checked.brute_replay,
        &checked.brute_metric,
    )?;
    rebuild_role_artifacts(
        package,
        &decoded.scenario,
        &scenario_resolution,
        &decoded.metric_set,
        &runs,
        boundaries,
        ReferenceArchitectureRole::Computed,
        &decoded.computed_design,
        &computed_materialization,
        &decoded.computed_replay,
        &decoded.computed_metric,
        &checked.computed_replay,
        &checked.computed_metric,
    )?;

    Ok(S1M5ReadbackReport {
        publication,
        build_end_tick: common_build_end,
        measurement_start_tick,
        max_ticks: boundaries.max_ticks,
        brute_final_state_hash: brute_execution.final_state_hash,
        computed_final_state_hash: computed_execution.final_state_hash,
        brute_checkpoint_count: brute_execution.checkpoint_count,
        computed_checkpoint_count: computed_execution.checkpoint_count,
    })
}

fn validate_package_identity(
    package: &SimulationPackage,
    scenario: &ScenarioManifest,
) -> Result<(), S1M5ReadbackError> {
    let contract = package.contract();
    if package.scenario_id() != scenario.scenario_id()
        || package.required_features() != scenario.required_features()
        || contract.semantics_version != scenario.semantics_version()
        || contract.numeric_profile_hash != scenario.profiles().numeric().profile_hash()
        || contract.physical_scale_profile_hash
            != scenario.profiles().physical_scale().profile_hash()
        || contract.balance_profile_hash != scenario.profiles().balance().profile_hash()
    {
        return Err(S1M5ReadbackError::Coherence(
            "supplied package does not bind the checked-in Scenario and Profile contract",
        ));
    }
    Ok(())
}

fn scenario_resolution(
    simulation: &Simulation,
) -> Result<ReferenceArchitectureScenarioResolution, S1M5ReadbackError> {
    let main_core = simulation
        .main_core_state()
        .ok_or(S1M5ReadbackError::Coherence(
            "checked-in S1-M5 Scenario package has no Main Core",
        ))?
        .id();
    Ok(ReferenceArchitectureScenarioResolution {
        main_core,
        power_sources: simulation
            .power_sources()
            .map(|source| source.id())
            .collect(),
        enemies: simulation
            .enemies()
            .iter()
            .map(|enemy| enemy.id())
            .collect(),
    })
}

fn validate_replay_commands(
    role: ReferenceArchitectureRole,
    replay: &ReplayArtifact,
    materialization: &MaterializedReferenceArchitecture,
) -> Result<(), S1M5ReadbackError> {
    if replay.replay().commands() != materialization.commands
        || !replay.replay().world_inputs().is_empty()
    {
        return Err(S1M5ReadbackError::ReplayCommandMismatch { role });
    }
    Ok(())
}

struct ReplayExecution {
    final_state_hash: StateHash,
    checkpoint_count: usize,
}

fn execute_retained_replay(
    package: &SimulationPackage,
    role: ReferenceArchitectureRole,
    artifact: &ReplayArtifact,
    materialization: &MaterializedReferenceArchitecture,
) -> Result<ReplayExecution, S1M5ReadbackError> {
    let replay = artifact.replay();
    let mut simulation = Simulation::new(package.clone())?;
    replay.validate_against(&simulation)?;
    let mut trace = vec![simulation.state_hash()];
    while simulation.next_tick() < replay.final_next_tick() {
        let tick = simulation.next_tick();
        let commands = replay.commands_for_tick(tick).cloned().collect::<Vec<_>>();
        let world_inputs = replay
            .world_inputs_for_tick(tick)
            .cloned()
            .collect::<Vec<_>>();
        let expected_acceptances = materialization
            .commands
            .iter()
            .zip(&materialization.acceptances)
            .filter_map(|(command, acceptance)| {
                (command.target_tick == tick).then_some(*acceptance)
            })
            .collect::<Vec<_>>();
        let report = simulation.step_with_world_inputs(&commands, &world_inputs)?;
        if report.command_acceptances != expected_acceptances
            || !report.command_rejections.is_empty()
        {
            return Err(S1M5ReadbackError::ReplayAcceptanceMismatch { role, tick });
        }
        trace.push(report.state_hash);
        if matches!(report.run_status, RunStatus::Ended { .. }) {
            replay.validate_terminal_boundary(report.next_tick)?;
        }
    }
    replay.verify_trace(&trace)?;
    Ok(ReplayExecution {
        final_state_hash: simulation.state_hash(),
        checkpoint_count: replay.checkpoints().len(),
    })
}

#[allow(clippy::too_many_arguments)]
fn rebuild_role_artifacts(
    package: &SimulationPackage,
    scenario: &ScenarioManifest,
    scenario_resolution: &ReferenceArchitectureScenarioResolution,
    metric_set: &ReferenceMetricSetArtifact,
    runs: &[ReferenceExperimentRunV2; 2],
    boundaries: ReferenceMetricBoundaries,
    role: ReferenceArchitectureRole,
    architecture: &ReferenceArchitectureArtifact,
    materialization: &MaterializedReferenceArchitecture,
    retained_replay: &ReplayArtifact,
    retained_metric: &ReferenceMetricArtifact,
    retained_replay_bytes: &[u8],
    retained_metric_bytes: &[u8],
) -> Result<(), S1M5ReadbackError> {
    let run = runs
        .iter()
        .find(|candidate| candidate.design.role == role)
        .ok_or(S1M5ReadbackError::Coherence(
            "resolved Experiment Plan has no run for one required role",
        ))?;
    let rebuilt = build_s1m5_trace_artifacts(S1M5TraceBuildInput {
        initial_simulation: Simulation::new(package.clone())?,
        scenario,
        architecture,
        materialization,
        scenario_resolution,
        metric_set,
        run_id: run.run_id,
        boundaries,
    })?;
    if rebuilt.replay != *retained_replay || rebuilt.replay_bytes != retained_replay_bytes {
        return Err(S1M5ReadbackError::RebuiltArtifactMismatch {
            role,
            artifact: "Replay",
        });
    }
    if rebuilt.metric != *retained_metric || rebuilt.metric_bytes != retained_metric_bytes {
        return Err(S1M5ReadbackError::RebuiltArtifactMismatch {
            role,
            artifact: "Metric",
        });
    }
    Ok(())
}

struct DecodedRetainedBundle {
    scenario: ScenarioManifest,
    brute_design: ReferenceArchitectureArtifact,
    computed_design: ReferenceArchitectureArtifact,
    pair: aon_sim::ReferenceArchitecturePairManifest,
    plan: aon_sim::ReferenceExperimentPlanV2,
    metric_set: ReferenceMetricSetArtifact,
    brute_replay: ReplayArtifact,
    computed_replay: ReplayArtifact,
    brute_metric: ReferenceMetricArtifact,
    computed_metric: ReferenceMetricArtifact,
}

impl DecodedRetainedBundle {
    fn decode(checked: &CheckedRetainedBytes) -> Result<Self, S1M5ReadbackError> {
        let scenario = artifact_result(
            S1M5_SCENARIO_PATH,
            decode_scenario_manifest(&checked.scenario),
        )?;
        let brute_design = decode_design(S1M5_BRUTE_DESIGN_PATH, &checked.brute_design)?;
        let computed_design = decode_design(S1M5_COMPUTED_DESIGN_PATH, &checked.computed_design)?;
        let pair = artifact_result(
            S1M5_PAIR_PATH,
            decode_reference_pair_manifest(&checked.pair),
        )?;
        let plan = artifact_result(
            S1M5_PLAN_PATH,
            decode_reference_experiment_plan_v2(&checked.plan),
        )?;
        let metric_set = artifact_result(
            S1M5_METRIC_SET_PATH,
            decode_reference_metric_set_artifact(&checked.metric_set),
        )?;
        let brute_replay = artifact_result(
            S1M5_BRUTE_REPLAY_PATH,
            decode_replay_artifact(&checked.brute_replay),
        )?;
        let computed_replay = artifact_result(
            S1M5_COMPUTED_REPLAY_PATH,
            decode_replay_artifact(&checked.computed_replay),
        )?;
        let brute_metric = artifact_result(
            S1M5_BRUTE_METRIC_PATH,
            decode_reference_metric_artifact(&checked.brute_metric, &metric_set),
        )?;
        let computed_metric = artifact_result(
            S1M5_COMPUTED_METRIC_PATH,
            decode_reference_metric_artifact(&checked.computed_metric, &metric_set),
        )?;
        Ok(Self {
            scenario,
            brute_design,
            computed_design,
            pair,
            plan,
            metric_set,
            brute_replay,
            computed_replay,
            brute_metric,
            computed_metric,
        })
    }
}

fn decode_design(
    artifact: &'static str,
    bytes: &[u8],
) -> Result<ReferenceArchitectureArtifact, S1M5ReadbackError> {
    let source = artifact_result(artifact, std::str::from_utf8(bytes))?;
    artifact_result(artifact, decode_reference_architecture_artifact(source))
}

fn artifact_result<T, E: Display>(
    artifact: &'static str,
    result: Result<T, E>,
) -> Result<T, S1M5ReadbackError> {
    result.map_err(|error| S1M5ReadbackError::InvalidArtifact {
        artifact,
        message: error.to_string(),
    })
}

struct CheckedRetainedBytes {
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

impl CheckedRetainedBytes {
    fn read(workspace_root: &Path) -> Result<Self, S1M5ReadbackError> {
        let read = |relative_path: &'static str| {
            fs::read(workspace_root.join(relative_path)).map_err(|source| S1M5ReadbackError::Io {
                path: PathBuf::from(relative_path),
                source,
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
