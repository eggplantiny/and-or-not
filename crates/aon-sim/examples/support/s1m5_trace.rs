//! Pure S1-M5 Replay and metric-artifact collection.
//!
//! The caller supplies a pristine Simulation plus already-validated retained inputs. This module
//! performs no filesystem I/O: it executes the exact materialization Command stream, records a
//! complete contiguous Replay v2 trace, derives metrics from the same immutable reports, and
//! returns canonical artifact bytes for publication by the generator.

use aon_sim::{
    ExperimentRunId, HashCheckpoint, MaterializedReferenceArchitecture,
    ReferenceArchitectureArtifact, ReferenceArchitectureError,
    ReferenceArchitectureScenarioResolution, ReferenceMetricArtifact, ReferenceMetricBoundaries,
    ReferenceMetricCollector, ReferenceMetricError, ReferenceMetricSetArtifact,
    ReferenceMetricTickSample, RenderSnapshot, Replay, ReplayArtifact, ReplayError, RunStatus,
    ScenarioManifest, Simulation, SimulationContract, SimulationError, Tick,
    derive_reference_static_inventory, encode_reference_metric_artifact, encode_replay_artifact,
    reduce_reference_metrics, resolve_reference_response_observations,
    validate_reference_architecture_against,
};
use thiserror::Error;

pub const S1M5_REPLAY_SCENARIO_PATH: &str = "../../scenarios/s1-m5-reference-architectures-v1.json";

pub struct S1M5TraceBuildInput<'a> {
    /// A fresh Tick-0 Simulation constructed from `scenario` and its selected Profiles.
    pub initial_simulation: Simulation,
    pub scenario: &'a ScenarioManifest,
    pub architecture: &'a ReferenceArchitectureArtifact,
    /// Evidence produced by atomically materializing `architecture` in an equivalent Simulation.
    pub materialization: &'a MaterializedReferenceArchitecture,
    pub scenario_resolution: &'a ReferenceArchitectureScenarioResolution,
    pub metric_set: &'a ReferenceMetricSetArtifact,
    pub run_id: ExperimentRunId,
    /// The shared pair boundaries, including any empty build padding for this design.
    pub boundaries: ReferenceMetricBoundaries,
}

#[derive(Clone, Debug)]
pub struct S1M5TraceArtifacts {
    pub replay: ReplayArtifact,
    pub replay_bytes: Vec<u8>,
    pub metric: ReferenceMetricArtifact,
    pub metric_bytes: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum S1M5TraceBuildError {
    #[error(transparent)]
    Architecture(#[from] ReferenceArchitectureError),
    #[error(transparent)]
    Metric(#[from] ReferenceMetricError),
    #[error(transparent)]
    Replay(#[from] ReplayError),
    #[error(transparent)]
    Simulation(#[from] SimulationError),
    #[error("the supplied Simulation must be pristine at Tick 0, got {actual:?}")]
    NonzeroInitialTick { actual: Tick },
    #[error("the supplied Simulation does not match the Scenario manifest identity")]
    ScenarioIdentityMismatch,
    #[error("the supplied Scenario resolution does not match the fresh Simulation identities")]
    ScenarioResolutionMismatch,
    #[error("the selected Balance Profile has no Construction probe")]
    MissingConstructionProbe,
    #[error(
        "materialization ends after the common build boundary: materialization={materialization:?}, common={common:?}"
    )]
    MaterializationPastCommonBuildEnd { materialization: Tick, common: Tick },
    #[error("fresh trace materialization acceptances diverged at {tick:?}")]
    MaterializationAcceptanceMismatch { tick: Tick },
    #[error(
        "the retained trace ended at completed Tick {completed_tick:?} before maxTicks {max_ticks:?}"
    )]
    TerminalBeforeBoundary {
        completed_tick: Tick,
        max_ticks: Tick,
    },
    #[error(
        "the retained comparison boundary must remain Running, but the run ended at {completed_tick:?}"
    )]
    TerminalAtBoundary { completed_tick: Tick },
    #[error(
        "the retained comparison window contains {count} structural destructions at completed Tick {completed_tick:?}"
    )]
    DestructionBeforeBoundary { completed_tick: Tick, count: usize },
}

/// Builds one retained S1-M5 Replay and metric artifact without touching the filesystem.
pub fn build_s1m5_trace_artifacts(
    input: S1M5TraceBuildInput<'_>,
) -> Result<S1M5TraceArtifacts, S1M5TraceBuildError> {
    let S1M5TraceBuildInput {
        mut initial_simulation,
        scenario,
        architecture,
        materialization,
        scenario_resolution,
        metric_set,
        run_id,
        boundaries,
    } = input;

    boundaries.validate()?;
    validate_initial_context(
        &initial_simulation,
        scenario,
        architecture,
        scenario_resolution,
    )?;
    if materialization.build_end_tick > boundaries.build_end_tick {
        return Err(S1M5TraceBuildError::MaterializationPastCommonBuildEnd {
            materialization: materialization.build_end_tick,
            common: boundaries.build_end_tick,
        });
    }

    let construction_probe = initial_simulation
        .profiles()
        .balance()
        .construction_probe
        .ok_or(S1M5TraceBuildError::MissingConstructionProbe)?;
    let static_inventory = derive_reference_static_inventory(
        architecture,
        &construction_probe,
        materialization,
        scenario_resolution,
    )?;
    let response = resolve_reference_response_observations(
        metric_set,
        architecture,
        materialization,
        scenario_resolution,
    )?;

    let initial_snapshot = render(&initial_simulation);
    let initial_sample = ReferenceMetricTickSample::from_snapshot(&initial_snapshot)?;
    let collector = ReferenceMetricCollector::new(
        boundaries,
        static_inventory,
        initial_sample,
        scenario_resolution.enemies.clone(),
        response,
    )?;

    let replay_header = initial_simulation.replay_header();
    let initial_hash = initial_simulation.state_hash();
    let mut trace = vec![initial_hash];
    let mut checkpoints = vec![HashCheckpoint {
        next_tick: Tick(0),
        state_hash: initial_hash,
    }];
    let mut reports = Vec::new();
    let mut samples = Vec::new();

    while initial_simulation.next_tick() < boundaries.max_ticks {
        let tick = initial_simulation.next_tick();
        let commands = materialization
            .commands
            .iter()
            .filter(|command| command.target_tick == tick)
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
        let report = initial_simulation.step(&commands)?;
        if report.command_acceptances != expected_acceptances
            || !report.command_rejections.is_empty()
        {
            return Err(S1M5TraceBuildError::MaterializationAcceptanceMismatch { tick });
        }

        trace.push(report.state_hash);
        checkpoints.push(HashCheckpoint {
            next_tick: report.next_tick,
            state_hash: report.state_hash,
        });
        let sample = ReferenceMetricTickSample::from_snapshot(&render(&initial_simulation))?;
        let terminal = matches!(report.run_status, RunStatus::Ended { .. });
        let completed_tick = report.completed_tick;
        let next_tick = report.next_tick;
        if !report.destructions.is_empty() {
            return Err(S1M5TraceBuildError::DestructionBeforeBoundary {
                completed_tick,
                count: report.destructions.len(),
            });
        }
        reports.push(report);
        samples.push(sample);
        if terminal {
            if next_tick < boundaries.max_ticks {
                return Err(S1M5TraceBuildError::TerminalBeforeBoundary {
                    completed_tick,
                    max_ticks: boundaries.max_ticks,
                });
            }
            return Err(S1M5TraceBuildError::TerminalAtBoundary { completed_tick });
        }
    }

    let result = reduce_reference_metrics(collector, reports.iter().zip(samples.iter().copied()))?;
    let replay = Replay::new_v2(
        replay_header,
        materialization.commands.clone(),
        Vec::new(),
        checkpoints,
    )?;
    replay.verify_trace(&trace)?;
    let replay = ReplayArtifact::new(S1M5_REPLAY_SCENARIO_PATH, replay)?;
    let replay_bytes = encode_replay_artifact(&replay)?;

    let metric = ReferenceMetricArtifact::v1(metric_set, run_id, result)?;
    let metric_bytes = encode_reference_metric_artifact(&metric, metric_set)?;

    Ok(S1M5TraceArtifacts {
        replay,
        replay_bytes,
        metric,
        metric_bytes,
    })
}

fn validate_initial_context(
    simulation: &Simulation,
    scenario: &ScenarioManifest,
    architecture: &ReferenceArchitectureArtifact,
    resolution: &ReferenceArchitectureScenarioResolution,
) -> Result<(), S1M5TraceBuildError> {
    if simulation.next_tick() != Tick(0) {
        return Err(S1M5TraceBuildError::NonzeroInitialTick {
            actual: simulation.next_tick(),
        });
    }
    let scenario_contract = SimulationContract {
        semantics_version: scenario.semantics_version(),
        numeric_profile_hash: scenario.profiles().numeric().profile_hash(),
        physical_scale_profile_hash: scenario.profiles().physical_scale().profile_hash(),
        balance_profile_hash: scenario.profiles().balance().profile_hash(),
    };
    if scenario.scenario_id() != simulation.scenario_id()
        || scenario.hash_algorithm() != simulation.contract().hash_algorithm_id()
        || scenario_contract != *simulation.contract()
    {
        return Err(S1M5TraceBuildError::ScenarioIdentityMismatch);
    }
    validate_reference_architecture_against(
        architecture,
        simulation.contract(),
        simulation.profiles().physical_scale(),
    )?;

    let main_core = simulation.main_core_state().map(|core| core.id());
    let power_sources = simulation
        .power_sources()
        .map(|source| source.id())
        .collect::<Vec<_>>();
    let enemies = simulation
        .enemies()
        .iter()
        .map(|enemy| enemy.id())
        .collect::<Vec<_>>();
    if main_core != Some(resolution.main_core)
        || power_sources != resolution.power_sources
        || enemies != resolution.enemies
    {
        return Err(S1M5TraceBuildError::ScenarioResolutionMismatch);
    }
    Ok(())
}

fn render(simulation: &Simulation) -> RenderSnapshot {
    let mut snapshot = RenderSnapshot::default();
    simulation.write_render_snapshot(&mut snapshot);
    snapshot
}
