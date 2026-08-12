use crate::artifact::{ProfileKind, ScenarioHashError, decode_scenario_manifest};
use crate::contract::{HASH_ALGORITHM_ID_BLAKE3_V1, HashAlgorithmId};
use crate::error::{JsonErrorCategory, PackageError};
use crate::experiment::{
    ArtifactHash, ExperimentAxis, ExperimentPlan, ExperimentPlanError, ExperimentTextField,
    GateGeometryVariant, PhysicalScaleMatrix, ResolvedExperimentPlan, gate_geometry_key,
};
use crate::hash::{HashParseError, ProfileHash};
use crate::numeric::Fixed;
use crate::profile::{
    BalanceProfile, GateFootprintTable, GatePortTable, NumericProfile, PhysicalScaleProfile,
    ProfileValidationError,
};
use crate::replay::{Seed, SeedParseError};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};
use thiserror::Error;

pub const EXPERIMENT_PLAN_FORMAT_VERSION_V1: u32 = 1;
pub const EXPERIMENT_STAGE_S1_M0: &str = "s1-m0";
pub const LONG_WIRE_DESIGN_DERIVATION_VERSION_V1: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExperimentStage {
    S1M0,
}

impl ExperimentStage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::S1M0 => EXPERIMENT_STAGE_S1_M0,
        }
    }

    fn parse(value: &str) -> Result<Self, ExperimentArtifactError> {
        match value {
            EXPERIMENT_STAGE_S1_M0 => Ok(Self::S1M0),
            actual => Err(ExperimentArtifactError::UnsupportedStage {
                expected: EXPERIMENT_STAGE_S1_M0,
                actual: actual.to_owned(),
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExperimentArtifactReference {
    path: String,
    artifact_hash: ArtifactHash,
}

impl ExperimentArtifactReference {
    pub fn path(&self) -> &str {
        &self.path
    }

    pub const fn artifact_hash(&self) -> ArtifactHash {
        self.artifact_hash
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExperimentProfileReference {
    path: String,
    profile_id: String,
    profile_hash: ProfileHash,
}

impl ExperimentProfileReference {
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
pub struct ExperimentPlanArtifact {
    format_version: u32,
    hash_algorithm_id: HashAlgorithmId,
    experiment_id: String,
    stage: ExperimentStage,
    scenario: ExperimentArtifactReference,
    long_wire_design_derivation_version: u32,
    base_physical_scale_profile: ExperimentProfileReference,
    numeric_profiles: Vec<ExperimentProfileReference>,
    balance_profiles: Vec<ExperimentProfileReference>,
    gate_geometries: Vec<GateGeometryVariant>,
    circuit_routing_pitches: Vec<Fixed>,
    world_routing_pitches: Vec<Fixed>,
    long_wire_distances: Vec<Fixed>,
    seeds: Vec<Seed>,
    max_ticks: u64,
    metric_set_id: String,
}

impl ExperimentPlanArtifact {
    pub const fn format_version(&self) -> u32 {
        self.format_version
    }

    pub fn experiment_id(&self) -> &str {
        &self.experiment_id
    }

    pub const fn stage(&self) -> ExperimentStage {
        self.stage
    }

    pub const fn hash_algorithm_id(&self) -> HashAlgorithmId {
        self.hash_algorithm_id
    }

    pub const fn scenario(&self) -> &ExperimentArtifactReference {
        &self.scenario
    }

    pub const fn long_wire_design_derivation_version(&self) -> u32 {
        self.long_wire_design_derivation_version
    }

    pub const fn base_physical_scale_profile(&self) -> &ExperimentProfileReference {
        &self.base_physical_scale_profile
    }

    pub fn numeric_profiles(&self) -> &[ExperimentProfileReference] {
        &self.numeric_profiles
    }

    pub fn balance_profiles(&self) -> &[ExperimentProfileReference] {
        &self.balance_profiles
    }

    pub fn metric_set_id(&self) -> &str {
        &self.metric_set_id
    }

    fn validate_structure(&self) -> Result<(), ExperimentArtifactError> {
        if self.format_version != EXPERIMENT_PLAN_FORMAT_VERSION_V1 {
            return Err(ExperimentArtifactError::UnsupportedFormatVersion {
                expected: EXPERIMENT_PLAN_FORMAT_VERSION_V1,
                actual: self.format_version,
            });
        }
        if self.long_wire_design_derivation_version != LONG_WIRE_DESIGN_DERIVATION_VERSION_V1 {
            return Err(
                ExperimentArtifactError::UnsupportedLongWireDesignDerivationVersion {
                    expected: LONG_WIRE_DESIGN_DERIVATION_VERSION_V1,
                    actual: self.long_wire_design_derivation_version,
                },
            );
        }
        validate_plan_text(ExperimentTextField::ExperimentId, &self.experiment_id)?;
        validate_plan_text(ExperimentTextField::MetricSetId, &self.metric_set_id)?;
        require_plan_axis(ExperimentAxis::NumericProfile, self.numeric_profiles.len())?;
        require_plan_axis(ExperimentAxis::GateGeometry, self.gate_geometries.len())?;
        require_plan_axis(
            ExperimentAxis::CircuitRoutingPitch,
            self.circuit_routing_pitches.len(),
        )?;
        require_plan_axis(
            ExperimentAxis::WorldRoutingPitch,
            self.world_routing_pitches.len(),
        )?;
        require_plan_axis(ExperimentAxis::BalanceProfile, self.balance_profiles.len())?;
        require_plan_axis(
            ExperimentAxis::LongWireDistance,
            self.long_wire_distances.len(),
        )?;
        require_plan_axis(ExperimentAxis::Seed, self.seeds.len())?;
        if self.max_ticks == 0 {
            return Err(ExperimentPlanError::NonPositiveMaxTicks.into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub struct ExperimentArtifactBytes<'a> {
    pub scenario: &'a [u8],
    pub base_physical_scale_profile: &'a [u8],
    pub numeric_profiles: &'a [&'a [u8]],
    pub balance_profiles: &'a [&'a [u8]],
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ExperimentArtifactError {
    #[error("invalid Experiment Plan JSON: category={category:?}, line={line}, column={column}")]
    InvalidJson {
        category: JsonErrorCategory,
        line: usize,
        column: usize,
    },

    #[error("unsupported Experiment Plan format version: expected {expected}, got {actual}")]
    UnsupportedFormatVersion { expected: u32, actual: u32 },

    #[error("unsupported Experiment Plan hash algorithm: expected {expected}, got `{actual}`")]
    UnsupportedHashAlgorithm {
        expected: &'static str,
        actual: String,
    },

    #[error("unsupported Experiment stage: expected `{expected}`, got `{actual}`")]
    UnsupportedStage {
        expected: &'static str,
        actual: String,
    },

    #[error("unsupported Long-wire Design derivation version: expected {expected}, got {actual}")]
    UnsupportedLongWireDesignDerivationVersion { expected: u32, actual: u32 },

    #[error("unable to encode canonical Experiment Plan JSON: {message}")]
    EncodeJson { message: String },

    #[error("Scenario hash algorithm does not match the Experiment Plan")]
    ScenarioHashAlgorithmMismatch,

    #[error("Experiment Plan field `{field}` must not be empty")]
    EmptyField { field: &'static str },

    #[error("Experiment Plan field `{field}` is not a portable relative artifact path")]
    InvalidArtifactPath { field: &'static str },

    #[error("invalid hash in Experiment Plan field `{field}`: {error}")]
    InvalidHash {
        field: &'static str,
        error: HashParseError,
    },

    #[error("invalid Seed in Experiment Plan at seeds[{index}]: {error}")]
    InvalidSeed { index: usize, error: SeedParseError },

    #[error(
        "Experiment Plan references {expected} {profile} profiles but received {actual} profile artifacts"
    )]
    ProfileArtifactCountMismatch {
        profile: ProfileKind,
        expected: usize,
        actual: usize,
    },

    #[error(transparent)]
    Scenario(#[from] PackageError),

    #[error(transparent)]
    ScenarioHash(#[from] ScenarioHashError),

    #[error("Scenario artifact hash mismatch: expected {expected}, got {actual}")]
    ScenarioArtifactHashMismatch {
        expected: ArtifactHash,
        actual: ArtifactHash,
    },

    #[error("Scenario {profile} profile reference is not present in the Experiment Plan axis")]
    ScenarioProfileMissingFromAxis { profile: ProfileKind },

    #[error(
        "invalid JSON for Experiment Plan {profile} profile[{index}]: category={category:?}, line={line}, column={column}"
    )]
    InvalidProfileJson {
        profile: ProfileKind,
        index: usize,
        category: JsonErrorCategory,
        line: usize,
        column: usize,
    },

    #[error("invalid Experiment Plan {profile} profile[{index}]: {error}")]
    InvalidProfile {
        profile: ProfileKind,
        index: usize,
        error: ProfileValidationError,
    },

    #[error(
        "Experiment Plan {profile} profile[{index}] ID mismatch: expected `{expected}`, got `{actual}`"
    )]
    ProfileIdMismatch {
        profile: ProfileKind,
        index: usize,
        expected: String,
        actual: String,
    },

    #[error(
        "Experiment Plan {profile} profile[{index}] hash mismatch: expected {expected}, got {actual}"
    )]
    ProfileHashMismatch {
        profile: ProfileKind,
        index: usize,
        expected: ProfileHash,
        actual: ProfileHash,
    },

    #[error(transparent)]
    Plan(#[from] ExperimentPlanError),
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExperimentPlanWire {
    format_version: u32,
    hash_algorithm_id: String,
    experiment_id: String,
    stage: String,
    scenario: ArtifactReferenceWire,
    long_wire_design_derivation_version: u32,
    base_physical_scale_profile: ProfileReferenceWire,
    numeric_profiles: Vec<ProfileReferenceWire>,
    physical_scale_matrix: PhysicalScaleMatrixWire,
    balance_profiles: Vec<ProfileReferenceWire>,
    long_wire_distances: Vec<Fixed>,
    seeds: Vec<String>,
    max_ticks: u64,
    metric_set_id: String,
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
struct PhysicalScaleMatrixWire {
    gate_geometries: Vec<GateGeometryWire>,
    circuit_routing_pitches: Vec<Fixed>,
    world_routing_pitches: Vec<Fixed>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GateGeometryWire {
    gate_footprints: GateFootprintTable,
    gate_port_anchors: GatePortTable,
}

pub fn decode_experiment_plan_artifact(
    bytes: &[u8],
) -> Result<ExperimentPlanArtifact, ExperimentArtifactError> {
    let wire: ExperimentPlanWire =
        serde_json::from_slice(bytes).map_err(|error| ExperimentArtifactError::InvalidJson {
            category: JsonErrorCategory::from(error.classify()),
            line: error.line(),
            column: error.column(),
        })?;
    if wire.format_version != EXPERIMENT_PLAN_FORMAT_VERSION_V1 {
        return Err(ExperimentArtifactError::UnsupportedFormatVersion {
            expected: EXPERIMENT_PLAN_FORMAT_VERSION_V1,
            actual: wire.format_version,
        });
    }
    let hash_algorithm_id = HashAlgorithmId::parse(&wire.hash_algorithm_id).map_err(|_| {
        ExperimentArtifactError::UnsupportedHashAlgorithm {
            expected: HASH_ALGORITHM_ID_BLAKE3_V1,
            actual: wire.hash_algorithm_id.clone(),
        }
    })?;
    let stage = ExperimentStage::parse(&wire.stage)?;
    if wire.long_wire_design_derivation_version != LONG_WIRE_DESIGN_DERIVATION_VERSION_V1 {
        return Err(
            ExperimentArtifactError::UnsupportedLongWireDesignDerivationVersion {
                expected: LONG_WIRE_DESIGN_DERIVATION_VERSION_V1,
                actual: wire.long_wire_design_derivation_version,
            },
        );
    }

    validate_wire_structure(&wire)?;
    let scenario = decode_artifact_reference(wire.scenario)?;
    let base_physical_scale_profile = decode_profile_reference(
        wire.base_physical_scale_profile,
        "basePhysicalScaleProfile.path",
        "basePhysicalScaleProfile.profileId",
        "basePhysicalScaleProfile.profileHash",
    )?;
    let numeric_profiles = wire
        .numeric_profiles
        .into_iter()
        .map(|reference| {
            decode_profile_reference(
                reference,
                "numericProfiles[].path",
                "numericProfiles[].profileId",
                "numericProfiles[].profileHash",
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let balance_profiles = wire
        .balance_profiles
        .into_iter()
        .map(|reference| {
            decode_profile_reference(
                reference,
                "balanceProfiles[].path",
                "balanceProfiles[].profileId",
                "balanceProfiles[].profileHash",
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let seeds = wire
        .seeds
        .iter()
        .enumerate()
        .map(|(index, seed)| {
            Seed::from_hex(seed)
                .map_err(|error| ExperimentArtifactError::InvalidSeed { index, error })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let artifact = ExperimentPlanArtifact {
        format_version: wire.format_version,
        hash_algorithm_id,
        experiment_id: wire.experiment_id,
        stage,
        scenario,
        long_wire_design_derivation_version: wire.long_wire_design_derivation_version,
        base_physical_scale_profile,
        numeric_profiles,
        balance_profiles,
        gate_geometries: wire
            .physical_scale_matrix
            .gate_geometries
            .into_iter()
            .map(|geometry| GateGeometryVariant {
                gate_footprints: geometry.gate_footprints,
                gate_port_anchors: geometry.gate_port_anchors,
            })
            .collect(),
        circuit_routing_pitches: wire.physical_scale_matrix.circuit_routing_pitches,
        world_routing_pitches: wire.physical_scale_matrix.world_routing_pitches,
        long_wire_distances: wire.long_wire_distances,
        seeds,
        max_ticks: wire.max_ticks,
        metric_set_id: wire.metric_set_id,
    };
    artifact.validate_structure()?;
    Ok(artifact)
}

pub fn encode_experiment_plan_artifact(
    artifact: &ExperimentPlanArtifact,
) -> Result<Vec<u8>, ExperimentArtifactError> {
    artifact.validate_structure()?;

    let mut numeric_profiles = artifact
        .numeric_profiles
        .iter()
        .map(profile_reference_wire)
        .collect::<Vec<_>>();
    numeric_profiles.sort_unstable_by(|left, right| {
        left.profile_hash
            .cmp(&right.profile_hash)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.profile_id.cmp(&right.profile_id))
    });
    let mut balance_profiles = artifact
        .balance_profiles
        .iter()
        .map(profile_reference_wire)
        .collect::<Vec<_>>();
    balance_profiles.sort_unstable_by(|left, right| {
        left.profile_hash
            .cmp(&right.profile_hash)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.profile_id.cmp(&right.profile_id))
    });

    let mut gate_geometries = artifact.gate_geometries.clone();
    gate_geometries.sort_unstable_by_key(|geometry| gate_geometry_key(*geometry));
    let mut circuit_routing_pitches = artifact.circuit_routing_pitches.clone();
    circuit_routing_pitches.sort_unstable();
    let mut world_routing_pitches = artifact.world_routing_pitches.clone();
    world_routing_pitches.sort_unstable();
    let mut long_wire_distances = artifact.long_wire_distances.clone();
    long_wire_distances.sort_unstable();
    let mut seeds = artifact.seeds.clone();
    seeds.sort_unstable();

    let wire = ExperimentPlanWire {
        format_version: artifact.format_version,
        hash_algorithm_id: artifact.hash_algorithm_id.as_str().to_owned(),
        experiment_id: artifact.experiment_id.clone(),
        stage: artifact.stage.as_str().to_owned(),
        scenario: ArtifactReferenceWire {
            path: artifact.scenario.path.clone(),
            artifact_hash: artifact.scenario.artifact_hash.to_string(),
        },
        long_wire_design_derivation_version: artifact.long_wire_design_derivation_version,
        base_physical_scale_profile: profile_reference_wire(&artifact.base_physical_scale_profile),
        numeric_profiles,
        physical_scale_matrix: PhysicalScaleMatrixWire {
            gate_geometries: gate_geometries
                .into_iter()
                .map(|geometry| GateGeometryWire {
                    gate_footprints: geometry.gate_footprints,
                    gate_port_anchors: geometry.gate_port_anchors,
                })
                .collect(),
            circuit_routing_pitches,
            world_routing_pitches,
        },
        balance_profiles,
        long_wire_distances,
        seeds: seeds.into_iter().map(|seed| seed.to_string()).collect(),
        max_ticks: artifact.max_ticks,
        metric_set_id: artifact.metric_set_id.clone(),
    };
    let mut encoded =
        serde_json::to_vec_pretty(&wire).map_err(|error| ExperimentArtifactError::EncodeJson {
            message: error.to_string(),
        })?;
    encoded.push(b'\n');
    Ok(encoded)
}

fn profile_reference_wire(reference: &ExperimentProfileReference) -> ProfileReferenceWire {
    ProfileReferenceWire {
        path: reference.path.clone(),
        profile_id: reference.profile_id.clone(),
        profile_hash: reference.profile_hash.to_string(),
    }
}

pub fn resolve_experiment_plan_artifact(
    artifact: &ExperimentPlanArtifact,
    bytes: ExperimentArtifactBytes<'_>,
) -> Result<ResolvedExperimentPlan, ExperimentArtifactError> {
    artifact.validate_structure()?;
    require_artifact_count(
        ProfileKind::Numeric,
        artifact.numeric_profiles.len(),
        bytes.numeric_profiles.len(),
    )?;
    require_artifact_count(
        ProfileKind::Balance,
        artifact.balance_profiles.len(),
        bytes.balance_profiles.len(),
    )?;

    let scenario = decode_scenario_manifest(bytes.scenario)?;
    if scenario.hash_algorithm() != artifact.hash_algorithm_id {
        return Err(ExperimentArtifactError::ScenarioHashAlgorithmMismatch);
    }
    let actual_scenario_hash = scenario.canonical_hash()?;
    if actual_scenario_hash != artifact.scenario.artifact_hash {
        return Err(ExperimentArtifactError::ScenarioArtifactHashMismatch {
            expected: artifact.scenario.artifact_hash,
            actual: actual_scenario_hash,
        });
    }

    let base_profile: PhysicalScaleProfile = decode_profile(
        bytes.base_physical_scale_profile,
        ProfileKind::PhysicalScale,
        0,
    )?;
    validate_profile_reference(
        &artifact.base_physical_scale_profile,
        &base_profile.profile_id,
        base_profile
            .canonical_hash()
            .map_err(|error| ExperimentArtifactError::InvalidProfile {
                profile: ProfileKind::PhysicalScale,
                index: 0,
                error,
            })?,
        ProfileKind::PhysicalScale,
        0,
    )?;

    let mut numeric_hashes = Vec::with_capacity(bytes.numeric_profiles.len());
    for (index, (reference, profile_bytes)) in artifact
        .numeric_profiles
        .iter()
        .zip(bytes.numeric_profiles)
        .enumerate()
    {
        let profile: NumericProfile = decode_profile(profile_bytes, ProfileKind::Numeric, index)?;
        let hash =
            profile
                .canonical_hash()
                .map_err(|error| ExperimentArtifactError::InvalidProfile {
                    profile: ProfileKind::Numeric,
                    index,
                    error,
                })?;
        validate_profile_reference(
            reference,
            &profile.profile_id,
            hash,
            ProfileKind::Numeric,
            index,
        )?;
        numeric_hashes.push(hash);
    }

    let mut balance_hashes = Vec::with_capacity(bytes.balance_profiles.len());
    for (index, (reference, profile_bytes)) in artifact
        .balance_profiles
        .iter()
        .zip(bytes.balance_profiles)
        .enumerate()
    {
        let profile: BalanceProfile = decode_profile(profile_bytes, ProfileKind::Balance, index)?;
        let hash =
            profile
                .canonical_hash()
                .map_err(|error| ExperimentArtifactError::InvalidProfile {
                    profile: ProfileKind::Balance,
                    index,
                    error,
                })?;
        validate_profile_reference(
            reference,
            &profile.profile_id,
            hash,
            ProfileKind::Balance,
            index,
        )?;
        balance_hashes.push(hash);
    }

    if scenario.profiles().physical_scale().profile_hash()
        != artifact.base_physical_scale_profile.profile_hash
        || scenario.profiles().physical_scale().profile_id()
            != artifact.base_physical_scale_profile.profile_id
    {
        return Err(ExperimentArtifactError::ScenarioProfileMissingFromAxis {
            profile: ProfileKind::PhysicalScale,
        });
    }
    for (profile, references, scenario_reference) in [
        (
            ProfileKind::Numeric,
            artifact.numeric_profiles.as_slice(),
            scenario.profiles().numeric(),
        ),
        (
            ProfileKind::Balance,
            artifact.balance_profiles.as_slice(),
            scenario.profiles().balance(),
        ),
    ] {
        if !references.iter().any(|reference| {
            reference.profile_hash == scenario_reference.profile_hash()
                && reference.profile_id == scenario_reference.profile_id()
        }) {
            return Err(ExperimentArtifactError::ScenarioProfileMissingFromAxis { profile });
        }
    }

    ExperimentPlan {
        experiment_id: artifact.experiment_id.clone(),
        scenario_artifact_hash: artifact.scenario.artifact_hash,
        physical_scale_matrix: PhysicalScaleMatrix {
            base_profile,
            gate_geometries: artifact.gate_geometries.clone(),
            circuit_routing_pitches: artifact.circuit_routing_pitches.clone(),
            world_routing_pitches: artifact.world_routing_pitches.clone(),
        },
        long_wire_distances: artifact.long_wire_distances.clone(),
        numeric_profile_hashes: numeric_hashes,
        balance_profile_hashes: balance_hashes,
        seeds: artifact.seeds.clone(),
        max_ticks: artifact.max_ticks,
        metric_set_id: artifact.metric_set_id.clone(),
    }
    .resolve()
    .map_err(ExperimentArtifactError::from)
}

fn validate_wire_structure(wire: &ExperimentPlanWire) -> Result<(), ExperimentArtifactError> {
    validate_plan_text(ExperimentTextField::ExperimentId, &wire.experiment_id)?;
    validate_plan_text(ExperimentTextField::MetricSetId, &wire.metric_set_id)?;
    require_plan_axis(ExperimentAxis::NumericProfile, wire.numeric_profiles.len())?;
    require_plan_axis(
        ExperimentAxis::GateGeometry,
        wire.physical_scale_matrix.gate_geometries.len(),
    )?;
    require_plan_axis(
        ExperimentAxis::CircuitRoutingPitch,
        wire.physical_scale_matrix.circuit_routing_pitches.len(),
    )?;
    require_plan_axis(
        ExperimentAxis::WorldRoutingPitch,
        wire.physical_scale_matrix.world_routing_pitches.len(),
    )?;
    require_plan_axis(ExperimentAxis::BalanceProfile, wire.balance_profiles.len())?;
    require_plan_axis(
        ExperimentAxis::LongWireDistance,
        wire.long_wire_distances.len(),
    )?;
    require_plan_axis(ExperimentAxis::Seed, wire.seeds.len())?;
    Ok(())
}

fn require_plan_axis(axis: ExperimentAxis, length: usize) -> Result<(), ExperimentArtifactError> {
    if length == 0 {
        Err(ExperimentPlanError::EmptyAxis { axis }.into())
    } else {
        Ok(())
    }
}

fn decode_artifact_reference(
    wire: ArtifactReferenceWire,
) -> Result<ExperimentArtifactReference, ExperimentArtifactError> {
    validate_artifact_path("scenario.path", &wire.path)?;
    let artifact_hash = ArtifactHash::from_hex(&wire.artifact_hash).map_err(|error| {
        ExperimentArtifactError::InvalidHash {
            field: "scenario.artifactHash",
            error,
        }
    })?;
    Ok(ExperimentArtifactReference {
        path: wire.path,
        artifact_hash,
    })
}

fn decode_profile_reference(
    wire: ProfileReferenceWire,
    path_field: &'static str,
    id_field: &'static str,
    hash_field: &'static str,
) -> Result<ExperimentProfileReference, ExperimentArtifactError> {
    validate_artifact_path(path_field, &wire.path)?;
    validate_nonempty(id_field, &wire.profile_id)?;
    let profile_hash = ProfileHash::from_hex(&wire.profile_hash).map_err(|error| {
        ExperimentArtifactError::InvalidHash {
            field: hash_field,
            error,
        }
    })?;
    Ok(ExperimentProfileReference {
        path: wire.path,
        profile_id: wire.profile_id,
        profile_hash,
    })
}

fn decode_profile<T>(
    bytes: &[u8],
    profile: ProfileKind,
    index: usize,
) -> Result<T, ExperimentArtifactError>
where
    T: for<'de> Deserialize<'de> + ValidateExperimentProfile,
{
    let decoded: T = serde_json::from_slice(bytes).map_err(|error| {
        ExperimentArtifactError::InvalidProfileJson {
            profile,
            index,
            category: JsonErrorCategory::from(error.classify()),
            line: error.line(),
            column: error.column(),
        }
    })?;
    decoded.validate_experiment_profile().map_err(|error| {
        ExperimentArtifactError::InvalidProfile {
            profile,
            index,
            error,
        }
    })?;
    Ok(decoded)
}

trait ValidateExperimentProfile {
    fn validate_experiment_profile(&self) -> Result<(), ProfileValidationError>;
}

impl ValidateExperimentProfile for NumericProfile {
    fn validate_experiment_profile(&self) -> Result<(), ProfileValidationError> {
        self.validate()
    }
}

impl ValidateExperimentProfile for PhysicalScaleProfile {
    fn validate_experiment_profile(&self) -> Result<(), ProfileValidationError> {
        self.validate()
    }
}

impl ValidateExperimentProfile for BalanceProfile {
    fn validate_experiment_profile(&self) -> Result<(), ProfileValidationError> {
        self.validate()
    }
}

fn validate_profile_reference(
    reference: &ExperimentProfileReference,
    actual_id: &str,
    actual_hash: ProfileHash,
    profile: ProfileKind,
    index: usize,
) -> Result<(), ExperimentArtifactError> {
    if reference.profile_id != actual_id {
        return Err(ExperimentArtifactError::ProfileIdMismatch {
            profile,
            index,
            expected: reference.profile_id.clone(),
            actual: actual_id.to_owned(),
        });
    }
    if reference.profile_hash != actual_hash {
        return Err(ExperimentArtifactError::ProfileHashMismatch {
            profile,
            index,
            expected: reference.profile_hash,
            actual: actual_hash,
        });
    }
    Ok(())
}

fn require_artifact_count(
    profile: ProfileKind,
    expected: usize,
    actual: usize,
) -> Result<(), ExperimentArtifactError> {
    if expected == actual {
        Ok(())
    } else {
        Err(ExperimentArtifactError::ProfileArtifactCountMismatch {
            profile,
            expected,
            actual,
        })
    }
}

fn validate_nonempty(field: &'static str, value: &str) -> Result<(), ExperimentArtifactError> {
    if value.trim().is_empty() {
        Err(ExperimentArtifactError::EmptyField { field })
    } else {
        Ok(())
    }
}

fn validate_plan_text(
    field: ExperimentTextField,
    value: &str,
) -> Result<(), ExperimentArtifactError> {
    if value.trim().is_empty() {
        Err(ExperimentPlanError::EmptyTextField { field }.into())
    } else {
        Ok(())
    }
}

fn validate_artifact_path(field: &'static str, value: &str) -> Result<(), ExperimentArtifactError> {
    validate_nonempty(field, value)?;
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
        Err(ExperimentArtifactError::InvalidArtifactPath { field })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZERO_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
    const SCENARIO_HASH: &str = "46a41702ea9dd3f404aa50f0c4952e5d773472c9a7f3410e8cacc8d68bde9ddd";

    fn minimal_plan(extra: &str) -> Vec<u8> {
        format!(
            r#"{{
                "formatVersion":1,
                "hashAlgorithmId":"blake3-v1",
                "experimentId":"s1-m0",
                "stage":"s1-m0",
                "scenario":{{"path":"../scenarios/empty.json","artifactHash":"{ZERO_HASH}"}},
                "longWireDesignDerivationVersion":1,
                "basePhysicalScaleProfile":{{"path":"../../profiles/physical-scale/stage0-alpha.json","profileId":"physical","profileHash":"{ZERO_HASH}"}},
                "numericProfiles":[],
                "balanceProfiles":[],
                "physicalScaleMatrix":{{"gateGeometries":[],"circuitRoutingPitches":[],"worldRoutingPitches":[]}},
                "longWireDistances":[],
                "seeds":[],
                "maxTicks":1,
                "metricSetId":"timing-v1"{extra}
            }}"#
        )
        .into_bytes()
    }

    fn retained_plan(extra: &str) -> Vec<u8> {
        let source =
            include_str!("../../../fixtures/experiments/s1-m0-physical-scale-v1.json").trim_end();
        let closing_brace = source.rfind('}').expect("retained plan is a JSON object");
        format!(
            "{}{}{}\n",
            &source[..closing_brace],
            extra,
            &source[closing_brace..]
        )
        .into_bytes()
    }

    #[test]
    fn retained_plan_strictly_decodes_and_canonically_reencodes() {
        let retained = retained_plan("");
        let artifact = decode_experiment_plan_artifact(&retained).expect("retained plan decodes");
        assert_eq!(artifact.format_version(), EXPERIMENT_PLAN_FORMAT_VERSION_V1);
        assert_eq!(artifact.stage(), ExperimentStage::S1M0);
        assert_eq!(
            artifact.long_wire_design_derivation_version(),
            LONG_WIRE_DESIGN_DERIVATION_VERSION_V1
        );
        let encoded = encode_experiment_plan_artifact(&artifact).expect("retained plan encodes");
        assert!(encoded.ends_with(b"\n"));
        assert!(!encoded.windows(2).any(|bytes| bytes == b"\r\n"));
        assert_eq!(encoded, retained);
        assert_eq!(
            encode_experiment_plan_artifact(
                &decode_experiment_plan_artifact(&encoded).expect("canonical plan decodes")
            )
            .expect("canonical plan re-encodes"),
            encoded
        );

        let mut permuted: serde_json::Value =
            serde_json::from_slice(&retained).expect("retained plan is JSON");
        for pointer in [
            "/numericProfiles",
            "/balanceProfiles",
            "/physicalScaleMatrix/gateGeometries",
            "/physicalScaleMatrix/circuitRoutingPitches",
            "/physicalScaleMatrix/worldRoutingPitches",
            "/longWireDistances",
            "/seeds",
        ] {
            permuted
                .pointer_mut(pointer)
                .and_then(serde_json::Value::as_array_mut)
                .expect("axis exists")
                .reverse();
        }
        let permuted = serde_json::to_vec(&permuted).expect("permuted plan serializes");
        assert_eq!(
            encode_experiment_plan_artifact(
                &decode_experiment_plan_artifact(&permuted).expect("permuted plan decodes")
            )
            .expect("permuted plan canonically encodes"),
            retained
        );
    }

    #[test]
    fn format_stage_and_design_derivation_versions_are_required_and_strict() {
        let old_name = String::from_utf8(retained_plan("")).unwrap().replacen(
            "\"formatVersion\"",
            "\"schemaVersion\"",
            1,
        );
        assert!(matches!(
            decode_experiment_plan_artifact(old_name.as_bytes()),
            Err(ExperimentArtifactError::InvalidJson { .. })
        ));

        let unsupported_format = String::from_utf8(retained_plan("")).unwrap().replacen(
            "\"formatVersion\": 1",
            "\"formatVersion\": 2",
            1,
        );
        assert_eq!(
            decode_experiment_plan_artifact(unsupported_format.as_bytes()),
            Err(ExperimentArtifactError::UnsupportedFormatVersion {
                expected: EXPERIMENT_PLAN_FORMAT_VERSION_V1,
                actual: 2,
            })
        );

        let unsupported_stage = String::from_utf8(retained_plan("")).unwrap().replacen(
            "\"stage\": \"s1-m0\"",
            "\"stage\": \"s1-m1\"",
            1,
        );
        assert_eq!(
            decode_experiment_plan_artifact(unsupported_stage.as_bytes()),
            Err(ExperimentArtifactError::UnsupportedStage {
                expected: EXPERIMENT_STAGE_S1_M0,
                actual: "s1-m1".to_owned(),
            })
        );

        let unsupported_derivation = String::from_utf8(retained_plan("")).unwrap().replacen(
            "\"longWireDesignDerivationVersion\": 1",
            "\"longWireDesignDerivationVersion\": 2",
            1,
        );
        assert_eq!(
            decode_experiment_plan_artifact(unsupported_derivation.as_bytes()),
            Err(
                ExperimentArtifactError::UnsupportedLongWireDesignDerivationVersion {
                    expected: LONG_WIRE_DESIGN_DERIVATION_VERSION_V1,
                    actual: 2,
                }
            )
        );

        let unsupported_algorithm = String::from_utf8(retained_plan("")).unwrap().replacen(
            "\"hashAlgorithmId\": \"blake3-v1\"",
            "\"hashAlgorithmId\": \"sha256\"",
            1,
        );
        assert_eq!(
            decode_experiment_plan_artifact(unsupported_algorithm.as_bytes()),
            Err(ExperimentArtifactError::UnsupportedHashAlgorithm {
                expected: HASH_ALGORITHM_ID_BLAKE3_V1,
                actual: "sha256".to_owned(),
            })
        );

        let malformed_seed =
            String::from_utf8(retained_plan(""))
                .unwrap()
                .replacen(&"0".repeat(64), "ABC", 1);
        assert!(matches!(
            decode_experiment_plan_artifact(malformed_seed.as_bytes()),
            Err(ExperimentArtifactError::InvalidSeed { index: 0, .. })
        ));
    }

    #[test]
    fn strict_json_rejects_unknown_duplicate_and_float_fields() {
        assert!(matches!(
            decode_experiment_plan_artifact(&retained_plan(",\"extra\":0")),
            Err(ExperimentArtifactError::InvalidJson { .. })
        ));
        let duplicate = String::from_utf8(retained_plan("")).unwrap().replacen(
            "\"formatVersion\": 1,",
            "\"formatVersion\": 1,\"formatVersion\": 1,",
            1,
        );
        assert!(matches!(
            decode_experiment_plan_artifact(duplicate.as_bytes()),
            Err(ExperimentArtifactError::InvalidJson { .. })
        ));
        let float = String::from_utf8(retained_plan("")).unwrap().replacen(
            "\"maxTicks\": 4096",
            "\"maxTicks\": 4096.0",
            1,
        );
        assert!(matches!(
            decode_experiment_plan_artifact(float.as_bytes()),
            Err(ExperimentArtifactError::InvalidJson { .. })
        ));

        let mut trailing = retained_plan("");
        trailing.extend_from_slice(b"{}\n");
        assert!(matches!(
            decode_experiment_plan_artifact(&trailing),
            Err(ExperimentArtifactError::InvalidJson { .. })
        ));

        let overflowing_distance = String::from_utf8(retained_plan("")).unwrap().replacen(
            "4194304",
            "9223372036854775808",
            1,
        );
        assert!(matches!(
            decode_experiment_plan_artifact(overflowing_distance.as_bytes()),
            Err(ExperimentArtifactError::InvalidJson { .. })
        ));
    }

    #[test]
    fn references_must_be_portable_and_hashes_lowercase() {
        let absolute = String::from_utf8(retained_plan("")).unwrap().replacen(
            "../scenarios/empty.json",
            "C:/fixtures/empty.json",
            1,
        );
        assert!(matches!(
            decode_experiment_plan_artifact(absolute.as_bytes()),
            Err(ExperimentArtifactError::InvalidArtifactPath {
                field: "scenario.path"
            })
        ));

        let uppercase = String::from_utf8(retained_plan("")).unwrap().replacen(
            SCENARIO_HASH,
            &format!("A{}", &SCENARIO_HASH[1..]),
            1,
        );
        assert!(matches!(
            decode_experiment_plan_artifact(uppercase.as_bytes()),
            Err(ExperimentArtifactError::InvalidHash {
                field: "scenario.artifactHash",
                ..
            })
        ));
    }

    #[test]
    fn structural_preflight_precedes_referenced_content_and_max_ticks() {
        for (source, field) in [
            (
                String::from_utf8(retained_plan("")).unwrap().replacen(
                    "\"experimentId\": \"s1-m0-physical-scale-v1\"",
                    "\"experimentId\": \" \"",
                    1,
                ),
                ExperimentTextField::ExperimentId,
            ),
            (
                String::from_utf8(retained_plan("")).unwrap().replacen(
                    "\"metricSetId\": \"s1-m0-timing-area-v1\"",
                    "\"metricSetId\": \"\"",
                    1,
                ),
                ExperimentTextField::MetricSetId,
            ),
        ] {
            assert_eq!(
                decode_experiment_plan_artifact(source.as_bytes()),
                Err(ExperimentArtifactError::Plan(
                    ExperimentPlanError::EmptyTextField { field }
                ))
            );
        }

        let malformed_reference = String::from_utf8(minimal_plan("")).unwrap().replacen(
            "../scenarios/empty.json",
            "C:/outside.json",
            1,
        );
        assert_eq!(
            decode_experiment_plan_artifact(malformed_reference.as_bytes()),
            Err(ExperimentArtifactError::Plan(
                ExperimentPlanError::EmptyAxis {
                    axis: ExperimentAxis::NumericProfile,
                }
            ))
        );

        let zero_ticks = String::from_utf8(retained_plan("")).unwrap().replacen(
            "\"maxTicks\": 4096",
            "\"maxTicks\": 0",
            1,
        );
        assert_eq!(
            decode_experiment_plan_artifact(zero_ticks.as_bytes()),
            Err(ExperimentArtifactError::Plan(
                ExperimentPlanError::NonPositiveMaxTicks
            ))
        );

        let malformed_hash_before_ticks =
            zero_ticks.replacen(SCENARIO_HASH, &format!("A{}", &SCENARIO_HASH[1..]), 1);
        assert!(matches!(
            decode_experiment_plan_artifact(malformed_hash_before_ticks.as_bytes()),
            Err(ExperimentArtifactError::InvalidHash {
                field: "scenario.artifactHash",
                ..
            })
        ));
    }

    #[test]
    fn direct_resolution_rechecks_structure_before_artifact_bytes() {
        let mut artifact =
            decode_experiment_plan_artifact(&retained_plan("")).expect("retained plan decodes");
        artifact.numeric_profiles.clear();
        let garbage = b"not-json";
        let no_profiles: &[&[u8]] = &[];

        assert_eq!(
            resolve_experiment_plan_artifact(
                &artifact,
                ExperimentArtifactBytes {
                    scenario: garbage,
                    base_physical_scale_profile: garbage,
                    numeric_profiles: no_profiles,
                    balance_profiles: no_profiles,
                }
            ),
            Err(ExperimentArtifactError::Plan(
                ExperimentPlanError::EmptyAxis {
                    axis: ExperimentAxis::NumericProfile,
                }
            ))
        );
    }

    #[test]
    fn artifact_backed_resolution_rejects_schema_kind_invariant_id_and_hash_mismatches() {
        const SCENARIO: &[u8] = include_bytes!("../../../fixtures/scenarios/empty.json");
        const PHYSICAL: &[u8] =
            include_bytes!("../../../profiles/physical-scale/stage0-alpha.json");
        const NUMERIC: &[u8] = include_bytes!("../../../profiles/numeric/v1.json");
        const BALANCE: &[u8] = include_bytes!("../../../profiles/balance/stage0-alpha.json");

        let artifact =
            decode_experiment_plan_artifact(&retained_plan("")).expect("retained plan decodes");
        let resolve = |artifact: &ExperimentPlanArtifact, scenario: &[u8], numeric: &[u8]| {
            resolve_experiment_plan_artifact(
                artifact,
                ExperimentArtifactBytes {
                    scenario,
                    base_physical_scale_profile: PHYSICAL,
                    numeric_profiles: &[numeric],
                    balance_profiles: &[BALANCE],
                },
            )
        };

        let wrong_scenario_schema = String::from_utf8(SCENARIO.to_vec()).unwrap().replacen(
            "\"schemaVersion\": 1",
            "\"schemaVersion\": 3",
            1,
        );
        assert!(matches!(
            resolve(&artifact, wrong_scenario_schema.as_bytes(), NUMERIC),
            Err(ExperimentArtifactError::Scenario(
                PackageError::UnsupportedSchema { .. }
            ))
        ));

        let wrong_profile_schema = String::from_utf8(NUMERIC.to_vec()).unwrap().replacen(
            "\"schemaVersion\": 1",
            "\"schemaVersion\": 2",
            1,
        );
        assert!(matches!(
            resolve(&artifact, SCENARIO, wrong_profile_schema.as_bytes()),
            Err(ExperimentArtifactError::InvalidProfile {
                profile: ProfileKind::Numeric,
                index: 0,
                error: ProfileValidationError::UnsupportedSchema { .. },
            })
        ));

        let wrong_kind = String::from_utf8(NUMERIC.to_vec()).unwrap().replacen(
            "\"kind\": \"numeric\"",
            "\"kind\": \"balance\"",
            1,
        );
        assert!(matches!(
            resolve(&artifact, SCENARIO, wrong_kind.as_bytes()),
            Err(ExperimentArtifactError::InvalidProfile {
                profile: ProfileKind::Numeric,
                index: 0,
                error: ProfileValidationError::ProfileKindMismatch { .. },
            })
        ));

        let invalid_invariant = String::from_utf8(NUMERIC.to_vec()).unwrap().replacen(
            "\"fixedOne\": 65536",
            "\"fixedOne\": 0",
            1,
        );
        assert!(matches!(
            resolve(&artifact, SCENARIO, invalid_invariant.as_bytes()),
            Err(ExperimentArtifactError::InvalidProfile {
                profile: ProfileKind::Numeric,
                index: 0,
                error: ProfileValidationError::FixedOneMismatch { .. },
            })
        ));

        let changed_id = String::from_utf8(NUMERIC.to_vec()).unwrap().replacen(
            "numeric-v1",
            "numeric-relocated-id",
            1,
        );
        assert!(matches!(
            resolve(&artifact, SCENARIO, changed_id.as_bytes()),
            Err(ExperimentArtifactError::ProfileIdMismatch {
                profile: ProfileKind::Numeric,
                index: 0,
                ..
            })
        ));

        let mut wrong_hash = artifact;
        wrong_hash.numeric_profiles[0].profile_hash = ProfileHash::from_bytes([0x99; 32]);
        assert!(matches!(
            resolve(&wrong_hash, SCENARIO, NUMERIC),
            Err(ExperimentArtifactError::ProfileHashMismatch {
                profile: ProfileKind::Numeric,
                index: 0,
                ..
            })
        ));
    }
}
