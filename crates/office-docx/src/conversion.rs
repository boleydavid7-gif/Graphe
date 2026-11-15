//! Public semantic-conversion contracts, independent of storage and interfaces.

use std::{error::Error, fmt};

use office_core::{Document, Id, ValidationError};

use crate::{DocxError, DocxSource};

pub(crate) const MAX_DIAGNOSTICS: usize = 128;

/// MIME type of an ordinary non-macro DOCX document.
pub const DOCX_MEDIA_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document";

/// A recognized compatibility finding; normalization notices are non-blocking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatibilityIssueKind {
    /// A document element has no supported semantic mapping.
    UnsupportedElement,
    /// A property or attribute cannot be represented faithfully.
    UnsupportedAttribute,
    /// A supported property uses a value outside the declared profile.
    UnsupportedValue,
    /// A relationship introduces unsupported document semantics.
    UnsupportedRelationship,
    /// An additional package part requires unsupported interpretation.
    UnsupportedPart,
    /// Approved formatting normalization permits editing with an explicit notice.
    NormalizedFormatting,
}

impl CompatibilityIssueKind {
    /// Returns the stable category used by interface transports.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::UnsupportedElement => "unsupported_element",
            Self::UnsupportedAttribute => "unsupported_attribute",
            Self::UnsupportedValue => "unsupported_value",
            Self::UnsupportedRelationship => "unsupported_relationship",
            Self::UnsupportedPart => "unsupported_part",
            Self::NormalizedFormatting => "normalized_formatting",
        }
    }
}

/// A bounded, structured explanation of preserved read-only content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibilityDiagnostic {
    /// Stable category suitable for interface translation.
    pub kind: CompatibilityIssueKind,
    /// Package part containing or identifying the unsupported construct.
    pub part: String,
    /// Human-readable element/property path within that part.
    pub path: String,
    /// Explanation; it must not be used as a machine-readable identifier.
    pub message: String,
}

/// Separate blocking compatibility findings and approved normalization notices.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompatibilityReport {
    /// At most 128 concrete findings in deterministic traversal order.
    pub diagnostics: Vec<CompatibilityDiagnostic>,
    /// Number of further findings omitted from this bounded report.
    pub omitted_diagnostics: usize,
    /// At most 128 formatting approximations applied during a supported import.
    /// These notices never make unsupported content editable.
    pub normalizations: Vec<CompatibilityDiagnostic>,
    /// Number of further normalization notices omitted from this bounded report.
    pub omitted_normalizations: usize,
}

impl CompatibilityReport {
    /// Whether every inspected semantic construct fits the editable profile.
    #[must_use]
    pub fn is_supported(&self) -> bool {
        self.diagnostics.is_empty() && self.omitted_diagnostics == 0
    }

    pub(crate) fn push(
        &mut self,
        kind: CompatibilityIssueKind,
        part: &str,
        path: impl Into<String>,
        message: impl Into<String>,
    ) {
        if self.diagnostics.len() < MAX_DIAGNOSTICS {
            self.diagnostics.push(CompatibilityDiagnostic {
                kind,
                part: bounded_label(part, 512),
                path: bounded_label(&path.into(), 512),
                message: bounded_label(&message.into(), 2048),
            });
        } else {
            self.omitted_diagnostics += 1;
        }
    }

    /// Records an approved approximation without consuming the blocker budget.
    pub(crate) fn normalize(
        &mut self,
        part: &str,
        path: impl Into<String>,
        message: impl Into<String>,
    ) {
        if self.normalizations.len() < MAX_DIAGNOSTICS {
            self.normalizations.push(CompatibilityDiagnostic {
                kind: CompatibilityIssueKind::NormalizedFormatting,
                part: bounded_label(part, 512),
                path: bounded_label(&path.into(), 512),
                message: bounded_label(&message.into(), 2048),
            });
        } else {
            self.omitted_normalizations = self.omitted_normalizations.saturating_add(1);
        }
    }
}

/// Bounds diagnostic text without splitting UTF-8; ellipsis marks truncation.
pub(crate) fn bounded_label(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.into();
    }
    let mut end = limit.saturating_sub(3);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

/// A complete validated editable projection and its immutable original file.
#[derive(Debug)]
pub struct ImportedDocx {
    pub(crate) document: Document,
    pub(crate) original: DocxSource,
    pub(crate) report: CompatibilityReport,
}

impl ImportedDocx {
    /// Returns the supported shared semantic document.
    #[must_use]
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Returns the exact original independently from current editable content.
    #[must_use]
    pub fn original(&self) -> &DocxSource {
        &self.original
    }

    /// Returns the bounded normalization notices for this editable projection.
    /// Blocking diagnostics are empty for every successfully imported document.
    #[must_use]
    pub fn report(&self) -> &CompatibilityReport {
        &self.report
    }

    /// Transfers both parts; callers must retain the original before editing.
    #[must_use]
    pub fn into_parts(self) -> (Document, DocxSource) {
        (self.document, self.original)
    }
}

/// An import is either complete and editable, or an intact read-only source.
#[derive(Debug)]
pub enum DocxImportOutcome {
    /// All semantic content fits the declared profile.
    Editable(ImportedDocx),
    /// Unsupported content is retained without publishing a partial document.
    Preserved {
        /// Exact immutable original bytes and package inventory.
        original: DocxSource,
        /// Reasons editing is unavailable for this source.
        report: CompatibilityReport,
    },
}

/// Semantic conversion failure; valid unsupported sources use an import outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConversionError {
    /// The bounded package/XML inspection rejected the input.
    Package(DocxError),
    /// Recognized WordprocessingML structure or property data is invalid.
    InvalidDocument {
        /// Relevant package part.
        part: String,
        /// Human-readable explanation.
        reason: String,
    },
    /// The resulting or supplied shared model violates its invariants.
    Model(ValidationError),
    /// The native model contains information that this exporter cannot encode.
    UnsupportedExport {
        /// Explicit representation limitation; nothing was emitted partially.
        feature: String,
    },
}

impl fmt::Display for ConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Package(error) => error.fmt(f),
            Self::InvalidDocument { part, reason } => {
                write!(f, "invalid DOCX document in {part}: {reason}")
            }
            Self::Model(error) => error.fmt(f),
            Self::UnsupportedExport { feature } => {
                write!(f, "unsupported DOCX export: {feature}")
            }
        }
    }
}

impl Error for ConversionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Package(error) => Some(error),
            Self::Model(error) => Some(error),
            _ => None,
        }
    }
}

impl From<DocxError> for ConversionError {
    fn from(error: DocxError) -> Self {
        Self::Package(error)
    }
}

impl From<ValidationError> for ConversionError {
    fn from(error: ValidationError) -> Self {
        Self::Model(error)
    }
}

pub(crate) fn invalid(part: &str, reason: impl Into<String>) -> ConversionError {
    ConversionError::InvalidDocument {
        part: part.into(),
        reason: reason.into(),
    }
}

/// Imports a DOCX under an explicit stable native document identifier.
///
/// Unsupported semantics produce a preserved read-only outcome. No partial
/// editable document is returned, and no storage or live session is modified.
pub fn import_docx(bytes: &[u8], document_id: Id) -> Result<DocxImportOutcome, ConversionError> {
    crate::import::import_document(bytes, document_id)
}

/// Validates and regenerates a supported DOCX from current shared-model content.
///
/// This is distinct from recovering an imported source's exact original bytes.
/// Native stable IDs and unrepresentable empty-container distinctions are not
/// universal DOCX guarantees. Errors return before publishing partial output.
pub fn export_docx(document: &Document) -> Result<Vec<u8>, ConversionError> {
    crate::export::export_document(document)
}

#[cfg(test)]
mod tests {
    use super::{CompatibilityIssueKind as Issue, CompatibilityReport, MAX_DIAGNOSTICS};

    #[test]
    fn normalization_flood_never_consumes_or_hides_blocking_diagnostics() {
        let mut already_blocked = CompatibilityReport::default();
        already_blocked.push(
            Issue::UnsupportedElement,
            "word/document.xml",
            "ins",
            "Revision content is unsupported.",
        );
        for index in 0..MAX_DIAGNOSTICS + 9 {
            already_blocked.normalize(
                "word/styles.xml",
                index.to_string(),
                "Approved approximation.",
            );
        }
        assert!(!already_blocked.is_supported());
        assert_eq!(already_blocked.diagnostics.len(), 1);
        assert_eq!(already_blocked.diagnostics[0].path, "ins");
        let mut report = CompatibilityReport::default();
        for index in 0..MAX_DIAGNOSTICS + 9 {
            report.normalize(
                "word/styles.xml",
                format!("rPr/{index}"),
                "Approved approximation.",
            );
        }
        assert!(report.is_supported());
        assert_eq!(report.normalizations.len(), MAX_DIAGNOSTICS);
        assert_eq!(report.omitted_normalizations, 9);
        assert!(report.diagnostics.is_empty());
        report.push(
            Issue::UnsupportedElement,
            "word/document.xml",
            "tbl",
            "Tables remain unsupported.",
        );
        assert!(!report.is_supported());
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].kind, Issue::UnsupportedElement);
        for index in 0..MAX_DIAGNOSTICS + 2 {
            report.push(
                Issue::UnsupportedRelationship,
                "word/_rels/document.xml.rels",
                index.to_string(),
                "Unsafe relationship.",
            );
        }
        assert_eq!(report.diagnostics.len(), MAX_DIAGNOSTICS);
        assert_eq!(report.omitted_diagnostics, 3);
        assert_eq!(report.normalizations.len(), MAX_DIAGNOSTICS);
        assert_eq!(report.omitted_normalizations, 9);
    }

    #[test]
    fn normalization_labels_remain_bounded_on_utf8_boundaries() {
        let mut report = CompatibilityReport::default();
        let text = "é".repeat(3000);
        report.normalize(&text, &text, &text);
        let warning = &report.normalizations[0];
        assert_eq!(warning.kind, Issue::NormalizedFormatting);
        assert_eq!(warning.kind.code(), "normalized_formatting");
        assert!(warning.part.len() <= 512 && warning.part.ends_with('…'));
        assert!(warning.path.len() <= 512 && warning.path.ends_with('…'));
        assert!(warning.message.len() <= 2048 && warning.message.ends_with('…'));
        assert!(report.is_supported());
    }
}
