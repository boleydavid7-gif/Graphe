use std::{error::Error, fmt};

/// Structured package-inspection failure; input is never repaired or modified.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DocxError {
    /// A compressed, expanded, structural, or XML resource budget was exceeded.
    LimitExceeded {
        /// Stable resource category, such as `package_entries` or `xml_depth`.
        resource: &'static str,
        /// Maximum accepted value.
        limit: u64,
        /// Requested or observed value.
        actual: u64,
    },
    /// ZIP bytes, headers, integrity checks, or part names are invalid.
    InvalidArchive {
        /// Human-readable diagnostic.
        reason: String,
    },
    /// A recognized feature lies outside the explicit initial profile.
    Unsupported {
        /// Feature or encoding requiring a future compatibility decision.
        feature: String,
    },
    /// XML syntax or the declared XML version/encoding is invalid.
    InvalidXml {
        /// Package item containing the XML.
        part: String,
        /// Human-readable diagnostic.
        reason: String,
    },
    /// OPC metadata, main-document identity, or relationships are invalid.
    InvalidPackage {
        /// Relevant control item or package part, when known.
        part: Option<String>,
        /// Human-readable diagnostic.
        reason: String,
    },
}

impl fmt::Display for DocxError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitExceeded {
                resource,
                limit,
                actual,
            } => write!(
                formatter,
                "DOCX {resource} limit {limit} exceeded by {actual}"
            ),
            Self::InvalidArchive { reason } => write!(formatter, "invalid DOCX archive: {reason}"),
            Self::Unsupported { feature } => {
                write!(formatter, "unsupported DOCX profile feature: {feature}")
            }
            Self::InvalidXml { part, reason } => {
                write!(formatter, "invalid XML in {part}: {reason}")
            }
            Self::InvalidPackage { part, reason } => match part {
                Some(part) => write!(formatter, "invalid DOCX package at {part}: {reason}"),
                None => write!(formatter, "invalid DOCX package: {reason}"),
            },
        }
    }
}

impl Error for DocxError {}

pub(crate) fn limit(resource: &'static str, limit: usize, actual: usize) -> DocxError {
    DocxError::LimitExceeded {
        resource,
        limit: limit as u64,
        actual: actual as u64,
    }
}
