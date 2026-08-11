use crate::{ArtifactKind, ProfileKind};
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
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SimulationError {
    #[error("canonical tick overflow")]
    TickOverflow,

    #[error("bootstrap simulation accepts only an empty command batch")]
    CommandsUnsupported,
}
