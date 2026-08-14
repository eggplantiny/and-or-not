//! In-memory assembly of the retained S1-M5 Pair, Plan, Replays, and Metric artifacts.

use super::s1m5_pair::{
    S1M5DesignEvidence, S1M5PairArtifacts, S1M5PairBuildError, S1M5PairBuildInput,
    build_s1m5_pair_artifacts,
};
use super::s1m5_trace::{
    S1M5TraceArtifacts, S1M5TraceBuildError, S1M5TraceBuildInput, build_s1m5_trace_artifacts,
};
use aon_sim::{
    MaterializedReferenceArchitecture, ReferenceArchitectureArtifact, ReferenceArchitectureError,
    ReferenceArchitectureRole, ReferenceArchitectureScenarioResolution, ReferenceMetricBoundaries,
    ReferenceMetricError, ScenarioManifest, Simulation, SimulationError, SimulationPackage, Tick,
    materialize_reference_architecture_pair, validate_reference_architecture_against,
    validate_reference_metric_bindings,
};
use thiserror::Error;

pub struct S1M5RetainedBuildInput<'a> {
    pub scenario: &'a ScenarioManifest,
    pub package: &'a SimulationPackage,
    pub scenario_resolution: &'a ReferenceArchitectureScenarioResolution,
    pub brute: &'a ReferenceArchitectureArtifact,
    pub brute_materialization: &'a MaterializedReferenceArchitecture,
    pub computed: &'a ReferenceArchitectureArtifact,
    pub computed_materialization: &'a MaterializedReferenceArchitecture,
    pub build_end_tick: Tick,
    pub measurement_start_tick: Tick,
    pub max_ticks: Tick,
}

pub struct S1M5RetainedSearchInput<'a> {
    pub scenario: &'a ScenarioManifest,
    pub package: &'a SimulationPackage,
    pub scenario_resolution: &'a ReferenceArchitectureScenarioResolution,
    pub brute: &'a ReferenceArchitectureArtifact,
    pub brute_materialization: &'a MaterializedReferenceArchitecture,
    pub computed: &'a ReferenceArchitectureArtifact,
    pub computed_materialization: &'a MaterializedReferenceArchitecture,
}

#[derive(Clone, Debug)]
pub struct S1M5RetainedArtifacts {
    pub pair: S1M5PairArtifacts,
    pub brute: S1M5TraceArtifacts,
    pub computed: S1M5TraceArtifacts,
}

#[derive(Debug, Error)]
pub enum S1M5RetainedBuildError {
    #[error(transparent)]
    Architecture(#[from] ReferenceArchitectureError),
    #[error(transparent)]
    Metric(#[from] ReferenceMetricError),
    #[error(transparent)]
    Pair(#[from] S1M5PairBuildError),
    #[error(transparent)]
    Trace(#[from] S1M5TraceBuildError),
    #[error(transparent)]
    Simulation(#[from] SimulationError),
    #[error("the resolved Experiment Plan has no {role:?} run")]
    MissingRun { role: ReferenceArchitectureRole },
    #[error("rematerializing a reference design produced different canonical evidence")]
    MaterializationEvidenceMismatch,
    #[error(
        "the paired materializations do not share buildEndTick: Brute={brute:?}, Computed={computed:?}"
    )]
    MaterializationBuildEndMismatch { brute: Tick, computed: Tick },
    #[error(
        "paired materialization did not finish at its evidence buildEndTick: buildEndTick={build_end:?}, Brute nextTick={brute_next:?}, Computed nextTick={computed_next:?}"
    )]
    MaterializationTickMismatch {
        build_end: Tick,
        brute_next: Tick,
        computed_next: Tick,
    },
    #[error("the paired v2 final barrier is not quiescent at buildEndTick {build_end:?}")]
    FinalBarrierNotQuiescent { build_end: Tick },
    #[error(
        "the retained v2 measurementStartTick must equal buildEndTick: measurementStartTick={measurement_start:?}, buildEndTick={build_end:?}"
    )]
    MeasurementStartBuildEndMismatch {
        measurement_start: Tick,
        build_end: Tick,
    },
    #[error("the two designs did not complete every response before the bounded retained horizon")]
    ResponseBoundaryNotReached,
    #[error("the bounded retained-horizon search overflowed Tick")]
    SearchBoundaryOverflow,
}

/// Rematerializes the pair and independently observes its final common stable signal boundary.
///
/// The v2 pair materializer includes a common quiescence barrier after its final binding stage, so
/// the retained measurement boundary is exactly `buildEndTick`; this verifier never advances the
/// returned post-materialization simulations.
pub fn derive_first_common_quiescent_tick(
    package: &SimulationPackage,
    brute: &ReferenceArchitectureArtifact,
    brute_materialization: &MaterializedReferenceArchitecture,
    computed: &ReferenceArchitectureArtifact,
    computed_materialization: &MaterializedReferenceArchitecture,
    scenario_resolution: &ReferenceArchitectureScenarioResolution,
) -> Result<Tick, S1M5RetainedBuildError> {
    let ((brute_simulation, brute_evidence), (computed_simulation, computed_evidence)) =
        materialize_reference_architecture_pair(
            (
                Simulation::new(package.clone())?,
                brute,
                scenario_resolution,
            ),
            (
                Simulation::new(package.clone())?,
                computed,
                scenario_resolution,
            ),
        )?;
    if &brute_evidence != brute_materialization || &computed_evidence != computed_materialization {
        return Err(S1M5RetainedBuildError::MaterializationEvidenceMismatch);
    }
    let build_end_tick = shared_build_end_tick(&brute_evidence, &computed_evidence)?;
    let brute_next = brute_simulation.next_tick();
    let computed_next = computed_simulation.next_tick();
    if brute_next != build_end_tick || computed_next != build_end_tick {
        return Err(S1M5RetainedBuildError::MaterializationTickMismatch {
            build_end: build_end_tick,
            brute_next,
            computed_next,
        });
    }
    let brute_quiescence = brute_simulation.signal_quiescence_snapshot()?;
    let computed_quiescence = computed_simulation.signal_quiescence_snapshot()?;
    if !brute_quiescence.is_quiescent() || !computed_quiescence.is_quiescent() {
        return Err(S1M5RetainedBuildError::FinalBarrierNotQuiescent {
            build_end: build_end_tick,
        });
    }
    Ok(build_end_tick)
}

/// Freezes the first shared boundary after both designs have completed every response row.
pub fn build_first_complete_s1m5_retained_artifacts(
    input: S1M5RetainedSearchInput<'_>,
) -> Result<S1M5RetainedArtifacts, S1M5RetainedBuildError> {
    let measurement_start_tick = derive_first_common_quiescent_tick(
        input.package,
        input.brute,
        input.brute_materialization,
        input.computed,
        input.computed_materialization,
        input.scenario_resolution,
    )?;
    let build_end_tick =
        shared_build_end_tick(input.brute_materialization, input.computed_materialization)?;
    require_measurement_start_at_build_end(measurement_start_tick, build_end_tick)?;
    let first_boundary = measurement_start_tick
        .0
        .checked_add(1)
        .ok_or(S1M5RetainedBuildError::SearchBoundaryOverflow)?;
    let last_boundary = measurement_start_tick
        .0
        .checked_add(64)
        .ok_or(S1M5RetainedBuildError::SearchBoundaryOverflow)?;
    for boundary in first_boundary..=last_boundary {
        let result =
            build_s1m5_retained_artifacts_after_pair_verification(S1M5RetainedBuildInput {
                scenario: input.scenario,
                package: input.package,
                scenario_resolution: input.scenario_resolution,
                brute: input.brute,
                brute_materialization: input.brute_materialization,
                computed: input.computed,
                computed_materialization: input.computed_materialization,
                build_end_tick,
                measurement_start_tick,
                max_ticks: Tick(boundary),
            });
        match result {
            Ok(artifacts) => return Ok(artifacts),
            Err(S1M5RetainedBuildError::Trace(S1M5TraceBuildError::Metric(
                ReferenceMetricError::ObservationNotReached { .. },
            )))
            | Err(S1M5RetainedBuildError::Trace(S1M5TraceBuildError::TerminalBeforeBoundary {
                ..
            })) => {}
            Err(error) => return Err(error),
        }
    }
    Err(S1M5RetainedBuildError::ResponseBoundaryNotReached)
}

fn build_s1m5_retained_artifacts_after_pair_verification(
    input: S1M5RetainedBuildInput<'_>,
) -> Result<S1M5RetainedArtifacts, S1M5RetainedBuildError> {
    let S1M5RetainedBuildInput {
        scenario,
        package,
        scenario_resolution,
        brute,
        brute_materialization,
        computed,
        computed_materialization,
        build_end_tick,
        measurement_start_tick,
        max_ticks,
    } = input;

    validate_reference_architecture_against(
        brute,
        package.contract(),
        package.profiles().physical_scale(),
    )?;
    validate_reference_architecture_against(
        computed,
        package.contract(),
        package.profiles().physical_scale(),
    )?;

    let pair = build_s1m5_pair_artifacts(S1M5PairBuildInput {
        scenario,
        profiles: package.profiles(),
        contract: *package.contract(),
        brute: S1M5DesignEvidence {
            artifact_hash: brute.semantic_hash()?,
            materialization: brute_materialization,
        },
        computed: S1M5DesignEvidence {
            artifact_hash: computed.semantic_hash()?,
            materialization: computed_materialization,
        },
        build_end_tick,
        measurement_start_tick,
        max_ticks,
    })?;
    validate_reference_metric_bindings(&pair.pair, &pair.metric_set, brute, computed)?;

    let run_id = |role| {
        pair.runs
            .iter()
            .find(|run| run.design.role == role)
            .map(|run| run.run_id)
            .ok_or(S1M5RetainedBuildError::MissingRun { role })
    };
    let boundaries = ReferenceMetricBoundaries {
        build_end_tick,
        measurement_start_tick,
        max_ticks,
    };
    let brute_trace = build_s1m5_trace_artifacts(S1M5TraceBuildInput {
        initial_simulation: Simulation::new(package.clone())?,
        scenario,
        architecture: brute,
        materialization: brute_materialization,
        scenario_resolution,
        metric_set: &pair.metric_set,
        run_id: run_id(ReferenceArchitectureRole::Brute)?,
        boundaries,
    })?;
    let computed_trace = build_s1m5_trace_artifacts(S1M5TraceBuildInput {
        initial_simulation: Simulation::new(package.clone())?,
        scenario,
        architecture: computed,
        materialization: computed_materialization,
        scenario_resolution,
        metric_set: &pair.metric_set,
        run_id: run_id(ReferenceArchitectureRole::Computed)?,
        boundaries,
    })?;

    Ok(S1M5RetainedArtifacts {
        pair,
        brute: brute_trace,
        computed: computed_trace,
    })
}

fn shared_build_end_tick(
    brute: &MaterializedReferenceArchitecture,
    computed: &MaterializedReferenceArchitecture,
) -> Result<Tick, S1M5RetainedBuildError> {
    if brute.build_end_tick != computed.build_end_tick {
        return Err(S1M5RetainedBuildError::MaterializationBuildEndMismatch {
            brute: brute.build_end_tick,
            computed: computed.build_end_tick,
        });
    }
    Ok(brute.build_end_tick)
}

fn require_measurement_start_at_build_end(
    measurement_start: Tick,
    build_end: Tick,
) -> Result<(), S1M5RetainedBuildError> {
    if measurement_start != build_end {
        return Err(S1M5RetainedBuildError::MeasurementStartBuildEndMismatch {
            measurement_start,
            build_end,
        });
    }
    Ok(())
}
