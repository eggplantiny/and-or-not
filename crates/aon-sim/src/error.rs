use crate::{
    ArtifactKind, Fixed, FixedVec2, HashParseError, HeatEnergy, Integrity, NumericError,
    ProfileHash, ProfileKind, ProfileValidationError,
};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonErrorCategory {
    Io,
    Syntax,
    Data,
    Eof,
}

impl From<serde_json::error::Category> for JsonErrorCategory {
    fn from(category: serde_json::error::Category) -> Self {
        match category {
            serde_json::error::Category::Io => Self::Io,
            serde_json::error::Category::Syntax => Self::Syntax,
            serde_json::error::Category::Data => Self::Data,
            serde_json::error::Category::Eof => Self::Eof,
        }
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PackageError {
    #[error("invalid JSON for {artifact}: category={category:?}, line={line}, column={column}")]
    InvalidJson {
        artifact: ArtifactKind,
        category: JsonErrorCategory,
        line: usize,
        column: usize,
    },

    #[error("unsupported schema for {artifact}: expected {expected}, got {actual}")]
    UnsupportedSchema {
        artifact: ArtifactKind,
        expected: u32,
        actual: u32,
    },

    #[error("unsupported semantics version: expected {expected}, got {actual}")]
    UnsupportedSemanticsVersion {
        expected: &'static str,
        actual: String,
    },

    #[error("unsupported hash algorithm: expected {expected}, got {actual}")]
    UnsupportedHashAlgorithm {
        expected: &'static str,
        actual: String,
    },

    #[error("{artifact} field `{field}` must not be empty")]
    EmptyField {
        artifact: ArtifactKind,
        field: &'static str,
    },

    #[error(
        "Scenario schema {schema_version} does not support initial world kind `{initial_world}`"
    )]
    UnsupportedInitialWorld {
        schema_version: u32,
        initial_world: &'static str,
    },

    #[error("Scenario initial-world field `{field}` must be positive")]
    NonPositiveInitialWorldField { field: &'static str },

    #[error("Scenario initial world contains duplicate Power Source position {position:?}")]
    DuplicateInitialPowerSourcePosition { position: FixedVec2 },

    #[error("Scenario initial world must contain at least one Enemy")]
    EmptyInitialEnemySet,

    #[error(
        "Scenario initial Enemy trajectory overflows: position={position:?}, velocityPerTick={velocity_per_tick:?}"
    )]
    InitialEnemyTrajectoryOverflow {
        position: FixedVec2,
        velocity_per_tick: FixedVec2,
    },

    #[error(
        "Scenario initial world contains duplicate Enemy: position={position:?}, velocityPerTick={velocity_per_tick:?}, radius={radius:?}, integrity={integrity:?}, heatEnergy={heat_energy:?}"
    )]
    DuplicateInitialEnemy {
        position: FixedVec2,
        velocity_per_tick: FixedVec2,
        radius: Fixed,
        integrity: Integrity,
        heat_energy: HeatEnergy,
    },

    #[error("Scenario v4 requires stage feature `{feature}`")]
    MissingRequiredScenarioFeature { feature: &'static str },

    #[error("Scenario initial-world field `{field}` is not aligned to wireGeometryQuantum")]
    InitialWorldFieldNotQuantumAligned { field: &'static str },

    #[error("Scenario v4 requires Balance schema 5, got {actual}")]
    ScenarioV4RequiresBalanceV5 { actual: u32 },

    #[error(
        "Scenario initial integrity for `{entity_kind}` does not match Balance v5: expected {expected}, got {actual}"
    )]
    InitialIntegrityProfileMismatch {
        entity_kind: &'static str,
        expected: Integrity,
        actual: Integrity,
    },

    #[error("profile kind mismatch: expected {expected}, got {actual}")]
    ProfileKindMismatch {
        expected: ProfileKind,
        actual: ProfileKind,
    },

    #[error("{profile} profile reference mismatch: expected {expected_id}, got {actual_id}")]
    ProfileReferenceMismatch {
        profile: ProfileKind,
        expected_id: String,
        actual_id: String,
    },

    #[error("invalid declared {profile} profile hash: {error}")]
    InvalidProfileHash {
        profile: ProfileKind,
        error: HashParseError,
    },

    #[error("invalid {profile} profile: {error}")]
    InvalidProfile {
        profile: ProfileKind,
        error: ProfileValidationError,
    },
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SimulationError {
    #[error("the simulation Run has already ended")]
    RunEnded,

    #[error(
        "initial integrity for `{entity_kind}` does not match Balance v5: expected {expected:?}, got {actual:?}"
    )]
    InitialIntegrityProfileMismatch {
        entity_kind: &'static str,
        expected: Integrity,
        actual: Integrity,
    },

    #[error("canonical numeric overflow")]
    NumericOverflow,

    #[error("canonical numeric divisor must be positive")]
    InvalidNumericDivisor,

    #[error("canonical structural invariant violated")]
    InvalidCanonicalState,

    #[error("canonical event queue invariant violated")]
    EventQueueInvariantViolation,

    #[error("canonical Driver Revision invariant violated")]
    DriverRevisionInvariantViolation,

    #[error("canonical Path Certificate invariant violated")]
    PathCertificateInvariantViolation,

    #[error("invalid simulation profile: {error}")]
    InvalidProfile { error: ProfileValidationError },

    #[error("{profile} profile hash mismatch: expected {expected}, got {actual}")]
    ProfileHashMismatch {
        profile: ProfileKind,
        expected: ProfileHash,
        actual: ProfileHash,
    },

    #[error("unsupported semantics version `{actual}`")]
    UnsupportedSemanticsVersion { actual: String },

    #[error("unsupported hash algorithm `{actual}`")]
    UnsupportedHashAlgorithm { actual: String },

    #[error("stage feature `{feature}` is not implemented by this engine build")]
    UnsupportedStageFeature { feature: &'static str },

    #[error("stage feature `capacity` requires the `main-core-v1` initial world")]
    CapacityRequiresMainCore,

    #[error("the `main-core-v1` initial world requires stage feature `capacity`")]
    MainCoreRequiresCapacity,

    #[error("stage feature `capacity` requires Balance section `capacityProbe`")]
    CapacityRequiresProfile,

    #[error("Main Core position is not aligned to wireGeometryQuantum")]
    InvalidMainCoreGeometryQuantum,

    #[error("Main Core integrity must be positive at world generation")]
    InvalidMainCoreIntegrity,

    #[error(
        "the `main-core-power-v1` initial world requires capacity, sensing, and power features"
    )]
    MainCorePowerRequiresFeatures,

    #[error("capacity, sensing, and power features require the `main-core-power-v1` initial world")]
    PowerFeaturesRequireMainCorePowerWorld,

    #[error("Balance section `powerProbe` requires the `main-core-power-v1` initial world")]
    PowerProbeRequiresMainCorePowerWorld,

    #[error(
        "the `main-core-power-v1` initial world requires Balance sections `capacityProbe` and `powerProbe`"
    )]
    MainCorePowerRequiresProfiles,

    #[error("Power Source position is not aligned to wireGeometryQuantum")]
    InvalidPowerSourceGeometryQuantum,

    #[error("WorldInput events must target the Tick currently being executed")]
    WorldInputTickMismatch,

    #[error("at most one HostileFrame is allowed for one Tick")]
    DuplicateWorldInputFrame,

    #[error("hostile collider ID 0 is reserved")]
    InvalidHostileId,

    #[error("a HostileFrame contains duplicate hostile collider ID {id}")]
    DuplicateHostileId { id: u64 },

    #[error("hostile collider {id} has a negative radius")]
    NegativeHostileRadius { id: u64 },
}

impl From<NumericError> for SimulationError {
    fn from(error: NumericError) -> Self {
        match error {
            NumericError::Overflow => Self::NumericOverflow,
            NumericError::NonPositiveDivisor => Self::InvalidNumericDivisor,
        }
    }
}
