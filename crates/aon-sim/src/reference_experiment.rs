//! Strict S1-M5 reference-pair and two-run experiment contracts.
//!
//! These artifacts are noncanonical experiment content. Loading, hashing, or validating them
//! cannot mutate a [`crate::Simulation`]. Experiment-v1 remains owned by `experiment.rs` and
//! `experiment_artifact.rs`; this module never reinterprets its bytes or Run IDs.

use crate::{
    ArtifactHash, BALANCE_SCHEMA_VERSION_V5, EnemyInitialState, ExperimentRunId, FixedAabb,
    FixedVec2, HASH_ALGORITHM_ID_BLAKE3_V1, HashAlgorithmId, HashParseError, InitialWorld,
    JsonErrorCategory, PROFILE_SCHEMA_VERSION_V1, PowerSourceInitialState, ProfileBundle,
    ProfileHash, SCENARIO_SCHEMA_VERSION_V4, ScenarioHashError, ScenarioManifest, Seed,
    SeedParseError, SemanticsVersion, SimulationContract, Tick, WorldGeneratorVersion,
    reference_architecture_command_log_hash,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Component, Path};
use thiserror::Error;

pub const REFERENCE_PAIR_FORMAT_VERSION_V1: u32 = 1;
pub const REFERENCE_EXPERIMENT_FORMAT_VERSION_V2: u32 = 2;
pub const REFERENCE_EXPERIMENT_STAGE_S1_M5: &str = "s1-m5";

const REFERENCE_PAIR_HASH_DOMAIN: &[u8] = b"AON\0REFERENCE-PAIR\0V1\0";
const REFERENCE_PAIR_ENCODER_VERSION: u16 = 1;
const EXPERIMENT_RUN_ID_V2_DOMAIN: &[u8] = b"AON\0EXPERIMENT-RUN\0V2\0";
const EXPERIMENT_RUN_ID_V2_ENCODER_VERSION: u16 = 2;
const REFERENCE_POWER_SOURCE_SEQUENCE_HASH_DOMAIN: &[u8] =
    b"AON\0REFERENCE-POWER-SOURCE-SEQUENCE\0V1\0";
const REFERENCE_ENEMY_SEQUENCE_HASH_DOMAIN: &[u8] = b"AON\0REFERENCE-ENEMY-SEQUENCE\0V1\0";
const REFERENCE_SCENARIO_SEQUENCE_ENCODER_VERSION: u16 = 1;
const MAX_TEXT_BYTES: usize = u32::MAX as usize;

/// Hashes Scenario-owned Power Sources in their canonical semantic order.
///
/// Runtime entity identities and input artifact order are deliberately excluded.
pub fn reference_power_source_sequence_hash(
    sources: &[PowerSourceInitialState],
) -> Result<ArtifactHash, ReferenceExperimentError> {
    let mut ordered = sources.to_vec();
    ordered.sort_unstable_by_key(|source| {
        let position = source.position();
        (position.x.0, position.y.0, source.generation_per_tick().0)
    });
    let mut encoder = CanonicalEncoder::new();
    encoder.bytes(REFERENCE_POWER_SOURCE_SEQUENCE_HASH_DOMAIN);
    encoder.u16(REFERENCE_SCENARIO_SEQUENCE_ENCODER_VERSION);
    encoder.count("powerSources", ordered.len())?;
    for source in ordered {
        encoder.point(source.position());
        encoder.u64(source.generation_per_tick().0);
    }
    Ok(ArtifactHash::from_bytes(
        *blake3::hash(&encoder.finish()).as_bytes(),
    ))
}

/// Hashes Scenario-owned Enemy trajectories in their canonical semantic order.
///
/// Runtime entity identities and input artifact order are deliberately excluded.
pub fn reference_enemy_sequence_hash(
    enemies: &[EnemyInitialState],
) -> Result<ArtifactHash, ReferenceExperimentError> {
    let mut ordered = enemies.to_vec();
    ordered.sort_unstable_by_key(|enemy| {
        let position = enemy.position();
        let velocity = enemy.velocity_per_tick();
        (
            position.x.0,
            position.y.0,
            velocity.x.0,
            velocity.y.0,
            enemy.radius().0,
            enemy.integrity().0,
            enemy.heat_energy().0,
        )
    });
    let mut encoder = CanonicalEncoder::new();
    encoder.bytes(REFERENCE_ENEMY_SEQUENCE_HASH_DOMAIN);
    encoder.u16(REFERENCE_SCENARIO_SEQUENCE_ENCODER_VERSION);
    encoder.count("enemies", ordered.len())?;
    for enemy in ordered {
        encoder.point(enemy.position());
        encoder.point(enemy.velocity_per_tick());
        encoder.i64(enemy.radius().0);
        encoder.u64(enemy.integrity().0);
        encoder.u64(enemy.heat_energy().0);
    }
    Ok(ArtifactHash::from_bytes(
        *blake3::hash(&encoder.finish()).as_bytes(),
    ))
}

/// Canonical v1 identity of the shared post-build command stream, which is empty in S1-M5.
pub fn reference_empty_shared_command_log_hash() -> Result<ArtifactHash, ReferenceExperimentError> {
    reference_architecture_command_log_hash(&[])
        .map_err(|_| ReferenceExperimentError::SharedCommandLogEncoding)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReferenceArchitectureRole {
    Brute,
    Computed,
}

impl ReferenceArchitectureRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Brute => "brute",
            Self::Computed => "computed",
        }
    }

    fn canonical_tag(self) -> u8 {
        match self {
            Self::Brute => 0,
            Self::Computed => 1,
        }
    }

    fn parse(value: &str) -> Result<Self, ReferenceExperimentError> {
        match value {
            "brute" => Ok(Self::Brute),
            "computed" => Ok(Self::Computed),
            actual => Err(ReferenceExperimentError::UnsupportedDesignRole {
                actual: actual.to_owned(),
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceArtifactReference {
    path: String,
    artifact_hash: ArtifactHash,
}

impl ReferenceArtifactReference {
    pub fn new(
        path: impl Into<String>,
        artifact_hash: ArtifactHash,
    ) -> Result<Self, ReferenceExperimentError> {
        let value = Self {
            path: path.into(),
            artifact_hash,
        };
        validate_portable_path("artifact.path", &value.path)?;
        Ok(value)
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub const fn artifact_hash(&self) -> ArtifactHash {
        self.artifact_hash
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceProfileReference {
    path: String,
    profile_id: String,
    profile_hash: ProfileHash,
}

impl ReferenceProfileReference {
    pub fn new(
        path: impl Into<String>,
        profile_id: impl Into<String>,
        profile_hash: ProfileHash,
    ) -> Result<Self, ReferenceExperimentError> {
        let value = Self {
            path: path.into(),
            profile_id: profile_id.into(),
            profile_hash,
        };
        validate_portable_path("profile.path", &value.path)?;
        validate_text("profile.profileId", &value.profile_id)?;
        Ok(value)
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub const fn profile_hash(&self) -> ProfileHash {
        self.profile_hash
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceDesignBinding {
    pub role: ReferenceArchitectureRole,
    pub design: ReferenceArtifactReference,
    pub command_log_hash: ArtifactHash,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReferenceTerritoryAnchor {
    pub name: String,
    pub position: FixedVec2,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReferenceResponseBinding {
    pub name: String,
    pub hostile_entry_binding: String,
    pub defense_contact_binding: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceArchitecturePairManifest {
    format_version: u32,
    hash_algorithm_id: HashAlgorithmId,
    pair_id: String,
    scenario_id: String,
    scenario: ReferenceArtifactReference,
    contract: SimulationContract,
    numeric_profile: ReferenceProfileReference,
    physical_scale_profile: ReferenceProfileReference,
    balance_profile: ReferenceProfileReference,
    seed: Seed,
    build_end_tick: Tick,
    measurement_start_tick: Tick,
    max_ticks: Tick,
    main_core_capacity: u64,
    territory: FixedAabb,
    territory_anchors: Vec<ReferenceTerritoryAnchor>,
    power_source_sequence_hash: ArtifactHash,
    enemy_sequence_hash: ArtifactHash,
    shared_command_log_hash: ArtifactHash,
    metric_set_id: String,
    metric_set_hash: ArtifactHash,
    designs: [ReferenceDesignBinding; 2],
    response_bindings: Vec<ReferenceResponseBinding>,
}

impl ReferenceArchitecturePairManifest {
    #[allow(clippy::too_many_arguments)]
    pub fn v1(
        pair_id: impl Into<String>,
        scenario_id: impl Into<String>,
        scenario: ReferenceArtifactReference,
        contract: SimulationContract,
        numeric_profile: ReferenceProfileReference,
        physical_scale_profile: ReferenceProfileReference,
        balance_profile: ReferenceProfileReference,
        build_end_tick: Tick,
        measurement_start_tick: Tick,
        max_ticks: Tick,
        main_core_capacity: u64,
        territory: FixedAabb,
        mut territory_anchors: Vec<ReferenceTerritoryAnchor>,
        power_source_sequence_hash: ArtifactHash,
        enemy_sequence_hash: ArtifactHash,
        shared_command_log_hash: ArtifactHash,
        metric_set_id: impl Into<String>,
        metric_set_hash: ArtifactHash,
        mut designs: [ReferenceDesignBinding; 2],
        mut response_bindings: Vec<ReferenceResponseBinding>,
    ) -> Result<Self, ReferenceExperimentError> {
        territory_anchors.sort_unstable_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.position.x.0.cmp(&right.position.x.0))
                .then_with(|| left.position.y.0.cmp(&right.position.y.0))
        });
        designs.sort_unstable_by_key(|binding| binding.role);
        response_bindings.sort_unstable();
        let value = Self {
            format_version: REFERENCE_PAIR_FORMAT_VERSION_V1,
            hash_algorithm_id: HashAlgorithmId::Blake3V1,
            pair_id: pair_id.into(),
            scenario_id: scenario_id.into(),
            scenario,
            contract,
            numeric_profile,
            physical_scale_profile,
            balance_profile,
            seed: Seed::ZERO,
            build_end_tick,
            measurement_start_tick,
            max_ticks,
            main_core_capacity,
            territory,
            territory_anchors,
            power_source_sequence_hash,
            enemy_sequence_hash,
            shared_command_log_hash,
            metric_set_id: metric_set_id.into(),
            metric_set_hash,
            designs,
            response_bindings,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ReferenceExperimentError> {
        if self.format_version != REFERENCE_PAIR_FORMAT_VERSION_V1 {
            return Err(ReferenceExperimentError::UnsupportedPairFormatVersion {
                expected: REFERENCE_PAIR_FORMAT_VERSION_V1,
                actual: self.format_version,
            });
        }
        if self.hash_algorithm_id != HashAlgorithmId::Blake3V1 {
            return Err(ReferenceExperimentError::UnsupportedHashAlgorithm {
                actual: self.hash_algorithm_id.as_str().to_owned(),
            });
        }
        validate_text("pairId", &self.pair_id)?;
        validate_text("scenarioId", &self.scenario_id)?;
        validate_text("metricSetId", &self.metric_set_id)?;
        validate_portable_path("scenario.path", self.scenario.path())?;
        validate_profile_reference("numericProfile", &self.numeric_profile)?;
        validate_profile_reference("physicalScaleProfile", &self.physical_scale_profile)?;
        validate_profile_reference("balanceProfile", &self.balance_profile)?;

        if self.seed != Seed::ZERO {
            return Err(ReferenceExperimentError::NonZeroSeed);
        }
        if self.contract.semantics_version != SemanticsVersion::AonV1 {
            return Err(ReferenceExperimentError::UnsupportedSemanticsVersion {
                actual: self.contract.semantics_version.as_str().to_owned(),
            });
        }
        check_profile_hash(
            "numericProfile",
            self.contract.numeric_profile_hash,
            self.numeric_profile.profile_hash,
        )?;
        check_profile_hash(
            "physicalScaleProfile",
            self.contract.physical_scale_profile_hash,
            self.physical_scale_profile.profile_hash,
        )?;
        check_profile_hash(
            "balanceProfile",
            self.contract.balance_profile_hash,
            self.balance_profile.profile_hash,
        )?;
        let empty_shared_log = reference_empty_shared_command_log_hash()?;
        if self.shared_command_log_hash != empty_shared_log {
            return Err(ReferenceExperimentError::NonEmptySharedCommandLog {
                expected: empty_shared_log,
                actual: self.shared_command_log_hash,
            });
        }
        if self.power_source_sequence_hash == reference_power_source_sequence_hash(&[])? {
            return Err(ReferenceExperimentError::EmptyPowerSourceSequence);
        }
        if self.enemy_sequence_hash == reference_enemy_sequence_hash(&[])? {
            return Err(ReferenceExperimentError::EmptyEnemySequence);
        }
        if self.max_ticks.0 == 0 {
            return Err(ReferenceExperimentError::NonPositiveMaxTicks);
        }
        if self.build_end_tick.0 > self.measurement_start_tick.0
            || self.measurement_start_tick.0 >= self.max_ticks.0
        {
            return Err(ReferenceExperimentError::InvalidTickBoundaries {
                build_end_tick: self.build_end_tick,
                measurement_start_tick: self.measurement_start_tick,
                max_ticks: self.max_ticks,
            });
        }
        if self.main_core_capacity == 0 {
            return Err(ReferenceExperimentError::NonPositiveMainCoreCapacity);
        }
        if !self.territory.is_nonempty() {
            return Err(ReferenceExperimentError::EmptyTerritory);
        }
        validate_anchors(&self.territory_anchors, self.territory)?;
        validate_designs(&self.designs)?;
        validate_response_bindings(&self.response_bindings)?;
        Ok(())
    }

    pub fn semantic_hash(&self) -> Result<ArtifactHash, ReferenceExperimentError> {
        self.validate()?;
        let mut encoder = CanonicalEncoder::new();
        encoder.bytes(REFERENCE_PAIR_HASH_DOMAIN);
        encoder.u16(REFERENCE_PAIR_ENCODER_VERSION);
        encoder.u32(self.format_version);
        encoder.text(self.hash_algorithm_id.as_str())?;
        encoder.text(&self.pair_id)?;
        encoder.text(&self.scenario_id)?;
        encoder.bytes(self.scenario.artifact_hash.as_bytes());
        encoder.contract(self.contract)?;
        encoder.profile_reference(&self.numeric_profile)?;
        encoder.profile_reference(&self.physical_scale_profile)?;
        encoder.profile_reference(&self.balance_profile)?;
        encoder.bytes(self.seed.as_bytes());
        encoder.u64(self.build_end_tick.0);
        encoder.u64(self.measurement_start_tick.0);
        encoder.u64(self.max_ticks.0);
        encoder.u64(self.main_core_capacity);
        encoder.aabb(self.territory);
        encoder.count("territoryAnchors", self.territory_anchors.len())?;
        for anchor in &self.territory_anchors {
            encoder.text(&anchor.name)?;
            encoder.point(anchor.position);
        }
        encoder.bytes(self.power_source_sequence_hash.as_bytes());
        encoder.bytes(self.enemy_sequence_hash.as_bytes());
        encoder.bytes(self.shared_command_log_hash.as_bytes());
        encoder.text(&self.metric_set_id)?;
        encoder.bytes(self.metric_set_hash.as_bytes());
        encoder.u32(2);
        for design in &self.designs {
            encoder.u8(design.role.canonical_tag());
            encoder.bytes(design.design.artifact_hash.as_bytes());
            encoder.bytes(design.command_log_hash.as_bytes());
        }
        encoder.count("responseBindings", self.response_bindings.len())?;
        for binding in &self.response_bindings {
            encoder.text(&binding.name)?;
            encoder.text(&binding.hostile_entry_binding)?;
            encoder.text(&binding.defense_contact_binding)?;
        }
        Ok(ArtifactHash::from_bytes(
            *blake3::hash(&encoder.finish()).as_bytes(),
        ))
    }

    pub fn pair_id(&self) -> &str {
        &self.pair_id
    }
    pub const fn format_version(&self) -> u32 {
        self.format_version
    }
    pub const fn hash_algorithm_id(&self) -> HashAlgorithmId {
        self.hash_algorithm_id
    }
    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }
    pub const fn scenario(&self) -> &ReferenceArtifactReference {
        &self.scenario
    }
    pub const fn numeric_profile(&self) -> &ReferenceProfileReference {
        &self.numeric_profile
    }
    pub const fn physical_scale_profile(&self) -> &ReferenceProfileReference {
        &self.physical_scale_profile
    }
    pub const fn balance_profile(&self) -> &ReferenceProfileReference {
        &self.balance_profile
    }
    pub const fn contract(&self) -> SimulationContract {
        self.contract
    }
    pub const fn seed(&self) -> Seed {
        self.seed
    }
    pub const fn build_end_tick(&self) -> Tick {
        self.build_end_tick
    }
    pub const fn measurement_start_tick(&self) -> Tick {
        self.measurement_start_tick
    }
    pub const fn max_ticks(&self) -> Tick {
        self.max_ticks
    }
    pub const fn main_core_capacity(&self) -> u64 {
        self.main_core_capacity
    }
    pub const fn territory(&self) -> FixedAabb {
        self.territory
    }
    pub fn territory_anchors(&self) -> &[ReferenceTerritoryAnchor] {
        &self.territory_anchors
    }
    pub const fn power_source_sequence_hash(&self) -> ArtifactHash {
        self.power_source_sequence_hash
    }
    pub const fn enemy_sequence_hash(&self) -> ArtifactHash {
        self.enemy_sequence_hash
    }
    pub const fn shared_command_log_hash(&self) -> ArtifactHash {
        self.shared_command_log_hash
    }
    pub fn metric_set_id(&self) -> &str {
        &self.metric_set_id
    }
    pub const fn metric_set_hash(&self) -> ArtifactHash {
        self.metric_set_hash
    }
    pub const fn designs(&self) -> &[ReferenceDesignBinding; 2] {
        &self.designs
    }
    pub fn response_bindings(&self) -> &[ReferenceResponseBinding] {
        &self.response_bindings
    }
}

#[derive(Clone, Copy)]
pub struct ReferencePairFairnessInput<'a> {
    pub scenario: &'a ScenarioManifest,
    pub contract: SimulationContract,
    pub profiles: &'a ProfileBundle,
    pub build_end_tick: Tick,
    pub measurement_start_tick: Tick,
    pub max_ticks: Tick,
    pub main_core_capacity: u64,
    pub territory: FixedAabb,
    pub shared_command_log_hash: ArtifactHash,
    pub seed: Seed,
    pub metric_set_id: &'a str,
    pub metric_set_hash: ArtifactHash,
}

pub fn validate_reference_pair_fairness(
    pair: &ReferenceArchitecturePairManifest,
    actual: ReferencePairFairnessInput<'_>,
) -> Result<(), ReferenceExperimentError> {
    pair.validate()?;
    if pair.scenario_id != actual.scenario.scenario_id() {
        return Err(ReferenceExperimentError::ScenarioIdMismatch);
    }
    if pair.scenario.artifact_hash != actual.scenario.canonical_hash()? {
        return Err(ReferenceExperimentError::ScenarioHashMismatch);
    }
    if actual.scenario.schema_version() != SCENARIO_SCHEMA_VERSION_V4 {
        return Err(ReferenceExperimentError::ScenarioSchemaMismatch {
            expected: SCENARIO_SCHEMA_VERSION_V4,
            actual: actual.scenario.schema_version(),
        });
    }
    let (power_sources, enemies) = match actual.scenario.initial_world() {
        InitialWorld::MainCorePowerEnemyV1 {
            power_sources,
            enemies,
            ..
        } => (power_sources.as_slice(), enemies.as_slice()),
        other => {
            return Err(ReferenceExperimentError::WorldGeneratorMismatch {
                expected: WorldGeneratorVersion::MainCorePowerEnemyV1.as_str(),
                actual: initial_world_kind(other),
            });
        }
    };
    if pair.contract != actual.contract {
        return Err(ReferenceExperimentError::SimulationContractMismatch);
    }
    if actual.scenario.semantics_version() != pair.contract.semantics_version {
        return Err(ReferenceExperimentError::ScenarioContractMismatch);
    }
    for (profile, scenario_reference, pair_reference) in [
        (
            "numericProfile",
            actual.scenario.profiles().numeric(),
            &pair.numeric_profile,
        ),
        (
            "physicalScaleProfile",
            actual.scenario.profiles().physical_scale(),
            &pair.physical_scale_profile,
        ),
        (
            "balanceProfile",
            actual.scenario.profiles().balance(),
            &pair.balance_profile,
        ),
    ] {
        if scenario_reference.profile_id() != pair_reference.profile_id
            || scenario_reference.profile_hash() != pair_reference.profile_hash
        {
            return Err(ReferenceExperimentError::ScenarioProfileMismatch { profile });
        }
    }
    for (profile, expected, actual) in [
        (
            "numericProfile",
            PROFILE_SCHEMA_VERSION_V1,
            actual.profiles.numeric.schema_version,
        ),
        (
            "physicalScaleProfile",
            PROFILE_SCHEMA_VERSION_V1,
            actual.profiles.physical_scale.schema_version,
        ),
        (
            "balanceProfile",
            BALANCE_SCHEMA_VERSION_V5,
            actual.profiles.balance.schema_version,
        ),
    ] {
        if actual != expected {
            return Err(ReferenceExperimentError::ProfileSchemaMismatch {
                profile,
                expected,
                actual,
            });
        }
    }
    actual
        .profiles
        .validate()
        .map_err(|_| ReferenceExperimentError::InvalidProfileBundle)?;
    for (profile, expected, actual) in [
        (
            "numericProfile",
            pair.numeric_profile.profile_id.as_str(),
            actual.profiles.numeric.profile_id.as_str(),
        ),
        (
            "physicalScaleProfile",
            pair.physical_scale_profile.profile_id.as_str(),
            actual.profiles.physical_scale.profile_id.as_str(),
        ),
        (
            "balanceProfile",
            pair.balance_profile.profile_id.as_str(),
            actual.profiles.balance.profile_id.as_str(),
        ),
    ] {
        if actual != expected {
            return Err(ReferenceExperimentError::ProfileIdMismatch {
                profile,
                expected: expected.to_owned(),
                actual: actual.to_owned(),
            });
        }
    }
    let hashes = actual
        .profiles
        .canonical_hashes()
        .map_err(|_| ReferenceExperimentError::InvalidProfileBundle)?;
    if hashes.numeric != pair.contract.numeric_profile_hash
        || hashes.physical_scale != pair.contract.physical_scale_profile_hash
        || hashes.balance != pair.contract.balance_profile_hash
    {
        return Err(ReferenceExperimentError::SimulationContractMismatch);
    }
    let capacity = actual
        .profiles
        .balance()
        .capacity_probe
        .ok_or(ReferenceExperimentError::MissingCapacityProbe)?
        .main_core_capacity;
    if pair.main_core_capacity != actual.main_core_capacity {
        return Err(ReferenceExperimentError::MainCoreCapacityMismatch {
            expected: pair.main_core_capacity,
            actual: actual.main_core_capacity,
        });
    }
    if capacity != pair.main_core_capacity {
        return Err(ReferenceExperimentError::MainCoreCapacityMismatch {
            expected: pair.main_core_capacity,
            actual: capacity,
        });
    }
    if pair.build_end_tick != actual.build_end_tick
        || pair.measurement_start_tick != actual.measurement_start_tick
        || pair.max_ticks != actual.max_ticks
    {
        return Err(ReferenceExperimentError::TickBoundaryMismatch);
    }
    if pair.territory != actual.territory {
        return Err(ReferenceExperimentError::TerritoryMismatch);
    }
    let actual_power_source_sequence_hash = reference_power_source_sequence_hash(power_sources)?;
    if pair.power_source_sequence_hash != actual_power_source_sequence_hash {
        return Err(ReferenceExperimentError::PowerSourceSequenceMismatch);
    }
    let actual_enemy_sequence_hash = reference_enemy_sequence_hash(enemies)?;
    if pair.enemy_sequence_hash != actual_enemy_sequence_hash {
        return Err(ReferenceExperimentError::EnemySequenceMismatch);
    }
    if pair.shared_command_log_hash != actual.shared_command_log_hash {
        return Err(ReferenceExperimentError::SharedCommandLogMismatch);
    }
    if actual.seed != Seed::ZERO || pair.seed != actual.seed {
        return Err(ReferenceExperimentError::NonZeroSeed);
    }
    if pair.metric_set_id != actual.metric_set_id || pair.metric_set_hash != actual.metric_set_hash
    {
        return Err(ReferenceExperimentError::MetricSetMismatch);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceExperimentPlanV2 {
    format_version: u32,
    hash_algorithm_id: HashAlgorithmId,
    experiment_id: String,
    stage: String,
    pair: ReferenceArtifactReference,
    scenario: ReferenceArtifactReference,
    numeric_profile: ReferenceProfileReference,
    physical_scale_profile: ReferenceProfileReference,
    balance_profile: ReferenceProfileReference,
    seed: Seed,
    build_end_tick: Tick,
    measurement_start_tick: Tick,
    max_ticks: Tick,
    metric_set_id: String,
    metric_set_hash: ArtifactHash,
    designs: [ReferenceDesignBinding; 2],
}

impl ReferenceExperimentPlanV2 {
    pub fn from_pair(
        experiment_id: impl Into<String>,
        pair_reference: ReferenceArtifactReference,
        pair: &ReferenceArchitecturePairManifest,
    ) -> Result<Self, ReferenceExperimentError> {
        pair.validate()?;
        if pair_reference.artifact_hash != pair.semantic_hash()? {
            return Err(ReferenceExperimentError::PairHashMismatch);
        }
        let mut designs = pair.designs.clone();
        designs.sort_unstable_by_key(|binding| binding.design.artifact_hash);
        let value = Self {
            format_version: REFERENCE_EXPERIMENT_FORMAT_VERSION_V2,
            hash_algorithm_id: HashAlgorithmId::Blake3V1,
            experiment_id: experiment_id.into(),
            stage: REFERENCE_EXPERIMENT_STAGE_S1_M5.to_owned(),
            pair: pair_reference,
            scenario: pair.scenario.clone(),
            numeric_profile: pair.numeric_profile.clone(),
            physical_scale_profile: pair.physical_scale_profile.clone(),
            balance_profile: pair.balance_profile.clone(),
            seed: pair.seed,
            build_end_tick: pair.build_end_tick,
            measurement_start_tick: pair.measurement_start_tick,
            max_ticks: pair.max_ticks,
            metric_set_id: pair.metric_set_id.clone(),
            metric_set_hash: pair.metric_set_hash,
            designs,
        };
        value.validate_against_pair(pair)?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ReferenceExperimentError> {
        if self.format_version != REFERENCE_EXPERIMENT_FORMAT_VERSION_V2 {
            return Err(
                ReferenceExperimentError::UnsupportedExperimentFormatVersion {
                    expected: REFERENCE_EXPERIMENT_FORMAT_VERSION_V2,
                    actual: self.format_version,
                },
            );
        }
        if self.hash_algorithm_id != HashAlgorithmId::Blake3V1 {
            return Err(ReferenceExperimentError::UnsupportedHashAlgorithm {
                actual: self.hash_algorithm_id.as_str().to_owned(),
            });
        }
        validate_text("experimentId", &self.experiment_id)?;
        if self.stage != REFERENCE_EXPERIMENT_STAGE_S1_M5 {
            return Err(ReferenceExperimentError::UnsupportedExperimentStage {
                actual: self.stage.clone(),
            });
        }
        validate_portable_path("pair.path", self.pair.path())?;
        validate_portable_path("scenario.path", self.scenario.path())?;
        validate_profile_reference("numericProfile", &self.numeric_profile)?;
        validate_profile_reference("physicalScaleProfile", &self.physical_scale_profile)?;
        validate_profile_reference("balanceProfile", &self.balance_profile)?;
        if self.seed != Seed::ZERO {
            return Err(ReferenceExperimentError::NonZeroSeed);
        }
        if self.max_ticks.0 == 0 {
            return Err(ReferenceExperimentError::NonPositiveMaxTicks);
        }
        if self.build_end_tick.0 > self.measurement_start_tick.0
            || self.measurement_start_tick.0 >= self.max_ticks.0
        {
            return Err(ReferenceExperimentError::InvalidTickBoundaries {
                build_end_tick: self.build_end_tick,
                measurement_start_tick: self.measurement_start_tick,
                max_ticks: self.max_ticks,
            });
        }
        validate_text("metricSetId", &self.metric_set_id)?;
        validate_experiment_designs(&self.designs)
    }

    pub fn validate_against_pair(
        &self,
        pair: &ReferenceArchitecturePairManifest,
    ) -> Result<(), ReferenceExperimentError> {
        pair.validate()?;
        self.validate()?;
        if self.pair.artifact_hash != pair.semantic_hash()? {
            return Err(ReferenceExperimentError::PairHashMismatch);
        }
        let mut pair_designs = pair.designs.clone();
        pair_designs.sort_unstable_by_key(|binding| binding.design.artifact_hash);
        if self.scenario != pair.scenario
            || self.numeric_profile != pair.numeric_profile
            || self.physical_scale_profile != pair.physical_scale_profile
            || self.balance_profile != pair.balance_profile
            || self.seed != pair.seed
            || self.build_end_tick != pair.build_end_tick
            || self.measurement_start_tick != pair.measurement_start_tick
            || self.max_ticks != pair.max_ticks
            || self.metric_set_id != pair.metric_set_id
            || self.metric_set_hash != pair.metric_set_hash
            || self.designs != pair_designs
        {
            return Err(ReferenceExperimentError::ExperimentPairMismatch);
        }
        Ok(())
    }

    pub fn resolve(
        &self,
        pair: &ReferenceArchitecturePairManifest,
    ) -> Result<[ReferenceExperimentRunV2; 2], ReferenceExperimentError> {
        self.validate_against_pair(pair)?;
        let pair_hash = pair.semantic_hash()?;
        let runs: [Result<ReferenceExperimentRunV2, ReferenceExperimentError>; 2] =
            self.designs.clone().map(|design| {
                let run_id = experiment_run_id_v2(&ReferenceExperimentRunIdentityV2 {
                    experiment_id: &self.experiment_id,
                    pair_artifact_hash: pair_hash,
                    scenario_artifact_hash: self.scenario.artifact_hash,
                    shared_command_log_hash: pair.shared_command_log_hash,
                    design_artifact_hash: design.design.artifact_hash,
                    design_command_log_hash: design.command_log_hash,
                    contract: pair.contract,
                    seed: self.seed,
                    build_end_tick: self.build_end_tick,
                    measurement_start_tick: self.measurement_start_tick,
                    max_ticks: self.max_ticks,
                    metric_set_id: &self.metric_set_id,
                    metric_set_hash: self.metric_set_hash,
                })?;
                Ok(ReferenceExperimentRunV2 { design, run_id })
            });
        let mut resolved = [runs[0].clone()?, runs[1].clone()?];
        resolved.sort_unstable_by_key(|run| run.design.design.artifact_hash);
        if resolved[0].run_id == resolved[1].run_id {
            return Err(ReferenceExperimentError::DuplicateRunId {
                run_id: resolved[0].run_id,
            });
        }
        Ok(resolved)
    }

    pub fn experiment_id(&self) -> &str {
        &self.experiment_id
    }

    pub const fn format_version(&self) -> u32 {
        self.format_version
    }

    pub const fn hash_algorithm_id(&self) -> HashAlgorithmId {
        self.hash_algorithm_id
    }

    pub fn stage(&self) -> &str {
        &self.stage
    }

    pub fn pair(&self) -> &ReferenceArtifactReference {
        &self.pair
    }

    pub fn scenario(&self) -> &ReferenceArtifactReference {
        &self.scenario
    }

    pub fn numeric_profile(&self) -> &ReferenceProfileReference {
        &self.numeric_profile
    }

    pub fn physical_scale_profile(&self) -> &ReferenceProfileReference {
        &self.physical_scale_profile
    }

    pub fn balance_profile(&self) -> &ReferenceProfileReference {
        &self.balance_profile
    }

    pub const fn seed(&self) -> Seed {
        self.seed
    }

    pub const fn build_end_tick(&self) -> Tick {
        self.build_end_tick
    }

    pub const fn measurement_start_tick(&self) -> Tick {
        self.measurement_start_tick
    }

    pub const fn max_ticks(&self) -> Tick {
        self.max_ticks
    }

    pub fn metric_set_id(&self) -> &str {
        &self.metric_set_id
    }

    pub const fn metric_set_hash(&self) -> ArtifactHash {
        self.metric_set_hash
    }

    pub fn designs(&self) -> &[ReferenceDesignBinding; 2] {
        &self.designs
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceExperimentRunV2 {
    pub design: ReferenceDesignBinding,
    pub run_id: ExperimentRunId,
}

#[derive(Clone, Copy)]
pub struct ReferenceExperimentRunIdentityV2<'a> {
    pub experiment_id: &'a str,
    pub pair_artifact_hash: ArtifactHash,
    pub scenario_artifact_hash: ArtifactHash,
    pub shared_command_log_hash: ArtifactHash,
    pub design_artifact_hash: ArtifactHash,
    pub design_command_log_hash: ArtifactHash,
    pub contract: SimulationContract,
    pub seed: Seed,
    pub build_end_tick: Tick,
    pub measurement_start_tick: Tick,
    pub max_ticks: Tick,
    pub metric_set_id: &'a str,
    pub metric_set_hash: ArtifactHash,
}

pub fn experiment_run_id_v2(
    identity: &ReferenceExperimentRunIdentityV2<'_>,
) -> Result<ExperimentRunId, ReferenceExperimentError> {
    validate_text("experimentId", identity.experiment_id)?;
    validate_text("metricSetId", identity.metric_set_id)?;
    if identity.seed != Seed::ZERO {
        return Err(ReferenceExperimentError::NonZeroSeed);
    }
    if identity.contract.semantics_version != SemanticsVersion::AonV1 {
        return Err(ReferenceExperimentError::UnsupportedSemanticsVersion {
            actual: identity.contract.semantics_version.as_str().to_owned(),
        });
    }
    if identity.max_ticks.0 == 0
        || identity.build_end_tick.0 > identity.measurement_start_tick.0
        || identity.measurement_start_tick.0 >= identity.max_ticks.0
    {
        return Err(ReferenceExperimentError::InvalidTickBoundaries {
            build_end_tick: identity.build_end_tick,
            measurement_start_tick: identity.measurement_start_tick,
            max_ticks: identity.max_ticks,
        });
    }
    let mut encoder = CanonicalEncoder::new();
    encoder.bytes(EXPERIMENT_RUN_ID_V2_DOMAIN);
    encoder.u16(EXPERIMENT_RUN_ID_V2_ENCODER_VERSION);
    encoder.text(identity.experiment_id)?;
    encoder.bytes(identity.pair_artifact_hash.as_bytes());
    encoder.bytes(identity.scenario_artifact_hash.as_bytes());
    encoder.bytes(identity.shared_command_log_hash.as_bytes());
    encoder.bytes(identity.design_artifact_hash.as_bytes());
    encoder.bytes(identity.design_command_log_hash.as_bytes());
    encoder.contract(identity.contract)?;
    encoder.bytes(identity.seed.as_bytes());
    encoder.u64(identity.build_end_tick.0);
    encoder.u64(identity.measurement_start_tick.0);
    encoder.u64(identity.max_ticks.0);
    encoder.text(identity.metric_set_id)?;
    encoder.bytes(identity.metric_set_hash.as_bytes());
    Ok(ExperimentRunId::from_bytes(
        *blake3::hash(&encoder.finish()).as_bytes(),
    ))
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ReferenceExperimentError {
    #[error(
        "invalid Reference Experiment JSON: category={category:?}, line={line}, column={column}"
    )]
    InvalidJson {
        category: JsonErrorCategory,
        line: usize,
        column: usize,
    },
    #[error("unable to encode canonical Reference Experiment JSON")]
    JsonEncoding,
    #[error("unsupported Reference Pair format: expected {expected}, got {actual}")]
    UnsupportedPairFormatVersion { expected: u32, actual: u32 },
    #[error("unsupported Reference Experiment format: expected {expected}, got {actual}")]
    UnsupportedExperimentFormatVersion { expected: u32, actual: u32 },
    #[error("unsupported Reference Experiment hash algorithm `{actual}`")]
    UnsupportedHashAlgorithm { actual: String },
    #[error("unsupported Reference Experiment semantics `{actual}`")]
    UnsupportedSemanticsVersion { actual: String },
    #[error("unsupported Reference Experiment stage `{actual}`")]
    UnsupportedExperimentStage { actual: String },
    #[error("unsupported Reference Architecture role `{actual}`")]
    UnsupportedDesignRole { actual: String },
    #[error("Reference Experiment field `{field}` must not be empty")]
    EmptyText { field: &'static str },
    #[error("Reference Experiment field `{field}` exceeds canonical u32 length")]
    TextTooLong { field: &'static str },
    #[error("Reference Experiment path `{field}` is not portable")]
    InvalidPath { field: &'static str },
    #[error("invalid Reference Experiment hash field `{field}`: {error}")]
    InvalidHash {
        field: &'static str,
        error: HashParseError,
    },
    #[error("invalid Reference Experiment Seed: {0}")]
    InvalidSeed(SeedParseError),
    #[error("S1-M5 Reference Experiment requires Seed::ZERO")]
    NonZeroSeed,
    #[error("Reference Experiment requires positive maxTicks")]
    NonPositiveMaxTicks,
    #[error("Reference Pair requires positive Main Core capacity")]
    NonPositiveMainCoreCapacity,
    #[error(
        "invalid build/measurement/max Tick boundaries: {build_end_tick:?}/{measurement_start_tick:?}/{max_ticks:?}"
    )]
    InvalidTickBoundaries {
        build_end_tick: Tick,
        measurement_start_tick: Tick,
        max_ticks: Tick,
    },
    #[error("Reference territory AABB must be nonempty")]
    EmptyTerritory,
    #[error("Reference territory requires exactly four named anchors")]
    InvalidTerritoryAnchorCount,
    #[error("Reference territory cardinal anchor midpoint cannot be represented exactly")]
    InvalidTerritoryAnchorGeometry,
    #[error(
        "invalid Reference territory anchor: expected `{expected_name}` at {expected_position:?}, got `{actual_name}` at {actual_position:?}"
    )]
    InvalidTerritoryAnchor {
        expected_name: &'static str,
        expected_position: FixedVec2,
        actual_name: String,
        actual_position: FixedVec2,
    },
    #[error("duplicate Reference territory anchor `{name}`")]
    DuplicateTerritoryAnchor { name: String },
    #[error("Reference territory anchor `{name}` lies outside the territory")]
    TerritoryAnchorOutside { name: String },
    #[error("Reference designs must contain exactly one Brute and one Computed role")]
    InvalidDesignRoles,
    #[error("Experiment v2 designs must be ordered by ascending Artifact hash")]
    NonCanonicalDesignOrder,
    #[error("Reference Brute and Computed artifacts must differ")]
    DuplicateDesignHash,
    #[error("Reference Brute and Computed command logs must differ")]
    DuplicateDesignCommandLogHash,
    #[error("duplicate response binding `{name}`")]
    DuplicateResponseBinding { name: String },
    #[error("duplicate response semantic binding `{binding}`")]
    DuplicateResponseSemanticBinding { binding: String },
    #[error(
        "response binding `{name}` field `{field}` must use the `{expected_prefix}` namespace, got `{actual}`"
    )]
    InvalidResponseBindingNamespace {
        name: String,
        field: &'static str,
        expected_prefix: &'static str,
        actual: String,
    },
    #[error(
        "response binding `{name}` pairs sensor sector `{hostile_sector}` with defense sector `{defense_sector}`"
    )]
    ResponseBindingSectorMismatch {
        name: String,
        hostile_sector: String,
        defense_sector: String,
    },
    #[error("profile reference `{profile}` disagrees with SimulationContract")]
    ProfileHashMismatch { profile: &'static str },
    #[error("selected profile `{profile}` ID mismatch: expected `{expected}`, got `{actual}`")]
    ProfileIdMismatch {
        profile: &'static str,
        expected: String,
        actual: String,
    },
    #[error("selected profile `{profile}` schema mismatch: expected {expected}, got {actual}")]
    ProfileSchemaMismatch {
        profile: &'static str,
        expected: u32,
        actual: u32,
    },
    #[error("Scenario ID mismatch")]
    ScenarioIdMismatch,
    #[error("Scenario semantic hash mismatch")]
    ScenarioHashMismatch,
    #[error("unable to hash the selected Scenario: {0}")]
    ScenarioHash(#[from] ScenarioHashError),
    #[error("Scenario schema mismatch: expected {expected}, got {actual}")]
    ScenarioSchemaMismatch { expected: u32, actual: u32 },
    #[error("World generator mismatch: expected `{expected}`, got `{actual}`")]
    WorldGeneratorMismatch {
        expected: &'static str,
        actual: String,
    },
    #[error("Scenario semantics disagree with the Reference Pair contract")]
    ScenarioContractMismatch,
    #[error("Scenario selected profile reference `{profile}` disagrees with the Reference Pair")]
    ScenarioProfileMismatch { profile: &'static str },
    #[error("SimulationContract mismatch")]
    SimulationContractMismatch,
    #[error("invalid selected Profile bundle")]
    InvalidProfileBundle,
    #[error("selected Balance profile has no Capacity probe")]
    MissingCapacityProbe,
    #[error("Main Core Capacity mismatch: expected {expected}, got {actual}")]
    MainCoreCapacityMismatch { expected: u64, actual: u64 },
    #[error("territory mismatch")]
    TerritoryMismatch,
    #[error("build/measurement/max Tick boundaries mismatch")]
    TickBoundaryMismatch,
    #[error("Power Source sequence semantic hash mismatch")]
    PowerSourceSequenceMismatch,
    #[error("Reference Pair requires a nonempty Power Source sequence")]
    EmptyPowerSourceSequence,
    #[error("Enemy sequence semantic hash mismatch")]
    EnemySequenceMismatch,
    #[error("Reference Pair requires a nonempty Enemy sequence")]
    EmptyEnemySequence,
    #[error("shared-build Command Log hash mismatch")]
    SharedCommandLogMismatch,
    #[error("unable to encode the canonical empty shared Command Log")]
    SharedCommandLogEncoding,
    #[error(
        "Reference Pair v1 shared Command Log must be canonical-empty: expected {expected}, got {actual}"
    )]
    NonEmptySharedCommandLog {
        expected: ArtifactHash,
        actual: ArtifactHash,
    },
    #[error("Metric Set identity mismatch")]
    MetricSetMismatch,
    #[error("Reference Pair declared hash mismatch")]
    PairHashMismatch,
    #[error("Experiment v2 fields disagree with the Reference Pair")]
    ExperimentPairMismatch,
    #[error("duplicate Experiment Run ID {run_id}")]
    DuplicateRunId { run_id: ExperimentRunId },
    #[error("Reference Experiment collection `{collection}` exceeds canonical u32 length")]
    CollectionTooLong { collection: &'static str },
}

fn validate_profile_reference(
    field: &'static str,
    reference: &ReferenceProfileReference,
) -> Result<(), ReferenceExperimentError> {
    validate_portable_path(field, &reference.path)?;
    validate_text(field, &reference.profile_id)
}

fn initial_world_kind(world: &InitialWorld) -> String {
    match world {
        InitialWorld::Empty => WorldGeneratorVersion::EmptyV1,
        InitialWorld::MainCoreV1 { .. } => WorldGeneratorVersion::MainCoreV1,
        InitialWorld::MainCorePowerV1 { .. } => WorldGeneratorVersion::MainCorePowerV1,
        InitialWorld::MainCorePowerEnemyV1 { .. } => WorldGeneratorVersion::MainCorePowerEnemyV1,
    }
    .as_str()
    .to_owned()
}

fn check_profile_hash(
    profile: &'static str,
    expected: ProfileHash,
    actual: ProfileHash,
) -> Result<(), ReferenceExperimentError> {
    if expected == actual {
        Ok(())
    } else {
        Err(ReferenceExperimentError::ProfileHashMismatch { profile })
    }
}

fn validate_anchors(
    anchors: &[ReferenceTerritoryAnchor],
    territory: FixedAabb,
) -> Result<(), ReferenceExperimentError> {
    if anchors.len() != 4 {
        return Err(ReferenceExperimentError::InvalidTerritoryAnchorCount);
    }
    let midpoint = |minimum: i64, maximum: i64| {
        i128::from(minimum) + (i128::from(maximum) - i128::from(minimum)) / 2
    };
    let midpoint_x = i64::try_from(midpoint(territory.min.x.0, territory.max.x.0))
        .map_err(|_| ReferenceExperimentError::InvalidTerritoryAnchorGeometry)?;
    let midpoint_y = i64::try_from(midpoint(territory.min.y.0, territory.max.y.0))
        .map_err(|_| ReferenceExperimentError::InvalidTerritoryAnchorGeometry)?;
    let expected = [
        (
            "east",
            FixedVec2::new(crate::Fixed(territory.max.x.0), crate::Fixed(midpoint_y)),
        ),
        (
            "north",
            FixedVec2::new(crate::Fixed(midpoint_x), crate::Fixed(territory.max.y.0)),
        ),
        (
            "south",
            FixedVec2::new(crate::Fixed(midpoint_x), crate::Fixed(territory.min.y.0)),
        ),
        (
            "west",
            FixedVec2::new(crate::Fixed(territory.min.x.0), crate::Fixed(midpoint_y)),
        ),
    ];
    for (anchor, (expected_name, expected_position)) in anchors.iter().zip(expected) {
        validate_text("territoryAnchors.name", &anchor.name)?;
        if anchor.name != expected_name || anchor.position != expected_position {
            return Err(ReferenceExperimentError::InvalidTerritoryAnchor {
                expected_name,
                expected_position,
                actual_name: anchor.name.clone(),
                actual_position: anchor.position,
            });
        }
    }
    Ok(())
}

fn validate_designs(designs: &[ReferenceDesignBinding; 2]) -> Result<(), ReferenceExperimentError> {
    if designs[0].role != ReferenceArchitectureRole::Brute
        || designs[1].role != ReferenceArchitectureRole::Computed
    {
        return Err(ReferenceExperimentError::InvalidDesignRoles);
    }
    for design in designs {
        validate_portable_path("designs.path", design.design.path())?;
    }
    if designs[0].design.artifact_hash == designs[1].design.artifact_hash {
        return Err(ReferenceExperimentError::DuplicateDesignHash);
    }
    if designs[0].command_log_hash == designs[1].command_log_hash {
        return Err(ReferenceExperimentError::DuplicateDesignCommandLogHash);
    }
    Ok(())
}

fn validate_experiment_designs(
    designs: &[ReferenceDesignBinding; 2],
) -> Result<(), ReferenceExperimentError> {
    for design in designs {
        validate_portable_path("designs.path", design.design.path())?;
    }
    let roles = [designs[0].role, designs[1].role];
    if !roles.contains(&ReferenceArchitectureRole::Brute)
        || !roles.contains(&ReferenceArchitectureRole::Computed)
    {
        return Err(ReferenceExperimentError::InvalidDesignRoles);
    }
    if designs[0].design.artifact_hash == designs[1].design.artifact_hash {
        return Err(ReferenceExperimentError::DuplicateDesignHash);
    }
    if designs[0].command_log_hash == designs[1].command_log_hash {
        return Err(ReferenceExperimentError::DuplicateDesignCommandLogHash);
    }
    if designs[0].design.artifact_hash > designs[1].design.artifact_hash {
        return Err(ReferenceExperimentError::NonCanonicalDesignOrder);
    }
    Ok(())
}

fn validate_response_bindings(
    bindings: &[ReferenceResponseBinding],
) -> Result<(), ReferenceExperimentError> {
    if bindings.is_empty() {
        return Err(ReferenceExperimentError::EmptyText {
            field: "responseBindings",
        });
    }
    let mut names = BTreeSet::new();
    let mut semantic = BTreeSet::new();
    let mut previous: Option<&ReferenceResponseBinding> = None;
    for binding in bindings {
        validate_text("responseBindings.name", &binding.name)?;
        validate_text(
            "responseBindings.hostileEntryBinding",
            &binding.hostile_entry_binding,
        )?;
        validate_text(
            "responseBindings.defenseContactBinding",
            &binding.defense_contact_binding,
        )?;
        for (field, value, expected_prefix) in [
            (
                "hostileEntryBinding",
                &binding.hostile_entry_binding,
                "sensor.",
            ),
            (
                "defenseContactBinding",
                &binding.defense_contact_binding,
                "defense.",
            ),
        ] {
            if !value.starts_with(expected_prefix) || value.len() == expected_prefix.len() {
                return Err(ReferenceExperimentError::InvalidResponseBindingNamespace {
                    name: binding.name.clone(),
                    field,
                    expected_prefix,
                    actual: value.clone(),
                });
            }
        }
        let hostile_sector = binding
            .hostile_entry_binding
            .strip_prefix("sensor.")
            .expect("validated sensor namespace")
            .split('.')
            .next()
            .expect("validated nonempty namespace suffix");
        let defense_sector = binding
            .defense_contact_binding
            .strip_prefix("defense.")
            .expect("validated defense namespace")
            .split('.')
            .next()
            .expect("validated nonempty namespace suffix");
        if !matches!(hostile_sector, "east" | "north" | "south" | "west")
            || !matches!(defense_sector, "east" | "north" | "south" | "west")
            || hostile_sector != defense_sector
        {
            return Err(ReferenceExperimentError::ResponseBindingSectorMismatch {
                name: binding.name.clone(),
                hostile_sector: hostile_sector.to_owned(),
                defense_sector: defense_sector.to_owned(),
            });
        }
        if previous.is_some_and(|value| value >= binding) {
            if previous.is_some_and(|value| value == binding) {
                return Err(ReferenceExperimentError::DuplicateResponseBinding {
                    name: binding.name.clone(),
                });
            }
            return Err(ReferenceExperimentError::DuplicateResponseBinding {
                name: binding.name.clone(),
            });
        }
        if !names.insert(binding.name.clone()) {
            return Err(ReferenceExperimentError::DuplicateResponseBinding {
                name: binding.name.clone(),
            });
        }
        for value in [
            &binding.hostile_entry_binding,
            &binding.defense_contact_binding,
        ] {
            if !semantic.insert(value.clone()) {
                return Err(ReferenceExperimentError::DuplicateResponseSemanticBinding {
                    binding: value.clone(),
                });
            }
        }
        previous = Some(binding);
    }
    Ok(())
}

fn validate_text(field: &'static str, value: &str) -> Result<(), ReferenceExperimentError> {
    if value.trim().is_empty() {
        Err(ReferenceExperimentError::EmptyText { field })
    } else if value.len() > MAX_TEXT_BYTES {
        Err(ReferenceExperimentError::TextTooLong { field })
    } else {
        Ok(())
    }
}

fn validate_portable_path(
    field: &'static str,
    value: &str,
) -> Result<(), ReferenceExperimentError> {
    validate_text(field, value)?;
    let path = Path::new(value);
    let portable = !value.contains('\\')
        && !value.contains(':')
        && !value.ends_with('/')
        && !path.is_absolute()
        && path.components().all(|component| {
            matches!(
                component,
                Component::Normal(_) | Component::ParentDir | Component::CurDir
            )
        });
    if portable {
        Ok(())
    } else {
        Err(ReferenceExperimentError::InvalidPath { field })
    }
}

struct CanonicalEncoder {
    bytes: Vec<u8>,
}

impl CanonicalEncoder {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }
    fn finish(self) -> Vec<u8> {
        self.bytes
    }
    fn bytes(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }
    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }
    fn u16(&mut self, value: u16) {
        self.bytes(&value.to_le_bytes());
    }
    fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }
    fn i64(&mut self, value: i64) {
        self.bytes(&value.to_le_bytes());
    }
    fn text(&mut self, value: &str) -> Result<(), ReferenceExperimentError> {
        let length =
            u32::try_from(value.len()).map_err(|_| ReferenceExperimentError::TextTooLong {
                field: "canonicalText",
            })?;
        self.u32(length);
        self.bytes(value.as_bytes());
        Ok(())
    }
    fn count(
        &mut self,
        collection: &'static str,
        count: usize,
    ) -> Result<(), ReferenceExperimentError> {
        let value = u32::try_from(count)
            .map_err(|_| ReferenceExperimentError::CollectionTooLong { collection })?;
        self.u32(value);
        Ok(())
    }
    fn point(&mut self, value: FixedVec2) {
        self.i64(value.x.0);
        self.i64(value.y.0);
    }
    fn aabb(&mut self, value: FixedAabb) {
        self.point(value.min);
        self.point(value.max);
    }
    fn contract(&mut self, value: SimulationContract) -> Result<(), ReferenceExperimentError> {
        self.text(value.semantics_version.as_str())?;
        self.bytes(value.numeric_profile_hash.as_bytes());
        self.bytes(value.physical_scale_profile_hash.as_bytes());
        self.bytes(value.balance_profile_hash.as_bytes());
        Ok(())
    }
    fn profile_reference(
        &mut self,
        value: &ReferenceProfileReference,
    ) -> Result<(), ReferenceExperimentError> {
        self.text(&value.profile_id)?;
        self.bytes(value.profile_hash.as_bytes());
        Ok(())
    }
}

// Strict wire shapes are intentionally local so v1/v2 decoders cannot accept one another's body.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FormatEnvelope {
    format_version: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HashEnvelope {
    hash_algorithm_id: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArtifactReferenceWire {
    path: String,
    artifact_hash: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProfileReferenceWire {
    path: String,
    profile_id: String,
    profile_hash: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PointWire {
    x: i64,
    y: i64,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AabbWire {
    min: PointWire,
    max: PointWire,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AnchorWire {
    name: String,
    position: PointWire,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DesignWire {
    role: String,
    design: ArtifactReferenceWire,
    command_log_hash: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResponseBindingWire {
    name: String,
    hostile_entry_binding: String,
    defense_contact_binding: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ContractWire {
    semantics_version: String,
    numeric_profile_hash: String,
    physical_scale_profile_hash: String,
    balance_profile_hash: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PairWire {
    format_version: u32,
    hash_algorithm_id: String,
    pair_id: String,
    scenario_id: String,
    scenario: ArtifactReferenceWire,
    contract: ContractWire,
    numeric_profile: ProfileReferenceWire,
    physical_scale_profile: ProfileReferenceWire,
    balance_profile: ProfileReferenceWire,
    seed: String,
    build_end_tick: u64,
    measurement_start_tick: u64,
    max_ticks: u64,
    main_core_capacity: u64,
    territory: AabbWire,
    territory_anchors: Vec<AnchorWire>,
    power_source_sequence_hash: String,
    enemy_sequence_hash: String,
    shared_command_log_hash: String,
    metric_set_id: String,
    metric_set_hash: String,
    designs: Vec<DesignWire>,
    response_bindings: Vec<ResponseBindingWire>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExperimentPlanV2Wire {
    format_version: u32,
    hash_algorithm_id: String,
    experiment_id: String,
    stage: String,
    pair: ArtifactReferenceWire,
    scenario: ArtifactReferenceWire,
    numeric_profile: ProfileReferenceWire,
    physical_scale_profile: ProfileReferenceWire,
    balance_profile: ProfileReferenceWire,
    seed: String,
    build_end_tick: u64,
    measurement_start_tick: u64,
    max_ticks: u64,
    metric_set_id: String,
    metric_set_hash: String,
    designs: Vec<DesignWire>,
}

pub fn decode_reference_pair_manifest(
    bytes: &[u8],
) -> Result<ReferenceArchitecturePairManifest, ReferenceExperimentError> {
    let envelope: FormatEnvelope = decode_json(bytes)?;
    if envelope.format_version != REFERENCE_PAIR_FORMAT_VERSION_V1 {
        return Err(ReferenceExperimentError::UnsupportedPairFormatVersion {
            expected: REFERENCE_PAIR_FORMAT_VERSION_V1,
            actual: envelope.format_version,
        });
    }
    let hash_envelope: HashEnvelope = decode_json(bytes)?;
    if hash_envelope.hash_algorithm_id != HASH_ALGORITHM_ID_BLAKE3_V1 {
        return Err(ReferenceExperimentError::UnsupportedHashAlgorithm {
            actual: hash_envelope.hash_algorithm_id,
        });
    }
    let wire: PairWire = decode_json(bytes)?;
    let seed = Seed::from_hex(&wire.seed).map_err(ReferenceExperimentError::InvalidSeed)?;
    if seed != Seed::ZERO {
        return Err(ReferenceExperimentError::NonZeroSeed);
    }
    let designs: [ReferenceDesignBinding; 2] = wire
        .designs
        .into_iter()
        .map(design_from_wire)
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| ReferenceExperimentError::InvalidDesignRoles)?;
    ReferenceArchitecturePairManifest::v1(
        wire.pair_id,
        wire.scenario_id,
        artifact_from_wire(wire.scenario)?,
        contract_from_wire(wire.contract)?,
        profile_from_wire(wire.numeric_profile)?,
        profile_from_wire(wire.physical_scale_profile)?,
        profile_from_wire(wire.balance_profile)?,
        Tick(wire.build_end_tick),
        Tick(wire.measurement_start_tick),
        Tick(wire.max_ticks),
        wire.main_core_capacity,
        aabb_from_wire(wire.territory),
        wire.territory_anchors
            .into_iter()
            .map(|value| ReferenceTerritoryAnchor {
                name: value.name,
                position: point_from_wire(value.position),
            })
            .collect(),
        parse_artifact_hash("powerSourceSequenceHash", &wire.power_source_sequence_hash)?,
        parse_artifact_hash("enemySequenceHash", &wire.enemy_sequence_hash)?,
        parse_artifact_hash("sharedCommandLogHash", &wire.shared_command_log_hash)?,
        wire.metric_set_id,
        parse_artifact_hash("metricSetHash", &wire.metric_set_hash)?,
        designs,
        wire.response_bindings
            .into_iter()
            .map(|value| ReferenceResponseBinding {
                name: value.name,
                hostile_entry_binding: value.hostile_entry_binding,
                defense_contact_binding: value.defense_contact_binding,
            })
            .collect(),
    )
}

pub fn encode_reference_pair_manifest(
    pair: &ReferenceArchitecturePairManifest,
) -> Result<Vec<u8>, ReferenceExperimentError> {
    pair.validate()?;
    let wire = pair_to_wire(pair);
    encode_json(&wire)
}

fn pair_to_wire(pair: &ReferenceArchitecturePairManifest) -> PairWire {
    PairWire {
        format_version: pair.format_version,
        hash_algorithm_id: pair.hash_algorithm_id.as_str().to_owned(),
        pair_id: pair.pair_id.clone(),
        scenario_id: pair.scenario_id.clone(),
        scenario: artifact_to_wire(&pair.scenario),
        contract: contract_to_wire(pair.contract),
        numeric_profile: profile_to_wire(&pair.numeric_profile),
        physical_scale_profile: profile_to_wire(&pair.physical_scale_profile),
        balance_profile: profile_to_wire(&pair.balance_profile),
        seed: pair.seed.to_string(),
        build_end_tick: pair.build_end_tick.0,
        measurement_start_tick: pair.measurement_start_tick.0,
        max_ticks: pair.max_ticks.0,
        main_core_capacity: pair.main_core_capacity,
        territory: aabb_to_wire(pair.territory),
        territory_anchors: pair
            .territory_anchors
            .iter()
            .map(|value| AnchorWire {
                name: value.name.clone(),
                position: point_to_wire(value.position),
            })
            .collect(),
        power_source_sequence_hash: pair.power_source_sequence_hash.to_string(),
        enemy_sequence_hash: pair.enemy_sequence_hash.to_string(),
        shared_command_log_hash: pair.shared_command_log_hash.to_string(),
        metric_set_id: pair.metric_set_id.clone(),
        metric_set_hash: pair.metric_set_hash.to_string(),
        designs: pair.designs.iter().map(design_to_wire).collect(),
        response_bindings: pair
            .response_bindings
            .iter()
            .map(|value| ResponseBindingWire {
                name: value.name.clone(),
                hostile_entry_binding: value.hostile_entry_binding.clone(),
                defense_contact_binding: value.defense_contact_binding.clone(),
            })
            .collect(),
    }
}

pub fn decode_reference_experiment_plan_v2(
    bytes: &[u8],
) -> Result<ReferenceExperimentPlanV2, ReferenceExperimentError> {
    let envelope: FormatEnvelope = decode_json(bytes)?;
    if envelope.format_version != REFERENCE_EXPERIMENT_FORMAT_VERSION_V2 {
        return Err(
            ReferenceExperimentError::UnsupportedExperimentFormatVersion {
                expected: REFERENCE_EXPERIMENT_FORMAT_VERSION_V2,
                actual: envelope.format_version,
            },
        );
    }
    let hash_envelope: HashEnvelope = decode_json(bytes)?;
    if hash_envelope.hash_algorithm_id != HASH_ALGORITHM_ID_BLAKE3_V1 {
        return Err(ReferenceExperimentError::UnsupportedHashAlgorithm {
            actual: hash_envelope.hash_algorithm_id,
        });
    }
    let wire: ExperimentPlanV2Wire = decode_json(bytes)?;
    let designs: [ReferenceDesignBinding; 2] = wire
        .designs
        .into_iter()
        .map(design_from_wire)
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| ReferenceExperimentError::InvalidDesignRoles)?;
    let value = ReferenceExperimentPlanV2 {
        format_version: wire.format_version,
        hash_algorithm_id: HashAlgorithmId::Blake3V1,
        experiment_id: wire.experiment_id,
        stage: wire.stage,
        pair: artifact_from_wire(wire.pair)?,
        scenario: artifact_from_wire(wire.scenario)?,
        numeric_profile: profile_from_wire(wire.numeric_profile)?,
        physical_scale_profile: profile_from_wire(wire.physical_scale_profile)?,
        balance_profile: profile_from_wire(wire.balance_profile)?,
        seed: Seed::from_hex(&wire.seed).map_err(ReferenceExperimentError::InvalidSeed)?,
        build_end_tick: Tick(wire.build_end_tick),
        measurement_start_tick: Tick(wire.measurement_start_tick),
        max_ticks: Tick(wire.max_ticks),
        metric_set_id: wire.metric_set_id,
        metric_set_hash: parse_artifact_hash("metricSetHash", &wire.metric_set_hash)?,
        designs,
    };
    value.validate()?;
    Ok(value)
}

pub fn encode_reference_experiment_plan_v2(
    plan: &ReferenceExperimentPlanV2,
) -> Result<Vec<u8>, ReferenceExperimentError> {
    plan.validate()?;
    encode_json(&ExperimentPlanV2Wire {
        format_version: plan.format_version,
        hash_algorithm_id: plan.hash_algorithm_id.as_str().to_owned(),
        experiment_id: plan.experiment_id.clone(),
        stage: plan.stage.clone(),
        pair: artifact_to_wire(&plan.pair),
        scenario: artifact_to_wire(&plan.scenario),
        numeric_profile: profile_to_wire(&plan.numeric_profile),
        physical_scale_profile: profile_to_wire(&plan.physical_scale_profile),
        balance_profile: profile_to_wire(&plan.balance_profile),
        seed: plan.seed.to_string(),
        build_end_tick: plan.build_end_tick.0,
        measurement_start_tick: plan.measurement_start_tick.0,
        max_ticks: plan.max_ticks.0,
        metric_set_id: plan.metric_set_id.clone(),
        metric_set_hash: plan.metric_set_hash.to_string(),
        designs: plan.designs.iter().map(design_to_wire).collect(),
    })
}

fn decode_json<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, ReferenceExperimentError> {
    serde_json::from_slice(bytes).map_err(|error| ReferenceExperimentError::InvalidJson {
        category: JsonErrorCategory::from(error.classify()),
        line: error.line(),
        column: error.column(),
    })
}
fn encode_json<T: Serialize>(wire: &T) -> Result<Vec<u8>, ReferenceExperimentError> {
    let mut bytes =
        serde_json::to_vec_pretty(wire).map_err(|_| ReferenceExperimentError::JsonEncoding)?;
    bytes.push(b'\n');
    Ok(bytes)
}
fn parse_artifact_hash(
    field: &'static str,
    value: &str,
) -> Result<ArtifactHash, ReferenceExperimentError> {
    ArtifactHash::from_hex(value)
        .map_err(|error| ReferenceExperimentError::InvalidHash { field, error })
}
fn parse_profile_hash(
    field: &'static str,
    value: &str,
) -> Result<ProfileHash, ReferenceExperimentError> {
    ProfileHash::from_hex(value)
        .map_err(|error| ReferenceExperimentError::InvalidHash { field, error })
}
fn artifact_from_wire(
    value: ArtifactReferenceWire,
) -> Result<ReferenceArtifactReference, ReferenceExperimentError> {
    ReferenceArtifactReference::new(
        value.path,
        parse_artifact_hash("artifactHash", &value.artifact_hash)?,
    )
}
fn artifact_to_wire(value: &ReferenceArtifactReference) -> ArtifactReferenceWire {
    ArtifactReferenceWire {
        path: value.path.clone(),
        artifact_hash: value.artifact_hash.to_string(),
    }
}
fn profile_from_wire(
    value: ProfileReferenceWire,
) -> Result<ReferenceProfileReference, ReferenceExperimentError> {
    ReferenceProfileReference::new(
        value.path,
        value.profile_id,
        parse_profile_hash("profileHash", &value.profile_hash)?,
    )
}
fn profile_to_wire(value: &ReferenceProfileReference) -> ProfileReferenceWire {
    ProfileReferenceWire {
        path: value.path.clone(),
        profile_id: value.profile_id.clone(),
        profile_hash: value.profile_hash.to_string(),
    }
}
fn design_from_wire(value: DesignWire) -> Result<ReferenceDesignBinding, ReferenceExperimentError> {
    Ok(ReferenceDesignBinding {
        role: ReferenceArchitectureRole::parse(&value.role)?,
        design: artifact_from_wire(value.design)?,
        command_log_hash: parse_artifact_hash("commandLogHash", &value.command_log_hash)?,
    })
}
fn design_to_wire(value: &ReferenceDesignBinding) -> DesignWire {
    DesignWire {
        role: value.role.as_str().to_owned(),
        design: artifact_to_wire(&value.design),
        command_log_hash: value.command_log_hash.to_string(),
    }
}
fn contract_from_wire(value: ContractWire) -> Result<SimulationContract, ReferenceExperimentError> {
    Ok(SimulationContract {
        semantics_version: SemanticsVersion::parse(&value.semantics_version).map_err(|_| {
            ReferenceExperimentError::UnsupportedSemanticsVersion {
                actual: value.semantics_version,
            }
        })?,
        numeric_profile_hash: parse_profile_hash(
            "numericProfileHash",
            &value.numeric_profile_hash,
        )?,
        physical_scale_profile_hash: parse_profile_hash(
            "physicalScaleProfileHash",
            &value.physical_scale_profile_hash,
        )?,
        balance_profile_hash: parse_profile_hash(
            "balanceProfileHash",
            &value.balance_profile_hash,
        )?,
    })
}
fn contract_to_wire(value: SimulationContract) -> ContractWire {
    ContractWire {
        semantics_version: value.semantics_version.as_str().to_owned(),
        numeric_profile_hash: value.numeric_profile_hash.to_string(),
        physical_scale_profile_hash: value.physical_scale_profile_hash.to_string(),
        balance_profile_hash: value.balance_profile_hash.to_string(),
    }
}
fn point_from_wire(value: PointWire) -> FixedVec2 {
    FixedVec2::new(crate::Fixed(value.x), crate::Fixed(value.y))
}
fn point_to_wire(value: FixedVec2) -> PointWire {
    PointWire {
        x: value.x.0,
        y: value.y.0,
    }
}
fn aabb_from_wire(value: AabbWire) -> FixedAabb {
    FixedAabb::new(point_from_wire(value.min), point_from_wire(value.max))
}
fn aabb_to_wire(value: FixedAabb) -> AabbWire {
    AabbWire {
        min: point_to_wire(value.min),
        max: point_to_wire(value.max),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BalanceProfile, Energy, HeatEnergy, Integrity, NumericProfile, PhysicalScaleProfile,
    };

    fn hash(byte: u8) -> ArtifactHash {
        ArtifactHash::from_bytes([byte; 32])
    }
    fn profile(byte: u8) -> ProfileHash {
        ProfileHash::from_hex(&format!("{byte:02x}").repeat(32)).unwrap()
    }
    fn profiles() -> ProfileBundle {
        ProfileBundle {
            numeric: NumericProfile::reference_v1("n"),
            physical_scale: PhysicalScaleProfile::stage0_alpha("p"),
            balance: BalanceProfile::construction_contact_damage_alpha("b"),
        }
    }
    fn power_sources() -> Vec<PowerSourceInitialState> {
        vec![
            PowerSourceInitialState::new(
                FixedVec2::new(crate::Fixed(-10), crate::Fixed(0)),
                Energy(100),
            ),
            PowerSourceInitialState::new(
                FixedVec2::new(crate::Fixed(10), crate::Fixed(0)),
                Energy(200),
            ),
        ]
    }
    fn enemies() -> Vec<EnemyInitialState> {
        vec![
            EnemyInitialState::new(
                FixedVec2::new(crate::Fixed(0), crate::Fixed(20)),
                FixedVec2::new(crate::Fixed(0), crate::Fixed(-1)),
                crate::Fixed(2),
                Integrity(10),
                HeatEnergy(0),
            ),
            EnemyInitialState::new(
                FixedVec2::new(crate::Fixed(20), crate::Fixed(0)),
                FixedVec2::new(crate::Fixed(-1), crate::Fixed(0)),
                crate::Fixed(2),
                Integrity(10),
                HeatEnergy(0),
            ),
        ]
    }
    fn scenario(profiles: &ProfileBundle) -> ScenarioManifest {
        let sources = power_sources()
            .into_iter()
            .map(|source| {
                serde_json::json!({
                    "position": {
                        "x": source.position().x.0,
                        "y": source.position().y.0,
                    },
                    "generationPerTick": source.generation_per_tick().0,
                })
            })
            .collect::<Vec<_>>();
        let enemies = enemies()
            .into_iter()
            .map(|enemy| {
                serde_json::json!({
                    "position": {
                        "x": enemy.position().x.0,
                        "y": enemy.position().y.0,
                    },
                    "velocityPerTick": {
                        "x": enemy.velocity_per_tick().x.0,
                        "y": enemy.velocity_per_tick().y.0,
                    },
                    "radius": enemy.radius().0,
                    "integrity": enemy.integrity().0,
                    "heatEnergy": enemy.heat_energy().0,
                })
            })
            .collect::<Vec<_>>();
        let bytes = serde_json::to_vec(&serde_json::json!({
            "schemaVersion": SCENARIO_SCHEMA_VERSION_V4,
            "scenarioId": "scenario",
            "semanticsVersion": SemanticsVersion::AonV1.as_str(),
            "hashAlgorithm": HASH_ALGORITHM_ID_BLAKE3_V1,
            "initialWorld": {
                "kind": "main-core-power-enemy-v1",
                "mainCore": {
                    "position": { "x": 0, "y": 0 },
                    "integrity": 100,
                    "heatEnergy": 0,
                },
                "powerSources": sources,
                "enemies": enemies,
            },
            "requiredFeatures": {
                "signal": true,
                "mobility": true,
                "capacity": true,
                "sensing": true,
                "power": true,
                "relay": false,
                "payload": false,
                "radiation": false,
                "construction": true,
                "contact": true,
                "damage": true,
            },
            "profiles": {
                "numeric": {
                    "path": "../profiles/n.json",
                    "profileId": profiles.numeric.profile_id,
                    "profileHash": profiles.numeric.canonical_hash().unwrap().to_string(),
                },
                "physicalScale": {
                    "path": "../profiles/p.json",
                    "profileId": profiles.physical_scale.profile_id,
                    "profileHash": profiles.physical_scale.canonical_hash().unwrap().to_string(),
                },
                "balance": {
                    "path": "../profiles/b.json",
                    "profileId": profiles.balance.profile_id,
                    "profileHash": profiles.balance.canonical_hash().unwrap().to_string(),
                },
            },
        }))
        .unwrap();
        crate::decode_scenario_manifest(&bytes).unwrap()
    }
    fn pair() -> ReferenceArchitecturePairManifest {
        let profiles = profiles();
        let scenario = scenario(&profiles);
        let contract = SimulationContract::from_profiles(&profiles).unwrap();
        ReferenceArchitecturePairManifest::v1(
            "pair",
            scenario.scenario_id(),
            ReferenceArtifactReference::new(
                "../scenarios/a.json",
                scenario.canonical_hash().unwrap(),
            )
            .unwrap(),
            contract,
            ReferenceProfileReference::new(
                "../profiles/n.json",
                "n",
                profiles.numeric.canonical_hash().unwrap(),
            )
            .unwrap(),
            ReferenceProfileReference::new(
                "../profiles/p.json",
                "p",
                profiles.physical_scale.canonical_hash().unwrap(),
            )
            .unwrap(),
            ReferenceProfileReference::new(
                "../profiles/b.json",
                "b",
                profiles.balance.canonical_hash().unwrap(),
            )
            .unwrap(),
            Tick(2),
            Tick(4),
            Tick(10),
            100,
            FixedAabb::new(
                FixedVec2::new(crate::Fixed(0), crate::Fixed(0)),
                FixedVec2::new(crate::Fixed(10), crate::Fixed(10)),
            ),
            vec![
                ReferenceTerritoryAnchor {
                    name: "east".into(),
                    position: FixedVec2::new(crate::Fixed(10), crate::Fixed(5)),
                },
                ReferenceTerritoryAnchor {
                    name: "north".into(),
                    position: FixedVec2::new(crate::Fixed(5), crate::Fixed(10)),
                },
                ReferenceTerritoryAnchor {
                    name: "south".into(),
                    position: FixedVec2::new(crate::Fixed(5), crate::Fixed(0)),
                },
                ReferenceTerritoryAnchor {
                    name: "west".into(),
                    position: FixedVec2::new(crate::Fixed(0), crate::Fixed(5)),
                },
            ],
            reference_power_source_sequence_hash(&power_sources()).unwrap(),
            reference_enemy_sequence_hash(&enemies()).unwrap(),
            reference_empty_shared_command_log_hash().unwrap(),
            "metrics",
            hash(7),
            [
                ReferenceDesignBinding {
                    role: ReferenceArchitectureRole::Brute,
                    design: ReferenceArtifactReference::new("../designs/brute.json", hash(10))
                        .unwrap(),
                    command_log_hash: hash(9),
                },
                ReferenceDesignBinding {
                    role: ReferenceArchitectureRole::Computed,
                    design: ReferenceArtifactReference::new("../designs/computed.json", hash(8))
                        .unwrap(),
                    command_log_hash: hash(11),
                },
            ],
            vec![ReferenceResponseBinding {
                name: "north.0".into(),
                hostile_entry_binding: "sensor.north.0".into(),
                defense_contact_binding: "defense.north.0".into(),
            }],
        )
        .unwrap()
    }

    #[test]
    fn pair_round_trips_and_hashes_independently_of_json() {
        let pair = pair();
        let bytes = encode_reference_pair_manifest(&pair).unwrap();
        let decoded = decode_reference_pair_manifest(&bytes).unwrap();
        assert_eq!(decoded, pair);
        assert_eq!(
            decoded.semantic_hash().unwrap(),
            pair.semantic_hash().unwrap()
        );
        assert_eq!(decoded.main_core_capacity(), 100);
        assert_eq!(
            decoded.power_source_sequence_hash(),
            reference_power_source_sequence_hash(&power_sources()).unwrap()
        );
    }

    #[test]
    fn pair_envelope_errors_precede_strict_body_errors() {
        let format_error = decode_reference_pair_manifest(
            br#"{"formatVersion":2,"hashAlgorithmId":"unsupported","unknown":true}"#,
        )
        .unwrap_err();
        assert!(matches!(
            format_error,
            ReferenceExperimentError::UnsupportedPairFormatVersion {
                expected: REFERENCE_PAIR_FORMAT_VERSION_V1,
                actual: 2
            }
        ));

        let hash_error = decode_reference_pair_manifest(
            br#"{"formatVersion":1,"hashAlgorithmId":"unsupported","unknown":true}"#,
        )
        .unwrap_err();
        assert!(matches!(
            hash_error,
            ReferenceExperimentError::UnsupportedHashAlgorithm { .. }
        ));
    }

    #[test]
    fn pair_hash_binds_capacity_and_power_source_sequence() {
        let pair = pair();
        let original = pair.semantic_hash().unwrap();

        let mut changed_capacity = pair.clone();
        changed_capacity.main_core_capacity += 1;
        assert_ne!(changed_capacity.semantic_hash().unwrap(), original);

        let mut changed_power_sources = pair;
        changed_power_sources.power_source_sequence_hash = hash(99);
        assert_ne!(changed_power_sources.semantic_hash().unwrap(), original);
    }

    #[test]
    fn canonical_scenario_sequence_hashes_normalize_order_and_bind_every_field() {
        let sources = power_sources();
        let mut reversed_sources = sources.clone();
        reversed_sources.reverse();
        assert_eq!(
            reference_power_source_sequence_hash(&sources).unwrap(),
            reference_power_source_sequence_hash(&reversed_sources).unwrap()
        );
        let mut changed_sources = sources;
        changed_sources[0] = PowerSourceInitialState::new(
            changed_sources[0].position(),
            Energy(changed_sources[0].generation_per_tick().0 + 1),
        );
        assert_ne!(
            reference_power_source_sequence_hash(&changed_sources).unwrap(),
            reference_power_source_sequence_hash(&power_sources()).unwrap()
        );

        let enemy_rows = enemies();
        let mut reversed_enemies = enemy_rows.clone();
        reversed_enemies.reverse();
        assert_eq!(
            reference_enemy_sequence_hash(&enemy_rows).unwrap(),
            reference_enemy_sequence_hash(&reversed_enemies).unwrap()
        );
        let mut changed_enemies = enemy_rows;
        let original = changed_enemies[0];
        changed_enemies[0] = EnemyInitialState::new(
            original.position(),
            original.velocity_per_tick(),
            original.radius(),
            Integrity(original.integrity().0 + 1),
            original.heat_energy(),
        );
        assert_ne!(
            reference_enemy_sequence_hash(&changed_enemies).unwrap(),
            reference_enemy_sequence_hash(&enemies()).unwrap()
        );
    }

    #[test]
    fn pair_requires_exact_cardinal_anchors_empty_shared_log_and_coherent_bindings() {
        let baseline = pair();
        assert_eq!(
            baseline.shared_command_log_hash(),
            reference_empty_shared_command_log_hash().unwrap()
        );

        let mut wrong_anchor = baseline.clone();
        wrong_anchor.territory_anchors[0].position.x.0 -= 1;
        assert!(matches!(
            wrong_anchor.validate(),
            Err(ReferenceExperimentError::InvalidTerritoryAnchor {
                expected_name: "east",
                ..
            })
        ));

        let mut nonempty_shared = baseline.clone();
        nonempty_shared.shared_command_log_hash = hash(44);
        assert!(matches!(
            nonempty_shared.validate(),
            Err(ReferenceExperimentError::NonEmptySharedCommandLog { .. })
        ));

        let mut crossed_response = baseline;
        crossed_response.response_bindings[0].defense_contact_binding = "defense.south".to_owned();
        assert!(matches!(
            crossed_response.validate(),
            Err(ReferenceExperimentError::ResponseBindingSectorMismatch { .. })
        ));
    }

    #[test]
    fn fairness_checks_profile_ids_schemas_scenario_identity_and_sequence_bodies() {
        let pair = pair();
        let profiles = profiles();
        let scenario = scenario(&profiles);
        let input = ReferencePairFairnessInput {
            scenario: &scenario,
            contract: SimulationContract::from_profiles(&profiles).unwrap(),
            profiles: &profiles,
            build_end_tick: Tick(2),
            measurement_start_tick: Tick(4),
            max_ticks: Tick(10),
            main_core_capacity: 100,
            territory: pair.territory(),
            shared_command_log_hash: reference_empty_shared_command_log_hash().unwrap(),
            seed: Seed::ZERO,
            metric_set_id: "metrics",
            metric_set_hash: hash(7),
        };
        validate_reference_pair_fairness(&pair, input).unwrap();

        let legacy_scenario = crate::decode_scenario_manifest(include_bytes!(
            "../../../fixtures/scenarios/s1-m3-c22-capacity-support-v1.json"
        ))
        .unwrap();
        let mut legacy_pair = pair.clone();
        legacy_pair.scenario_id = legacy_scenario.scenario_id().to_owned();
        legacy_pair.scenario.artifact_hash = legacy_scenario.canonical_hash().unwrap();
        assert_eq!(
            validate_reference_pair_fairness(
                &legacy_pair,
                ReferencePairFairnessInput {
                    scenario: &legacy_scenario,
                    ..input
                }
            ),
            Err(ReferenceExperimentError::ScenarioSchemaMismatch {
                expected: SCENARIO_SCHEMA_VERSION_V4,
                actual: 3,
            })
        );

        let mut changed_id = profiles.clone();
        changed_id.numeric.profile_id = "other-numeric".to_owned();
        assert!(matches!(
            validate_reference_pair_fairness(
                &pair,
                ReferencePairFairnessInput {
                    profiles: &changed_id,
                    ..input
                }
            ),
            Err(ReferenceExperimentError::ProfileIdMismatch {
                profile: "numericProfile",
                ..
            })
        ));

        let mut changed_schema = profiles.clone();
        changed_schema.numeric.schema_version = 2;
        assert_eq!(
            validate_reference_pair_fairness(
                &pair,
                ReferencePairFairnessInput {
                    profiles: &changed_schema,
                    ..input
                }
            ),
            Err(ReferenceExperimentError::ProfileSchemaMismatch {
                profile: "numericProfile",
                expected: PROFILE_SCHEMA_VERSION_V1,
                actual: 2,
            })
        );

        let mut changed_sequence_pair = pair.clone();
        changed_sequence_pair.power_source_sequence_hash = hash(99);
        assert_eq!(
            validate_reference_pair_fairness(&changed_sequence_pair, input),
            Err(ReferenceExperimentError::PowerSourceSequenceMismatch)
        );
    }

    #[test]
    fn pair_strict_json_rejects_unknown_fields_and_nonzero_seed() {
        let pair = pair();
        let mut value: serde_json::Value =
            serde_json::from_slice(&encode_reference_pair_manifest(&pair).unwrap()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("unknown".to_owned(), serde_json::Value::Bool(true));
        assert!(matches!(
            decode_reference_pair_manifest(&serde_json::to_vec(&value).unwrap()),
            Err(ReferenceExperimentError::InvalidJson { .. })
        ));

        value.as_object_mut().unwrap().remove("unknown");
        value.as_object_mut().unwrap().insert(
            "seed".to_owned(),
            serde_json::Value::String(format!("{}1", "0".repeat(63))),
        );
        assert_eq!(
            decode_reference_pair_manifest(&serde_json::to_vec(&value).unwrap()),
            Err(ReferenceExperimentError::NonZeroSeed)
        );
    }

    #[test]
    fn v2_plan_resolves_exactly_two_distinct_runs() {
        let pair = pair();
        let reference =
            ReferenceArtifactReference::new("pair.json", pair.semantic_hash().unwrap()).unwrap();
        let plan = ReferenceExperimentPlanV2::from_pair("experiment", reference, &pair).unwrap();
        let runs = plan.resolve(&pair).unwrap();
        assert_ne!(runs[0].run_id, runs[1].run_id);
        assert!(runs[0].design.design.artifact_hash < runs[1].design.design.artifact_hash);
    }

    #[test]
    fn experiment_v2_strict_json_round_trips_canonically() {
        let pair = pair();
        let reference =
            ReferenceArtifactReference::new("pair.json", pair.semantic_hash().unwrap()).unwrap();
        let plan = ReferenceExperimentPlanV2::from_pair("experiment", reference, &pair).unwrap();

        let encoded = encode_reference_experiment_plan_v2(&plan).unwrap();
        let decoded = decode_reference_experiment_plan_v2(&encoded).unwrap();

        assert_eq!(decoded, plan);
        decoded.validate_against_pair(&pair).unwrap();
        assert_eq!(
            encode_reference_experiment_plan_v2(&decoded).unwrap(),
            encoded
        );
    }

    #[test]
    fn experiment_v2_envelope_errors_precede_strict_body_errors() {
        let format_error = decode_reference_experiment_plan_v2(
            br#"{"formatVersion":1,"hashAlgorithmId":"unsupported","unknown":true}"#,
        )
        .unwrap_err();
        assert!(matches!(
            format_error,
            ReferenceExperimentError::UnsupportedExperimentFormatVersion {
                expected: REFERENCE_EXPERIMENT_FORMAT_VERSION_V2,
                actual: 1
            }
        ));

        let hash_error = decode_reference_experiment_plan_v2(
            br#"{"formatVersion":2,"hashAlgorithmId":"unsupported","unknown":true}"#,
        )
        .unwrap_err();
        assert!(matches!(
            hash_error,
            ReferenceExperimentError::UnsupportedHashAlgorithm { .. }
        ));
    }

    #[test]
    fn experiment_v2_rejects_unknown_body_fields() {
        let pair = pair();
        let reference =
            ReferenceArtifactReference::new("pair.json", pair.semantic_hash().unwrap()).unwrap();
        let plan = ReferenceExperimentPlanV2::from_pair("experiment", reference, &pair).unwrap();
        let mut value: serde_json::Value =
            serde_json::from_slice(&encode_reference_experiment_plan_v2(&plan).unwrap()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("unknown".to_owned(), serde_json::Value::Bool(true));

        assert!(matches!(
            decode_reference_experiment_plan_v2(&serde_json::to_vec(&value).unwrap()),
            Err(ReferenceExperimentError::InvalidJson { .. })
        ));
    }

    #[test]
    fn experiment_v2_retains_pair_hash_and_resolve_revalidates_it() {
        let pair = pair();
        let reference =
            ReferenceArtifactReference::new("pair.json", pair.semantic_hash().unwrap()).unwrap();
        let plan = ReferenceExperimentPlanV2::from_pair("experiment", reference, &pair).unwrap();
        let mut value: serde_json::Value =
            serde_json::from_slice(&encode_reference_experiment_plan_v2(&plan).unwrap()).unwrap();
        value["pair"]["artifactHash"] = serde_json::Value::String(hash(99).to_string());

        let decoded =
            decode_reference_experiment_plan_v2(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(decoded.pair().artifact_hash(), hash(99));
        assert_eq!(
            decoded.validate_against_pair(&pair),
            Err(ReferenceExperimentError::PairHashMismatch)
        );
        assert_eq!(
            decoded.resolve(&pair),
            Err(ReferenceExperimentError::PairHashMismatch)
        );
    }

    #[test]
    fn v2_run_id_has_independent_domain_and_field_sensitivity() {
        let pair = pair();
        let base = ReferenceExperimentRunIdentityV2 {
            experiment_id: "experiment",
            pair_artifact_hash: pair.semantic_hash().unwrap(),
            scenario_artifact_hash: pair.scenario.artifact_hash,
            shared_command_log_hash: pair.shared_command_log_hash,
            design_artifact_hash: pair.designs[0].design.artifact_hash,
            design_command_log_hash: pair.designs[0].command_log_hash,
            contract: pair.contract,
            seed: Seed::ZERO,
            build_end_tick: pair.build_end_tick,
            measurement_start_tick: pair.measurement_start_tick,
            max_ticks: pair.max_ticks,
            metric_set_id: &pair.metric_set_id,
            metric_set_hash: pair.metric_set_hash,
        };
        let first = experiment_run_id_v2(&base).unwrap();
        let assert_changed = |changed: ReferenceExperimentRunIdentityV2<'_>| {
            assert_ne!(first, experiment_run_id_v2(&changed).unwrap());
        };
        assert_changed(ReferenceExperimentRunIdentityV2 {
            experiment_id: "experiment-changed",
            ..base
        });
        assert_changed(ReferenceExperimentRunIdentityV2 {
            pair_artifact_hash: hash(90),
            ..base
        });
        assert_changed(ReferenceExperimentRunIdentityV2 {
            scenario_artifact_hash: hash(91),
            ..base
        });
        assert_changed(ReferenceExperimentRunIdentityV2 {
            shared_command_log_hash: hash(92),
            ..base
        });
        assert_changed(ReferenceExperimentRunIdentityV2 {
            design_artifact_hash: hash(93),
            ..base
        });
        assert_changed(ReferenceExperimentRunIdentityV2 {
            design_command_log_hash: hash(94),
            ..base
        });
        assert_changed(ReferenceExperimentRunIdentityV2 {
            contract: SimulationContract {
                numeric_profile_hash: profile(20),
                ..base.contract
            },
            ..base
        });
        assert_changed(ReferenceExperimentRunIdentityV2 {
            contract: SimulationContract {
                physical_scale_profile_hash: profile(21),
                ..base.contract
            },
            ..base
        });
        assert_changed(ReferenceExperimentRunIdentityV2 {
            contract: SimulationContract {
                balance_profile_hash: profile(22),
                ..base.contract
            },
            ..base
        });
        assert_changed(ReferenceExperimentRunIdentityV2 {
            build_end_tick: Tick(3),
            ..base
        });
        assert_changed(ReferenceExperimentRunIdentityV2 {
            measurement_start_tick: Tick(5),
            ..base
        });
        assert_changed(ReferenceExperimentRunIdentityV2 {
            max_ticks: Tick(11),
            ..base
        });
        assert_changed(ReferenceExperimentRunIdentityV2 {
            metric_set_id: "metrics-changed",
            ..base
        });
        assert_changed(ReferenceExperimentRunIdentityV2 {
            metric_set_hash: hash(95),
            ..base
        });

        let nonzero_seed = Seed::from_hex(&format!("{}1", "0".repeat(63))).unwrap();
        assert_eq!(
            experiment_run_id_v2(&ReferenceExperimentRunIdentityV2 {
                seed: nonzero_seed,
                ..base
            }),
            Err(ReferenceExperimentError::NonZeroSeed)
        );
    }
}
