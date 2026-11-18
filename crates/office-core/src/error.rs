use std::error::Error;
use std::fmt;

/// A document invariant violation. Validation returns the first error in model order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// An identifier is empty or has leading/trailing whitespace.
    InvalidId {
        /// Location of the identifier.
        path: String,
    },
    /// Two model entities share an identifier, including style definitions.
    DuplicateId {
        /// Identifier used by both entities.
        id: String,
    },
    /// A paragraph or style refers to a style that does not exist.
    MissingStyle {
        /// Location of the reference.
        path: String,
        /// Unresolved style identifier.
        id: String,
    },
    /// Style inheritance contains a cycle.
    StyleCycle {
        /// Identifier of a style in the cycle.
        id: String,
    },
    /// A named style's display name is blank or duplicates another name.
    InvalidStyleName {
        /// Location of the name.
        path: String,
    },
    /// A formatting property has an invalid value.
    InvalidFormatting {
        /// Location of the property.
        path: String,
        /// Explanation of the allowed value.
        reason: String,
    },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId { path } => write!(f, "invalid identifier at {path}"),
            Self::DuplicateId { id } => write!(f, "duplicate identifier: {id}"),
            Self::MissingStyle { path, id } => write!(f, "unresolved style {id} at {path}"),
            Self::StyleCycle { id } => write!(f, "style inheritance cycle at {id}"),
            Self::InvalidStyleName { path } => write!(f, "blank or duplicate style name at {path}"),
            Self::InvalidFormatting { path, reason } => write!(f, "invalid {path}: {reason}"),
        }
    }
}

impl Error for ValidationError {}

/// A serialization, schema, or document validation failure.
#[derive(Debug)]
pub enum DocumentError {
    /// The input advertises a schema version this engine does not support.
    UnsupportedSchemaVersion {
        /// Version found in the input.
        found: u64,
        /// Version supported by this engine.
        supported: u32,
    },
    /// JSON syntax or shape does not match the schema.
    Json(serde_json::Error),
    /// Raster asset validation or resource limits failed before document decoding.
    Image(crate::ImageError),
    /// The decoded or constructed model violates a documented invariant.
    Validation(ValidationError),
}

impl fmt::Display for DocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion { found, supported } => {
                write!(
                    f,
                    "unsupported document schema {found}; supported: {supported}"
                )
            }
            Self::Json(error) => write!(f, "invalid document JSON: {error}"),
            Self::Image(error) => error.fmt(f),
            Self::Validation(error) => error.fmt(f),
        }
    }
}

impl Error for DocumentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Image(error) => Some(error),
            Self::Validation(error) => Some(error),
            Self::UnsupportedSchemaVersion { .. } => None,
        }
    }
}

impl From<serde_json::Error> for DocumentError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<ValidationError> for DocumentError {
    fn from(error: ValidationError) -> Self {
        Self::Validation(error)
    }
}

impl From<crate::ImageError> for DocumentError {
    fn from(error: crate::ImageError) -> Self {
        Self::Image(error)
    }
}
