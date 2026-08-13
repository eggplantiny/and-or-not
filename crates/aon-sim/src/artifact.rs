use crate::contract::{
    HASH_ALGORITHM_ID_BLAKE3_V1, HashAlgorithmId, SEMANTICS_VERSION_V1, SemanticsVersion,
};
use crate::profile::{
    BalanceProfile, NumericProfile, PhysicalScaleProfile, ProfileBundle, ProfileValidationError,
};
use crate::{
    ArtifactHash, Energy, Fixed, FixedVec2, HeatEnergy, Integrity, JsonErrorCategory, ProfileHash,
};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

pub const SCENARIO_SCHEMA_VERSION_V1: u32 = 1;
pub const SCENARIO_SCHEMA_VERSION_V2: u32 = 2;
pub const SCENARIO_SCHEMA_VERSION_V3: u32 = 3;
const SCENARIO_HASH_DOMAIN_V1: &[u8] = b"AON\0SCENARIO\0V1\0";
const SCENARIO_HASH_DOMAIN_V2: &[u8] = b"AON\0SCENARIO\0V2\0";
const SCENARIO_HASH_DOMAIN_V3: &[u8] = b"AON\0SCENARIO\0V3\0";
const SCENARIO_HASH_ENCODER_VERSION_V1: u16 = 1;
const SCENARIO_HASH_ENCODER_VERSION_V2: u16 = 2;
const SCENARIO_HASH_ENCODER_VERSION_V3: u16 = 3;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileKind {
    Numeric,
    PhysicalScale,
    Balance,
}

impl ProfileKind {
    pub(crate) const fn canonical_tag(self) -> u8 {
        match self {
            Self::Numeric => 0,
            Self::PhysicalScale => 1,
            Self::Balance => 2,
        }
    }
}

impl fmt::Display for ProfileKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Numeric => "numeric",
            Self::PhysicalScale => "physical-scale",
            Self::Balance => "balance",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactKind {
    Scenario,
    Profile(ProfileKind),
}

impl fmt::Display for ArtifactKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scenario => formatter.write_str("scenario"),
            Self::Profile(kind) => write!(formatter, "{kind} profile"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitialWorld {
    Empty,
    MainCoreV1 {
        position: FixedVec2,
        integrity: Integrity,
        heat_energy: HeatEnergy,
    },
    MainCorePowerV1 {
        main_core_position: FixedVec2,
        main_core_integrity: Integrity,
        main_core_heat_energy: HeatEnergy,
        power_sources: Vec<PowerSourceInitialState>,
    },
}

/// Immutable world-generator input for one Scenario-owned Power Source.
///
/// Scenario v3 canonicalizes the collection by semantic value before exposing it. Entity IDs and
/// topology attachment identities are assigned later by world generation, not by artifact order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerSourceInitialState {
    position: FixedVec2,
    generation_per_tick: Energy,
}

impl PowerSourceInitialState {
    pub const fn position(self) -> FixedVec2 {
        self.position
    }

    pub const fn generation_per_tick(self) -> Energy {
        self.generation_per_tick
    }
}

fn power_source_semantic_key(source: &PowerSourceInitialState) -> (i64, i64, u64) {
    (
        source.position.x.0,
        source.position.y.0,
        source.generation_per_tick.0,
    )
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StageFeatureSet {
    pub signal: bool,
    pub mobility: bool,
    pub capacity: bool,
    pub sensing: bool,
    pub power: bool,
    pub relay: bool,
    pub payload: bool,
    pub radiation: bool,
}

impl StageFeatureSet {
    pub const fn none() -> Self {
        Self {
            signal: false,
            mobility: false,
            capacity: false,
            sensing: false,
            power: false,
            relay: false,
            payload: false,
            radiation: false,
        }
    }

    pub(crate) const fn first_unsupported(self) -> Option<&'static str> {
        // Signal/mobility are implemented by Stage 0, capacity by S1-M1, and sensing/power by
        // S1-M2. Only later-stage requirements are rejected here.
        if self.relay {
            Some("relay")
        } else if self.payload {
            Some("payload")
        } else if self.radiation {
            Some("radiation")
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileReference {
    path: String,
    profile_id: String,
    profile_hash: ProfileHash,
}

impl ProfileReference {
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
pub struct ProfileReferences {
    numeric: ProfileReference,
    physical_scale: ProfileReference,
    balance: ProfileReference,
}

impl ProfileReferences {
    pub fn numeric(&self) -> &ProfileReference {
        &self.numeric
    }

    pub fn physical_scale(&self) -> &ProfileReference {
        &self.physical_scale
    }

    pub fn balance(&self) -> &ProfileReference {
        &self.balance
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScenarioManifest {
    schema_version: u32,
    scenario_id: String,
    semantics_version: SemanticsVersion,
    hash_algorithm: HashAlgorithmId,
    initial_world: InitialWorld,
    required_features: StageFeatureSet,
    profiles: ProfileReferences,
}

impl ScenarioManifest {
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }

    pub const fn semantics_version(&self) -> SemanticsVersion {
        self.semantics_version
    }

    pub const fn hash_algorithm(&self) -> HashAlgorithmId {
        self.hash_algorithm
    }

    pub fn initial_world(&self) -> &InitialWorld {
        &self.initial_world
    }

    pub const fn required_features(&self) -> StageFeatureSet {
        self.required_features
    }

    pub fn profiles(&self) -> &ProfileReferences {
        &self.profiles
    }

    /// Hashes the portable semantic identity of this Scenario manifest.
    ///
    /// Artifact paths and display profile IDs are deliberately excluded. The logical
    /// `scenarioId`, declared features, initial-world kind, and all three declared semantic
    /// profile hashes are included.
    pub fn canonical_hash(&self) -> Result<ArtifactHash, ScenarioHashError> {
        let mut hasher = blake3::Hasher::new();
        let (domain, encoder_version) = match self.schema_version {
            SCENARIO_SCHEMA_VERSION_V1 => {
                (SCENARIO_HASH_DOMAIN_V1, SCENARIO_HASH_ENCODER_VERSION_V1)
            }
            SCENARIO_SCHEMA_VERSION_V2 => {
                (SCENARIO_HASH_DOMAIN_V2, SCENARIO_HASH_ENCODER_VERSION_V2)
            }
            SCENARIO_SCHEMA_VERSION_V3 => {
                (SCENARIO_HASH_DOMAIN_V3, SCENARIO_HASH_ENCODER_VERSION_V3)
            }
            _ => unreachable!("ScenarioManifest is created only by the strict decoder"),
        };
        hasher.update(domain);
        hasher.update(&encoder_version.to_le_bytes());
        hasher.update(&self.schema_version.to_le_bytes());
        hash_text(&mut hasher, "scenarioId", &self.scenario_id)?;
        hash_text(
            &mut hasher,
            "semanticsVersion",
            self.semantics_version.as_str(),
        )?;
        hash_text(&mut hasher, "hashAlgorithm", self.hash_algorithm.as_str())?;
        match &self.initial_world {
            InitialWorld::Empty => {
                hasher.update(&[0]);
            }
            InitialWorld::MainCoreV1 {
                position,
                integrity,
                heat_energy,
            } => {
                hasher.update(&[1]);
                hasher.update(&position.x.0.to_le_bytes());
                hasher.update(&position.y.0.to_le_bytes());
                hasher.update(&integrity.0.to_le_bytes());
                hasher.update(&heat_energy.0.to_le_bytes());
            }
            InitialWorld::MainCorePowerV1 {
                main_core_position,
                main_core_integrity,
                main_core_heat_energy,
                power_sources,
            } => {
                hasher.update(&[2]);
                hasher.update(&main_core_position.x.0.to_le_bytes());
                hasher.update(&main_core_position.y.0.to_le_bytes());
                hasher.update(&main_core_integrity.0.to_le_bytes());
                hasher.update(&main_core_heat_energy.0.to_le_bytes());

                let mut ordered_sources = power_sources.clone();
                ordered_sources.sort_unstable_by_key(power_source_semantic_key);
                let source_count = u32::try_from(ordered_sources.len())
                    .map_err(|_| ScenarioHashError::PowerSourceCountOverflow)?;
                hasher.update(&source_count.to_le_bytes());
                for source in ordered_sources {
                    hasher.update(&source.position.x.0.to_le_bytes());
                    hasher.update(&source.position.y.0.to_le_bytes());
                    hasher.update(&source.generation_per_tick.0.to_le_bytes());
                }
            }
        };
        for enabled in [
            self.required_features.signal,
            self.required_features.mobility,
            self.required_features.capacity,
            self.required_features.sensing,
            self.required_features.power,
            self.required_features.relay,
            self.required_features.payload,
            self.required_features.radiation,
        ] {
            hasher.update(&[u8::from(enabled)]);
        }
        hasher.update(self.profiles.numeric.profile_hash.as_bytes());
        hasher.update(self.profiles.physical_scale.profile_hash.as_bytes());
        hasher.update(self.profiles.balance.profile_hash.as_bytes());
        Ok(ArtifactHash::from_bytes(*hasher.finalize().as_bytes()))
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ScenarioHashError {
    #[error("Scenario field `{field}` exceeds the canonical u32 byte-length boundary")]
    TextLengthOverflow { field: &'static str },

    #[error("Scenario Power Source count exceeds the canonical u32 boundary")]
    PowerSourceCountOverflow,
}

#[derive(Clone, Copy)]
pub struct ArtifactBytes<'a> {
    pub scenario: &'a [u8],
    pub numeric_profile: &'a [u8],
    pub physical_scale_profile: &'a [u8],
    pub balance_profile: &'a [u8],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScenarioWire {
    schema_version: u32,
    scenario_id: String,
    semantics_version: String,
    hash_algorithm: String,
    initial_world: InitialWorldWire,
    required_features: StageFeatureSet,
    profiles: ProfileReferencesWire,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScenarioSchemaEnvelope {
    schema_version: u32,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum InitialWorldWire {
    Empty,
    MainCoreV1 {
        position: FixedVec2Wire,
        integrity: u64,
        #[serde(rename = "heatEnergy")]
        heat_energy: u64,
    },
    MainCorePowerV1 {
        #[serde(rename = "mainCore")]
        main_core: MainCoreInitialWire,
        #[serde(rename = "powerSources")]
        power_sources: Vec<PowerSourceInitialWire>,
    },
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FixedVec2Wire {
    x: i64,
    y: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MainCoreInitialWire {
    position: FixedVec2Wire,
    integrity: u64,
    heat_energy: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PowerSourceInitialWire {
    position: FixedVec2Wire,
    generation_per_tick: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProfileReferencesWire {
    numeric: ProfileReferenceWire,
    physical_scale: ProfileReferenceWire,
    balance: ProfileReferenceWire,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProfileReferenceWire {
    path: String,
    profile_id: String,
    profile_hash: String,
}

pub fn decode_scenario_manifest(bytes: &[u8]) -> Result<ScenarioManifest, crate::PackageError> {
    // Schema support is a protocol-envelope decision and deterministically precedes decoding
    // version-specific InitialWorld payloads. The second strict decode still rejects every
    // unknown, duplicate, or ill-typed field for a supported schema.
    let envelope: ScenarioSchemaEnvelope = decode_json(bytes, ArtifactKind::Scenario)?;
    if !matches!(
        envelope.schema_version,
        SCENARIO_SCHEMA_VERSION_V1 | SCENARIO_SCHEMA_VERSION_V2 | SCENARIO_SCHEMA_VERSION_V3
    ) {
        return Err(crate::PackageError::UnsupportedSchema {
            artifact: ArtifactKind::Scenario,
            expected: SCENARIO_SCHEMA_VERSION_V3,
            actual: envelope.schema_version,
        });
    }
    let wire: ScenarioWire = decode_json(bytes, ArtifactKind::Scenario)?;
    validate_non_empty(ArtifactKind::Scenario, "scenarioId", &wire.scenario_id)?;

    let semantics_version = SemanticsVersion::parse(&wire.semantics_version).map_err(|_| {
        crate::PackageError::UnsupportedSemanticsVersion {
            expected: SEMANTICS_VERSION_V1,
            actual: wire.semantics_version.clone(),
        }
    })?;
    let hash_algorithm = HashAlgorithmId::parse(&wire.hash_algorithm).map_err(|_| {
        crate::PackageError::UnsupportedHashAlgorithm {
            expected: HASH_ALGORITHM_ID_BLAKE3_V1,
            actual: wire.hash_algorithm.clone(),
        }
    })?;

    let initial_world = match (wire.schema_version, wire.initial_world) {
        (SCENARIO_SCHEMA_VERSION_V1, InitialWorldWire::Empty) => InitialWorld::Empty,
        (
            SCENARIO_SCHEMA_VERSION_V2,
            InitialWorldWire::MainCoreV1 {
                position,
                integrity,
                heat_energy,
            },
        ) => {
            if integrity == 0 {
                return Err(crate::PackageError::NonPositiveInitialWorldField {
                    field: "initialWorld.integrity",
                });
            }
            InitialWorld::MainCoreV1 {
                position: FixedVec2::new(Fixed(position.x), Fixed(position.y)),
                integrity: Integrity(integrity),
                heat_energy: HeatEnergy(heat_energy),
            }
        }
        (
            SCENARIO_SCHEMA_VERSION_V3,
            InitialWorldWire::MainCorePowerV1 {
                main_core,
                power_sources,
            },
        ) => {
            if main_core.integrity == 0 {
                return Err(crate::PackageError::NonPositiveInitialWorldField {
                    field: "initialWorld.mainCore.integrity",
                });
            }
            let mut power_sources = power_sources
                .into_iter()
                .map(|source| PowerSourceInitialState {
                    position: FixedVec2::new(Fixed(source.position.x), Fixed(source.position.y)),
                    generation_per_tick: Energy(source.generation_per_tick),
                })
                .collect::<Vec<_>>();
            power_sources.sort_unstable_by_key(power_source_semantic_key);
            for source in &power_sources {
                if source.generation_per_tick.0 == 0 {
                    return Err(crate::PackageError::NonPositiveInitialWorldField {
                        field: "initialWorld.powerSources[].generationPerTick",
                    });
                }
            }
            if let Some(duplicate) = power_sources
                .windows(2)
                .find(|pair| pair[0].position == pair[1].position)
            {
                return Err(crate::PackageError::DuplicateInitialPowerSourcePosition {
                    position: duplicate[0].position,
                });
            }

            InitialWorld::MainCorePowerV1 {
                main_core_position: FixedVec2::new(
                    Fixed(main_core.position.x),
                    Fixed(main_core.position.y),
                ),
                main_core_integrity: Integrity(main_core.integrity),
                main_core_heat_energy: HeatEnergy(main_core.heat_energy),
                power_sources,
            }
        }
        (schema_version, InitialWorldWire::Empty) => {
            return Err(crate::PackageError::UnsupportedInitialWorld {
                schema_version,
                initial_world: "empty",
            });
        }
        (schema_version, InitialWorldWire::MainCoreV1 { .. }) => {
            return Err(crate::PackageError::UnsupportedInitialWorld {
                schema_version,
                initial_world: "main-core-v1",
            });
        }
        (schema_version, InitialWorldWire::MainCorePowerV1 { .. }) => {
            return Err(crate::PackageError::UnsupportedInitialWorld {
                schema_version,
                initial_world: "main-core-power-v1",
            });
        }
    };

    Ok(ScenarioManifest {
        schema_version: wire.schema_version,
        scenario_id: wire.scenario_id,
        semantics_version,
        hash_algorithm,
        initial_world,
        required_features: wire.required_features,
        profiles: ProfileReferences {
            numeric: decode_profile_reference(wire.profiles.numeric, ProfileKind::Numeric)?,
            physical_scale: decode_profile_reference(
                wire.profiles.physical_scale,
                ProfileKind::PhysicalScale,
            )?,
            balance: decode_profile_reference(wire.profiles.balance, ProfileKind::Balance)?,
        },
    })
}

pub fn decode_package(
    bytes: ArtifactBytes<'_>,
) -> Result<crate::SimulationPackage, crate::PackageError> {
    let scenario = decode_scenario_manifest(bytes.scenario)?;
    let numeric: NumericProfile =
        decode_typed_profile(bytes.numeric_profile, ProfileKind::Numeric)?;
    let physical_scale: PhysicalScaleProfile =
        decode_typed_profile(bytes.physical_scale_profile, ProfileKind::PhysicalScale)?;
    let balance: BalanceProfile =
        decode_typed_profile(bytes.balance_profile, ProfileKind::Balance)?;

    validate_profile_reference(
        scenario.profiles().numeric(),
        &numeric.profile_id,
        ProfileKind::Numeric,
    )?;
    validate_profile_reference(
        scenario.profiles().physical_scale(),
        &physical_scale.profile_id,
        ProfileKind::PhysicalScale,
    )?;
    validate_profile_reference(
        scenario.profiles().balance(),
        &balance.profile_id,
        ProfileKind::Balance,
    )?;

    Ok(crate::SimulationPackage::from_artifacts(
        scenario,
        ProfileBundle {
            numeric,
            physical_scale,
            balance,
        },
    ))
}

/// Strictly decodes and validates one standalone Physical Scale Profile artifact.
pub fn decode_physical_scale_profile(
    bytes: &[u8],
) -> Result<PhysicalScaleProfile, crate::PackageError> {
    decode_typed_profile(bytes, ProfileKind::PhysicalScale)
}

/// Strictly decodes and validates one standalone Numeric Profile artifact.
pub fn decode_numeric_profile(bytes: &[u8]) -> Result<NumericProfile, crate::PackageError> {
    decode_typed_profile(bytes, ProfileKind::Numeric)
}

/// Strictly decodes and validates one standalone Balance Profile artifact.
pub fn decode_balance_profile(bytes: &[u8]) -> Result<BalanceProfile, crate::PackageError> {
    decode_typed_profile(bytes, ProfileKind::Balance)
}

/// Validates and emits the canonical pretty JSON form of a Physical Scale Profile.
///
/// This representation is an artifact transport encoding only. It does not change the
/// independent canonical profile-hash encoder or its V1 domain.
pub fn encode_physical_scale_profile(
    profile: &PhysicalScaleProfile,
) -> Result<Vec<u8>, PhysicalScaleProfileArtifactError> {
    profile.validate()?;
    let mut encoded = serde_json::to_vec_pretty(profile)?;
    encoded.push(b'\n');
    Ok(encoded)
}

#[derive(Debug, Error)]
pub enum PhysicalScaleProfileArtifactError {
    #[error(transparent)]
    Validation(#[from] ProfileValidationError),

    #[error("unable to encode Physical Scale Profile JSON: {0}")]
    Json(#[from] serde_json::Error),
}

fn decode_profile_reference(
    wire: ProfileReferenceWire,
    profile: ProfileKind,
) -> Result<ProfileReference, crate::PackageError> {
    let artifact = ArtifactKind::Scenario;
    validate_non_empty(artifact, profile_path_field(profile), &wire.path)?;
    validate_non_empty(artifact, profile_id_field(profile), &wire.profile_id)?;
    let profile_hash = ProfileHash::from_hex(&wire.profile_hash)
        .map_err(|error| crate::PackageError::InvalidProfileHash { profile, error })?;
    Ok(ProfileReference {
        path: wire.path,
        profile_id: wire.profile_id,
        profile_hash,
    })
}

fn decode_typed_profile<T>(bytes: &[u8], profile: ProfileKind) -> Result<T, crate::PackageError>
where
    T: for<'de> Deserialize<'de> + ValidateProfile,
{
    let value: T = decode_json(bytes, ArtifactKind::Profile(profile))?;
    value
        .validate_profile()
        .map_err(|error| crate::PackageError::InvalidProfile { profile, error })?;
    Ok(value)
}

trait ValidateProfile {
    fn validate_profile(&self) -> Result<(), ProfileValidationError>;
}

impl ValidateProfile for NumericProfile {
    fn validate_profile(&self) -> Result<(), ProfileValidationError> {
        self.validate()
    }
}

impl ValidateProfile for PhysicalScaleProfile {
    fn validate_profile(&self) -> Result<(), ProfileValidationError> {
        self.validate()
    }
}

impl ValidateProfile for BalanceProfile {
    fn validate_profile(&self) -> Result<(), ProfileValidationError> {
        self.validate()
    }
}

fn validate_profile_reference(
    reference: &ProfileReference,
    actual_id: &str,
    profile: ProfileKind,
) -> Result<(), crate::PackageError> {
    if reference.profile_id() == actual_id {
        Ok(())
    } else {
        Err(crate::PackageError::ProfileReferenceMismatch {
            profile,
            expected_id: reference.profile_id().to_owned(),
            actual_id: actual_id.to_owned(),
        })
    }
}

fn decode_json<T>(bytes: &[u8], artifact: ArtifactKind) -> Result<T, crate::PackageError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_slice(bytes).map_err(|error| crate::PackageError::InvalidJson {
        artifact,
        category: JsonErrorCategory::from(error.classify()),
        line: error.line(),
        column: error.column(),
    })
}

fn validate_non_empty(
    artifact: ArtifactKind,
    field: &'static str,
    value: &str,
) -> Result<(), crate::PackageError> {
    if value.trim().is_empty() {
        Err(crate::PackageError::EmptyField { artifact, field })
    } else {
        Ok(())
    }
}

fn hash_text(
    hasher: &mut blake3::Hasher,
    field: &'static str,
    value: &str,
) -> Result<(), ScenarioHashError> {
    let length =
        u32::try_from(value.len()).map_err(|_| ScenarioHashError::TextLengthOverflow { field })?;
    hasher.update(&length.to_le_bytes());
    hasher.update(value.as_bytes());
    Ok(())
}

const fn profile_path_field(profile: ProfileKind) -> &'static str {
    match profile {
        ProfileKind::Numeric => "profiles.numeric.path",
        ProfileKind::PhysicalScale => "profiles.physicalScale.path",
        ProfileKind::Balance => "profiles.balance.path",
    }
}

const fn profile_id_field(profile: ProfileKind) -> &'static str {
    match profile {
        ProfileKind::Numeric => "profiles.numeric.profileId",
        ProfileKind::PhysicalScale => "profiles.physicalScale.profileId",
        ProfileKind::Balance => "profiles.balance.profileId",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenario_v3(initial_world: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 3,
            "scenarioId": "scenario-v3-unit",
            "semanticsVersion": "aon-semantics-v1",
            "hashAlgorithm": "blake3-v1",
            "initialWorld": initial_world,
            "requiredFeatures": {
                "signal": false,
                "mobility": false,
                "capacity": false,
                "sensing": false,
                "power": false,
                "relay": false,
                "payload": false,
                "radiation": false
            },
            "profiles": {
                "numeric": {
                    "path": "n",
                    "profileId": "n",
                    "profileHash": "0000000000000000000000000000000000000000000000000000000000000000"
                },
                "physicalScale": {
                    "path": "p",
                    "profileId": "p",
                    "profileHash": "0000000000000000000000000000000000000000000000000000000000000000"
                },
                "balance": {
                    "path": "b",
                    "profileId": "b",
                    "profileHash": "0000000000000000000000000000000000000000000000000000000000000000"
                }
            }
        }))
        .expect("unit Scenario serializes")
    }

    fn main_core_power(sources: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "kind": "main-core-power-v1",
            "mainCore": {
                "position": { "x": 100, "y": 200 },
                "integrity": 300,
                "heatEnergy": 400
            },
            "powerSources": sources
        })
    }

    fn sources_in_reverse_semantic_order() -> serde_json::Value {
        serde_json::json!([
            {
                "position": { "x": 20, "y": 2 },
                "generationPerTick": 8
            },
            {
                "position": { "x": -10, "y": 1 },
                "generationPerTick": 5
            }
        ])
    }

    #[test]
    fn scenario_v3_sorts_sources_and_hashes_an_independent_canonical_stream() {
        let reverse = sources_in_reverse_semantic_order();
        let mut forward = reverse.as_array().expect("sources are an array").clone();
        forward.reverse();

        let first = decode_scenario_manifest(&scenario_v3(main_core_power(reverse)))
            .expect("v3 Scenario decodes");
        let second = decode_scenario_manifest(&scenario_v3(main_core_power(forward.into())))
            .expect("reordered v3 Scenario decodes");
        assert_eq!(first.canonical_hash(), second.canonical_hash());

        let InitialWorld::MainCorePowerV1 { power_sources, .. } = first.initial_world() else {
            panic!("v3 decodes the frozen initial-world kind");
        };
        assert_eq!(
            power_sources
                .iter()
                .map(|source| source.position().x.0)
                .collect::<Vec<_>>(),
            vec![-10, 20]
        );

        let mut canonical = Vec::new();
        canonical.extend_from_slice(b"AON\0SCENARIO\0V3\0");
        canonical.extend_from_slice(&3_u16.to_le_bytes());
        canonical.extend_from_slice(&3_u32.to_le_bytes());
        for value in ["scenario-v3-unit", "aon-semantics-v1", "blake3-v1"] {
            canonical.extend_from_slice(&(value.len() as u32).to_le_bytes());
            canonical.extend_from_slice(value.as_bytes());
        }
        canonical.push(2);
        for value in [100_i64, 200] {
            canonical.extend_from_slice(&value.to_le_bytes());
        }
        for value in [300_u64, 400] {
            canonical.extend_from_slice(&value.to_le_bytes());
        }
        canonical.extend_from_slice(&2_u32.to_le_bytes());
        for values in [(-10_i64, 1_i64, 5_u64), (20_i64, 2_i64, 8_u64)] {
            canonical.extend_from_slice(&values.0.to_le_bytes());
            canonical.extend_from_slice(&values.1.to_le_bytes());
            canonical.extend_from_slice(&values.2.to_le_bytes());
        }
        canonical.extend_from_slice(&[0; 8]);
        canonical.extend_from_slice(&[0; 32 * 3]);
        assert_eq!(
            first
                .canonical_hash()
                .expect("v3 Scenario hashes")
                .as_bytes(),
            blake3::hash(&canonical).as_bytes()
        );
    }

    #[test]
    fn scenario_v3_allows_source_less_and_rejects_invalid_duplicate_cross_version_payloads() {
        let source_less =
            decode_scenario_manifest(&scenario_v3(main_core_power(serde_json::json!([]))))
                .expect("source-less v3 Scenario decodes for rho zero evidence");
        assert!(matches!(
            source_less.initial_world(),
            InitialWorld::MainCorePowerV1 { power_sources, .. } if power_sources.is_empty()
        ));

        let duplicate = serde_json::json!([
            {
                "position": { "x": 0, "y": 0 },
                "generationPerTick": 1
            },
            {
                "position": { "x": 0, "y": 0 },
                "generationPerTick": 2
            }
        ]);
        assert_eq!(
            decode_scenario_manifest(&scenario_v3(main_core_power(duplicate))),
            Err(crate::PackageError::DuplicateInitialPowerSourcePosition {
                position: FixedVec2::new(Fixed::ZERO, Fixed::ZERO)
            })
        );

        let zero_generation = serde_json::json!([{
            "position": { "x": 0, "y": 0 },
            "generationPerTick": 0
        }]);
        assert_eq!(
            decode_scenario_manifest(&scenario_v3(main_core_power(zero_generation))),
            Err(crate::PackageError::NonPositiveInitialWorldField {
                field: "initialWorld.powerSources[].generationPerTick"
            })
        );

        let out_of_scope_source_state = serde_json::json!([{
            "position": { "x": 0, "y": 0 },
            "generationPerTick": 1,
            "integrity": 1
        }]);
        assert!(matches!(
            decode_scenario_manifest(&scenario_v3(main_core_power(out_of_scope_source_state))),
            Err(crate::PackageError::InvalidJson {
                artifact: ArtifactKind::Scenario,
                category: JsonErrorCategory::Data,
                ..
            })
        ));

        let mut v2_with_v3_world = serde_json::from_slice::<serde_json::Value>(&scenario_v3(
            main_core_power(sources_in_reverse_semantic_order()),
        ))
        .expect("Scenario is JSON");
        v2_with_v3_world["schemaVersion"] = 2.into();
        assert_eq!(
            decode_scenario_manifest(
                &serde_json::to_vec(&v2_with_v3_world).expect("Scenario serializes")
            ),
            Err(crate::PackageError::UnsupportedInitialWorld {
                schema_version: 2,
                initial_world: "main-core-power-v1"
            })
        );
    }
}
