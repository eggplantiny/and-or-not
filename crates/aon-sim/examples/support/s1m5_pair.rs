//! Pure construction of the retained S1-M5 Metric Set, Pair, Plan, and Run IDs.
//!
//! This module deliberately performs no simulation, replay collection, metric reduction, or
//! filesystem I/O. The generator supplies already-materialized design evidence and the three
//! shared Tick boundaries; this builder binds those facts into the strict public artifacts.

use aon_sim::{
    ArtifactHash, Fixed, FixedAabb, FixedVec2, InitialWorld, MaterializedReferenceArchitecture,
    ProfileBundle, ReferenceArchitecturePairManifest, ReferenceArchitectureRole,
    ReferenceArtifactReference, ReferenceDesignBinding, ReferenceExperimentError,
    ReferenceExperimentPlanV2, ReferenceExperimentRunV2, ReferenceMetricError,
    ReferenceMetricSetArtifact, ReferencePairFairnessInput, ReferenceProfileReference,
    ReferenceResponseBinding, ReferenceResponseObservationSpec, ReferenceTerritoryAnchor,
    ScenarioHashError, ScenarioManifest, Seed, SimulationContract, Tick,
    encode_reference_experiment_plan_v2, encode_reference_metric_set_artifact,
    encode_reference_pair_manifest, reference_empty_shared_command_log_hash,
    reference_enemy_sequence_hash, reference_power_source_sequence_hash,
    validate_reference_pair_fairness,
};
use thiserror::Error;

pub const S1M5_PAIR_ID: &str = "s1-m5-reference-pair-v1";
pub const S1M5_EXPERIMENT_ID: &str = "s1-m5-reference-experiment-v2";

const SCENARIO_REFERENCE_PATH: &str = "../scenarios/s1-m5-reference-architectures-v1.json";
const BRUTE_DESIGN_REFERENCE_PATH: &str = "../designs/s1-m5-brute-v2.json";
const COMPUTED_DESIGN_REFERENCE_PATH: &str = "../designs/s1-m5-computed-v2.json";
const PAIR_REFERENCE_PATH: &str = "s1-m5-reference-pair-v1.json";

#[derive(Clone, Copy)]
pub struct S1M5DesignEvidence<'a> {
    pub artifact_hash: ArtifactHash,
    pub materialization: &'a MaterializedReferenceArchitecture,
}

#[derive(Clone, Copy)]
pub struct S1M5PairBuildInput<'a> {
    pub scenario: &'a ScenarioManifest,
    pub profiles: &'a ProfileBundle,
    pub contract: SimulationContract,
    pub brute: S1M5DesignEvidence<'a>,
    pub computed: S1M5DesignEvidence<'a>,
    pub build_end_tick: Tick,
    pub measurement_start_tick: Tick,
    pub max_ticks: Tick,
}

#[derive(Clone, Debug)]
pub struct S1M5PairArtifacts {
    pub metric_set: ReferenceMetricSetArtifact,
    pub metric_set_bytes: Vec<u8>,
    pub pair: ReferenceArchitecturePairManifest,
    pub pair_bytes: Vec<u8>,
    pub plan_bytes: Vec<u8>,
    pub runs: [ReferenceExperimentRunV2; 2],
}

#[derive(Debug, Error)]
pub enum S1M5PairBuildError {
    #[error(transparent)]
    Experiment(#[from] ReferenceExperimentError),
    #[error(transparent)]
    Metric(#[from] ReferenceMetricError),
    #[error(transparent)]
    ScenarioHash(#[from] ScenarioHashError),
    #[error("S1-M5 requires the main-core/power/enemy Scenario-v4 world")]
    UnexpectedInitialWorld,
    #[error("S1-M5 requires exactly four cardinal Power Sources")]
    InvalidPowerSourceLayout,
    #[error("the selected Balance Profile has no Capacity probe")]
    MissingCapacityProbe,
    #[error(
        "paired materialization build boundaries differ: brute={brute:?}, computed={computed:?}"
    )]
    MaterializationBuildEndMismatch { brute: Tick, computed: Tick },
    #[error("paired v2 materialization evidence is missing")]
    MissingPairedMaterializationEvidence,
    #[error("paired v2 executed-batch evidence differs")]
    ExecutedBatchEvidenceMismatch,
    #[error("paired v2 binding-stage evidence differs")]
    BindingStageEvidenceMismatch,
    #[error("paired v2 materialization evidence is invalid: {0}")]
    InvalidPairedMaterializationEvidence(&'static str),
    #[error(
        "paired v2 final quiescent boundary mismatch: buildEndTick={build_end:?}, evidence={evidence:?}"
    )]
    FinalQuiescentBoundaryMismatch { build_end: Tick, evidence: Tick },
    #[error("common buildEndTick mismatch: supplied={supplied:?}, evidence={evidence:?}")]
    BuildEndMismatch { supplied: Tick, evidence: Tick },
}

/// Builds all non-runtime S1-M5 retained artifacts from shared inputs and materialization evidence.
pub fn build_s1m5_pair_artifacts(
    input: S1M5PairBuildInput<'_>,
) -> Result<S1M5PairArtifacts, S1M5PairBuildError> {
    let evidence_build_end = validate_paired_materialization_evidence(
        input.brute.materialization,
        input.computed.materialization,
    )?;
    if input.build_end_tick != evidence_build_end {
        return Err(S1M5PairBuildError::BuildEndMismatch {
            supplied: input.build_end_tick,
            evidence: evidence_build_end,
        });
    }

    let InitialWorld::MainCorePowerEnemyV1 {
        power_sources,
        enemies,
        ..
    } = input.scenario.initial_world()
    else {
        return Err(S1M5PairBuildError::UnexpectedInitialWorld);
    };
    let (territory, territory_anchors) = territory_from_power_sources(power_sources)?;
    let main_core_capacity = input
        .profiles
        .balance()
        .capacity_probe
        .ok_or(S1M5PairBuildError::MissingCapacityProbe)?
        .main_core_capacity;

    let response_observations = canonical_response_observations();
    let response_bindings = response_observations
        .iter()
        .map(|row| ReferenceResponseBinding {
            name: row.name.clone(),
            hostile_entry_binding: row.hostile_entry_binding.clone(),
            defense_contact_binding: row.defense_contact_binding.clone(),
        })
        .collect();
    let metric_set = ReferenceMetricSetArtifact::v1(response_observations)?;
    let metric_set_hash = metric_set.semantic_hash()?;
    let metric_set_bytes = encode_reference_metric_set_artifact(&metric_set)?;

    let scenario_profiles = input.scenario.profiles();
    let profile_reference = |reference: &aon_sim::ProfileReference| {
        ReferenceProfileReference::new(
            reference.path(),
            reference.profile_id(),
            reference.profile_hash(),
        )
    };
    let pair = ReferenceArchitecturePairManifest::v1(
        S1M5_PAIR_ID,
        input.scenario.scenario_id(),
        ReferenceArtifactReference::new(SCENARIO_REFERENCE_PATH, input.scenario.canonical_hash()?)?,
        input.contract,
        profile_reference(scenario_profiles.numeric())?,
        profile_reference(scenario_profiles.physical_scale())?,
        profile_reference(scenario_profiles.balance())?,
        input.build_end_tick,
        input.measurement_start_tick,
        input.max_ticks,
        main_core_capacity,
        territory,
        territory_anchors,
        reference_power_source_sequence_hash(power_sources)?,
        reference_enemy_sequence_hash(enemies)?,
        reference_empty_shared_command_log_hash()?,
        metric_set.metric_set_id(),
        metric_set_hash,
        [
            ReferenceDesignBinding {
                role: ReferenceArchitectureRole::Brute,
                design: ReferenceArtifactReference::new(
                    BRUTE_DESIGN_REFERENCE_PATH,
                    input.brute.artifact_hash,
                )?,
                command_log_hash: input.brute.materialization.command_log_hash,
            },
            ReferenceDesignBinding {
                role: ReferenceArchitectureRole::Computed,
                design: ReferenceArtifactReference::new(
                    COMPUTED_DESIGN_REFERENCE_PATH,
                    input.computed.artifact_hash,
                )?,
                command_log_hash: input.computed.materialization.command_log_hash,
            },
        ],
        response_bindings,
    )?;
    validate_reference_pair_fairness(
        &pair,
        ReferencePairFairnessInput {
            scenario: input.scenario,
            contract: input.contract,
            profiles: input.profiles,
            build_end_tick: input.build_end_tick,
            measurement_start_tick: input.measurement_start_tick,
            max_ticks: input.max_ticks,
            main_core_capacity,
            territory,
            shared_command_log_hash: reference_empty_shared_command_log_hash()?,
            seed: Seed::ZERO,
            metric_set_id: metric_set.metric_set_id(),
            metric_set_hash,
        },
    )?;
    let pair_hash = pair.semantic_hash()?;
    let pair_bytes = encode_reference_pair_manifest(&pair)?;
    let plan = ReferenceExperimentPlanV2::from_pair(
        S1M5_EXPERIMENT_ID,
        ReferenceArtifactReference::new(PAIR_REFERENCE_PATH, pair_hash)?,
        &pair,
    )?;
    let plan_bytes = encode_reference_experiment_plan_v2(&plan)?;
    let runs = plan.resolve(&pair)?;

    Ok(S1M5PairArtifacts {
        metric_set,
        metric_set_bytes,
        pair,
        pair_bytes,
        plan_bytes,
        runs,
    })
}

fn validate_paired_materialization_evidence(
    brute: &MaterializedReferenceArchitecture,
    computed: &MaterializedReferenceArchitecture,
) -> Result<Tick, S1M5PairBuildError> {
    if brute.build_end_tick != computed.build_end_tick {
        return Err(S1M5PairBuildError::MaterializationBuildEndMismatch {
            brute: brute.build_end_tick,
            computed: computed.build_end_tick,
        });
    }
    if brute.executed_batch_evidence.is_empty()
        || brute.binding_stage_evidence.is_empty()
        || computed.executed_batch_evidence.is_empty()
        || computed.binding_stage_evidence.is_empty()
    {
        return Err(S1M5PairBuildError::MissingPairedMaterializationEvidence);
    }
    if brute.executed_batch_evidence != computed.executed_batch_evidence {
        return Err(S1M5PairBuildError::ExecutedBatchEvidenceMismatch);
    }
    if brute.binding_stage_evidence != computed.binding_stage_evidence {
        return Err(S1M5PairBuildError::BindingStageEvidenceMismatch);
    }
    let binding_stage_count = brute.binding_stage_evidence.len();
    let executed_binding_batches = brute
        .executed_batch_evidence
        .iter()
        .filter_map(|batch| match batch.kind {
            aon_sim::ReferenceArchitectureMaterializationBatchKind::Binding { stage } => {
                Some((stage, batch.command_tick))
            }
            aon_sim::ReferenceArchitectureMaterializationBatchKind::Placement { .. } => None,
        })
        .collect::<Vec<_>>();
    if executed_binding_batches.len() != binding_stage_count {
        return Err(S1M5PairBuildError::InvalidPairedMaterializationEvidence(
            "executed binding-batch count does not match binding-stage evidence",
        ));
    }
    let mut prior_quiescent_tick = None;
    for (expected_stage, ((executed_stage, executed_tick), stage_evidence)) in
        executed_binding_batches
            .iter()
            .copied()
            .zip(&brute.binding_stage_evidence)
            .enumerate()
    {
        let expected_stage = u8::try_from(expected_stage).map_err(|_| {
            S1M5PairBuildError::InvalidPairedMaterializationEvidence(
                "binding-stage index exceeds the v2 evidence range",
            )
        })?;
        if executed_stage != expected_stage
            || stage_evidence.stage != expected_stage
            || stage_evidence.command_tick != executed_tick
            || prior_quiescent_tick.is_some_and(|prior| prior != executed_tick)
        {
            return Err(S1M5PairBuildError::InvalidPairedMaterializationEvidence(
                "binding stages are not consecutive on their shared quiescent boundaries",
            ));
        }
        let mut expected_tick = next_tick(executed_tick)?;
        for &barrier_tick in &stage_evidence.barrier_ticks {
            if barrier_tick != expected_tick {
                return Err(S1M5PairBuildError::InvalidPairedMaterializationEvidence(
                    "binding-stage barrier Ticks are not contiguous",
                ));
            }
            expected_tick = next_tick(expected_tick)?;
        }
        if stage_evidence.quiescent_tick != expected_tick {
            return Err(S1M5PairBuildError::InvalidPairedMaterializationEvidence(
                "binding-stage quiescent boundary does not follow its barrier",
            ));
        }
        prior_quiescent_tick = Some(stage_evidence.quiescent_tick);
    }
    let final_quiescent_tick = brute
        .binding_stage_evidence
        .last()
        .expect("nonempty paired binding-stage evidence")
        .quiescent_tick;
    if final_quiescent_tick != brute.build_end_tick {
        return Err(S1M5PairBuildError::FinalQuiescentBoundaryMismatch {
            build_end: brute.build_end_tick,
            evidence: final_quiescent_tick,
        });
    }
    Ok(brute.build_end_tick)
}

fn next_tick(tick: Tick) -> Result<Tick, S1M5PairBuildError> {
    tick.0
        .checked_add(1)
        .map(Tick)
        .ok_or(S1M5PairBuildError::InvalidPairedMaterializationEvidence(
            "materialization evidence Tick overflow",
        ))
}

fn canonical_response_observations() -> Vec<ReferenceResponseObservationSpec> {
    [
        ("east.0", "sensor.east.0", "defense.east.0", 3),
        ("north.0", "sensor.north.0", "defense.north.0", 2),
        ("south.0", "sensor.south.0", "defense.south.0", 1),
        ("west.0", "sensor.west.0", "defense.west.0", 0),
    ]
    .into_iter()
    .map(
        |(name, hostile_entry_binding, defense_contact_binding, enemy_ordinal)| {
            ReferenceResponseObservationSpec {
                name: name.to_owned(),
                hostile_entry_binding: hostile_entry_binding.to_owned(),
                defense_contact_binding: defense_contact_binding.to_owned(),
                enemy_ordinal,
            }
        },
    )
    .collect()
}

fn territory_from_power_sources(
    power_sources: &[aon_sim::PowerSourceInitialState],
) -> Result<(FixedAabb, Vec<ReferenceTerritoryAnchor>), S1M5PairBuildError> {
    if power_sources.len() != 4 {
        return Err(S1M5PairBuildError::InvalidPowerSourceLayout);
    }
    let minimum_x = power_sources
        .iter()
        .map(|source| source.position().x.0)
        .min()
        .ok_or(S1M5PairBuildError::InvalidPowerSourceLayout)?;
    let maximum_x = power_sources
        .iter()
        .map(|source| source.position().x.0)
        .max()
        .ok_or(S1M5PairBuildError::InvalidPowerSourceLayout)?;
    let minimum_y = power_sources
        .iter()
        .map(|source| source.position().y.0)
        .min()
        .ok_or(S1M5PairBuildError::InvalidPowerSourceLayout)?;
    let maximum_y = power_sources
        .iter()
        .map(|source| source.position().y.0)
        .max()
        .ok_or(S1M5PairBuildError::InvalidPowerSourceLayout)?;
    if minimum_x >= maximum_x || minimum_y >= maximum_y {
        return Err(S1M5PairBuildError::InvalidPowerSourceLayout);
    }
    let midpoint_x =
        i64::try_from(i128::from(minimum_x) + (i128::from(maximum_x) - i128::from(minimum_x)) / 2)
            .map_err(|_| S1M5PairBuildError::InvalidPowerSourceLayout)?;
    let midpoint_y =
        i64::try_from(i128::from(minimum_y) + (i128::from(maximum_y) - i128::from(minimum_y)) / 2)
            .map_err(|_| S1M5PairBuildError::InvalidPowerSourceLayout)?;
    let expected = [
        ("east", FixedVec2::new(Fixed(maximum_x), Fixed(midpoint_y))),
        ("north", FixedVec2::new(Fixed(midpoint_x), Fixed(maximum_y))),
        ("south", FixedVec2::new(Fixed(midpoint_x), Fixed(minimum_y))),
        ("west", FixedVec2::new(Fixed(minimum_x), Fixed(midpoint_y))),
    ];
    if expected.iter().any(|(_, position)| {
        !power_sources
            .iter()
            .any(|source| source.position() == *position)
    }) {
        return Err(S1M5PairBuildError::InvalidPowerSourceLayout);
    }
    let territory = FixedAabb::new(
        FixedVec2::new(Fixed(minimum_x), Fixed(minimum_y)),
        FixedVec2::new(Fixed(maximum_x), Fixed(maximum_y)),
    );
    let anchors = expected
        .into_iter()
        .map(|(name, position)| ReferenceTerritoryAnchor {
            name: name.to_owned(),
            position,
        })
        .collect();
    Ok((territory, anchors))
}
