use crate::{
    ArtifactKind, HashParseError, NumericError, ProfileHash, ProfileKind, ProfileValidationError,
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
}

impl From<NumericError> for SimulationError {
    fn from(error: NumericError) -> Self {
        match error {
            NumericError::Overflow => Self::NumericOverflow,
            NumericError::NonPositiveDivisor => Self::InvalidNumericDivisor,
        }
    }
}
