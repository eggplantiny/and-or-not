use crate::canonical;
pub use crate::error::PackageError;
use crate::{JsonErrorCategory, ProfileHash};
use serde::Deserialize;
use std::fmt;

const BOOTSTRAP_SCHEMA_VERSION: u32 = 0;
const BOOTSTRAP_SEMANTICS_VERSION: &str = "bootstrap-v0";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
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
        let value = match self {
            Self::Numeric => "numeric",
            Self::PhysicalScale => "physical-scale",
            Self::Balance => "balance",
        };
        formatter.write_str(value)
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

impl InitialWorld {
    pub(crate) const fn canonical_tag(&self) -> u8 {
        match self {
            Self::Empty => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileReference {
    path: String,
    profile_id: String,
}

impl ProfileReference {
    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn profile_id(&self) -> &str {
        &self.profile_id
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
    semantics_version: String,
    initial_world: InitialWorld,
    profiles: ProfileReferences,
}

impl ScenarioManifest {
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }

    pub fn semantics_version(&self) -> &str {
        &self.semantics_version
    }

    pub(crate) fn initial_world(&self) -> &InitialWorld {
        &self.initial_world
    }

    pub fn profiles(&self) -> &ProfileReferences {
        &self.profiles
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileArtifact {
    schema_version: u32,
    profile_id: String,
    kind: ProfileKind,
}

impl ProfileArtifact {
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub const fn kind(&self) -> ProfileKind {
        self.kind
    }

    pub fn canonical_hash(&self) -> ProfileHash {
        canonical::profile_hash(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileBundle {
    numeric: ProfileArtifact,
    physical_scale: ProfileArtifact,
    balance: ProfileArtifact,
}

impl ProfileBundle {
    pub fn numeric(&self) -> &ProfileArtifact {
        &self.numeric
    }

    pub fn physical_scale(&self) -> &ProfileArtifact {
        &self.physical_scale
    }

    pub fn balance(&self) -> &ProfileArtifact {
        &self.balance
    }
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
    initial_world: InitialWorldWire,
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
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProfileWire {
    schema_version: u32,
    profile_id: String,
    kind: ProfileKind,
}

pub fn decode_scenario_manifest(bytes: &[u8]) -> Result<ScenarioManifest, PackageError> {
    let wire: ScenarioWire = decode_json(bytes, ArtifactKind::Scenario)?;
    validate_schema(ArtifactKind::Scenario, wire.schema_version)?;
    validate_non_empty(ArtifactKind::Scenario, "scenarioId", &wire.scenario_id)?;
    validate_non_empty(
        ArtifactKind::Scenario,
        "semanticsVersion",
        &wire.semantics_version,
    )?;
    if wire.semantics_version != BOOTSTRAP_SEMANTICS_VERSION {
        return Err(PackageError::UnsupportedSemanticsVersion {
            expected: BOOTSTRAP_SEMANTICS_VERSION,
            actual: wire.semantics_version,
        });
    }
    validate_non_empty(
        ArtifactKind::Scenario,
        "profiles.numeric.path",
        &wire.profiles.numeric.path,
    )?;
    validate_non_empty(
        ArtifactKind::Scenario,
        "profiles.numeric.profileId",
        &wire.profiles.numeric.profile_id,
    )?;
    validate_non_empty(
        ArtifactKind::Scenario,
        "profiles.physicalScale.path",
        &wire.profiles.physical_scale.path,
    )?;
    validate_non_empty(
        ArtifactKind::Scenario,
        "profiles.physicalScale.profileId",
        &wire.profiles.physical_scale.profile_id,
    )?;
    validate_non_empty(
        ArtifactKind::Scenario,
        "profiles.balance.path",
        &wire.profiles.balance.path,
    )?;
    validate_non_empty(
        ArtifactKind::Scenario,
        "profiles.balance.profileId",
        &wire.profiles.balance.profile_id,
    )?;

    Ok(ScenarioManifest {
        schema_version: wire.schema_version,
        scenario_id: wire.scenario_id,
        semantics_version: wire.semantics_version,
        initial_world: match wire.initial_world {
            InitialWorldWire::Empty => InitialWorld::Empty,
        },
        profiles: ProfileReferences {
            numeric: ProfileReference {
                path: wire.profiles.numeric.path,
                profile_id: wire.profiles.numeric.profile_id,
            },
            physical_scale: ProfileReference {
                path: wire.profiles.physical_scale.path,
                profile_id: wire.profiles.physical_scale.profile_id,
            },
            balance: ProfileReference {
                path: wire.profiles.balance.path,
                profile_id: wire.profiles.balance.profile_id,
            },
        },
    })
}

pub fn decode_package(bytes: ArtifactBytes<'_>) -> Result<crate::SimulationPackage, PackageError> {
    let scenario = decode_scenario_manifest(bytes.scenario)?;
    let numeric = decode_profile(bytes.numeric_profile, ProfileKind::Numeric)?;
    let physical_scale = decode_profile(bytes.physical_scale_profile, ProfileKind::PhysicalScale)?;
    let balance = decode_profile(bytes.balance_profile, ProfileKind::Balance)?;
    validate_profile_reference(scenario.profiles().numeric(), &numeric)?;
    validate_profile_reference(scenario.profiles().physical_scale(), &physical_scale)?;
    validate_profile_reference(scenario.profiles().balance(), &balance)?;

    Ok(crate::SimulationPackage::from_bootstrap_artifacts(
        scenario,
        ProfileBundle {
            numeric,
            physical_scale,
            balance,
        },
    ))
}

fn validate_profile_reference(
    reference: &ProfileReference,
    profile: &ProfileArtifact,
) -> Result<(), PackageError> {
    if reference.profile_id() == profile.profile_id() {
        Ok(())
    } else {
        Err(PackageError::ProfileReferenceMismatch {
            profile: profile.kind(),
            expected_id: reference.profile_id().to_owned(),
            actual_id: profile.profile_id().to_owned(),
        })
    }
}

fn decode_profile(bytes: &[u8], expected: ProfileKind) -> Result<ProfileArtifact, PackageError> {
    let artifact = ArtifactKind::Profile(expected);
    let wire: ProfileWire = decode_json(bytes, artifact)?;
    validate_schema(artifact, wire.schema_version)?;
    validate_non_empty(artifact, "profileId", &wire.profile_id)?;
    if wire.kind != expected {
        return Err(PackageError::ProfileKindMismatch {
            expected,
            actual: wire.kind,
        });
    }

    Ok(ProfileArtifact {
        schema_version: wire.schema_version,
        profile_id: wire.profile_id,
        kind: wire.kind,
    })
}

fn decode_json<T>(bytes: &[u8], artifact: ArtifactKind) -> Result<T, PackageError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_slice(bytes).map_err(|error| PackageError::InvalidJson {
        artifact,
        category: JsonErrorCategory::from(error.classify()),
        line: error.line(),
        column: error.column(),
    })
}

fn validate_schema(artifact: ArtifactKind, actual: u32) -> Result<(), PackageError> {
    if actual == BOOTSTRAP_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(PackageError::UnsupportedSchema {
            artifact,
            expected: BOOTSTRAP_SCHEMA_VERSION,
            actual,
        })
    }
}

fn validate_non_empty(
    artifact: ArtifactKind,
    field: &'static str,
    value: &str,
) -> Result<(), PackageError> {
    if value.trim().is_empty() {
        Err(PackageError::EmptyField { artifact, field })
    } else {
        Ok(())
    }
}
