use crate::contract::{
    HASH_ALGORITHM_ID_BLAKE3_V1, HashAlgorithmId, SEMANTICS_VERSION_V1, SemanticsVersion,
};
use crate::profile::{
    BalanceProfile, NumericProfile, PhysicalScaleProfile, ProfileBundle, ProfileValidationError,
};
use crate::{ArtifactHash, JsonErrorCategory, ProfileHash};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

const SCENARIO_SCHEMA_VERSION_V1: u32 = 1;
const SCENARIO_HASH_DOMAIN: &[u8] = b"AON\0SCENARIO\0V1\0";
const SCENARIO_HASH_ENCODER_VERSION: u16 = 1;

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

    pub(crate) const fn first_enabled(self) -> Option<&'static str> {
        // Signal and mobility are implemented by Stage 0; only later-stage requirements are
        // rejected here.
        if self.capacity {
            Some("capacity")
        } else if self.sensing {
            Some("sensing")
        } else if self.power {
            Some("power")
        } else if self.relay {
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

    pub(crate) fn initial_world(&self) -> &InitialWorld {
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
        hasher.update(SCENARIO_HASH_DOMAIN);
        hasher.update(&SCENARIO_HASH_ENCODER_VERSION.to_le_bytes());
        hasher.update(&self.schema_version.to_le_bytes());
        hash_text(&mut hasher, "scenarioId", &self.scenario_id)?;
        hash_text(
            &mut hasher,
            "semanticsVersion",
            self.semantics_version.as_str(),
        )?;
        hash_text(&mut hasher, "hashAlgorithm", self.hash_algorithm.as_str())?;
        hasher.update(&[match &self.initial_world {
            InitialWorld::Empty => 0,
        }]);
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
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum InitialWorldWire {
    Empty,
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
    let wire: ScenarioWire = decode_json(bytes, ArtifactKind::Scenario)?;
    validate_schema(
        ArtifactKind::Scenario,
        SCENARIO_SCHEMA_VERSION_V1,
        wire.schema_version,
    )?;
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

    Ok(ScenarioManifest {
        schema_version: wire.schema_version,
        scenario_id: wire.scenario_id,
        semantics_version,
        hash_algorithm,
        initial_world: match wire.initial_world {
            InitialWorldWire::Empty => InitialWorld::Empty,
        },
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

fn validate_schema(
    artifact: ArtifactKind,
    expected: u32,
    actual: u32,
) -> Result<(), crate::PackageError> {
    if actual == expected {
        Ok(())
    } else {
        Err(crate::PackageError::UnsupportedSchema {
            artifact,
            expected,
            actual,
        })
    }
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
