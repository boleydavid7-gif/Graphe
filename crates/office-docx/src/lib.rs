//! Bounded, filesystem-independent DOCX inspection and semantic conversion.
//!
//! Package inspection validates the OPC/ZIP and XML profile. [`import_docx`]
//! returns a complete editable shared model plus its exact original, or a
//! preserved read-only outcome for unsupported semantics. [`export_docx`]
//! regenerates the supported profile from current model values. Source recovery
//! and regeneration are distinct operations. No rendering, storage, execution
//! of embedded content or external-relationship fetching occurs here.
//!
//! The image profile supports bounded embedded PNG/JPEG pictures in dedicated
//! ordinary body paragraphs, with physical extents, alternative text and
//! left/center/right alignment. DrawingML crop, floating anchors, effects,
//! linked images, mixed text/pictures and table-cell pictures remain preserved
//! read-only. Their relationships are never accepted solely by filename.
//! Export uses original raster bytes except when EXIF orientation or color
//! approximation requires the canonical Rust preview PNG. Native files retain
//! the original asset. Source-aware copies include only current body media;
//! deleted images and original thumbnails are never grafted back into a copy.
//!
//! Keep original bytes outside the document model/history using the native
//! full-file API before editing. Inspection alone does not imply editability,
//! complete OOXML conformance, or Microsoft Word pagination equivalence.

mod ancillary;
mod conversion;
mod copy;
mod error;
mod export;
mod import;
mod numbering;
mod opc;
mod package;
mod properties;
mod xml;

pub use conversion::{
    CompatibilityDiagnostic, CompatibilityIssueKind, CompatibilityReport, ConversionError,
    DOCX_MEDIA_TYPE, DocxImportOutcome, ImportedDocx, export_docx, import_docx,
};
pub use copy::{DocxCopyAssessment, assess_docx_copy, export_docx_with_source};
pub use error::DocxError;

/// Maximum compressed input accepted by this prototype, in bytes (16 MiB).
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
/// Maximum ZIP entries, including explicit directory entries.
pub const MAX_PACKAGE_ENTRIES: usize = 256;
/// Maximum expanded size of one package part, in bytes (8 MiB).
pub const MAX_PART_BYTES: usize = 8 * 1024 * 1024;
/// Maximum aggregate expanded bytes, including control and relationship items.
pub const MAX_EXPANDED_BYTES: usize = 32 * 1024 * 1024;

pub(crate) const MAX_XML_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const MAX_XML_DEPTH: usize = 64;
pub(crate) const MAX_XML_NODES: usize = 32_768;
pub(crate) const MAX_XML_ATTRIBUTES: usize = 64;
pub(crate) const MAX_XML_TOTAL_ATTRIBUTES: usize = 65_536;
pub(crate) const MAX_XML_NAMESPACE_DECLARATIONS: usize = 128;

/// Namespace family used by the main WordprocessingML document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocxConformance {
    /// ECMA/ISO Transitional WordprocessingML namespace family.
    Transitional,
    /// ISO Strict WordprocessingML namespace family.
    Strict,
}

/// Read-only metadata for a package part; no decompression/library types escape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocxPart {
    /// Relative canonical part name, without a leading slash.
    pub name: String,
    /// MIME content type resolved from the package's content-types item.
    pub content_type: String,
    /// Observed expanded size after a complete checked read.
    pub uncompressed_bytes: u64,
}

/// Validated container inventory, not an editable or semantically imported document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocxInspection {
    /// Main document part discovered through the package relationship.
    pub main_document_part: String,
    /// Main XML namespace family, checked against the relationship type.
    pub conformance: DocxConformance,
    /// Package parts in deterministic name order, excluding `[Content_Types].xml`.
    pub parts: Vec<DocxPart>,
    /// Number of package and part relationships checked.
    pub relationship_count: usize,
    /// External relationships inventoried but never fetched.
    pub external_relationship_count: usize,
    /// Observed expanded size, including content types and relationship parts.
    pub total_uncompressed_bytes: u64,
}

/// An immutable original DOCX package and its validated read-only inventory.
///
/// Unsupported document semantics are retained byte-for-byte in the source.
/// This does not make them editable or guarantee they can survive future edits;
/// no editable projection or native-format conversion exists in Step 5A.
pub struct DocxSource {
    original: Vec<u8>,
    inspection: DocxInspection,
}

impl DocxSource {
    /// Validates the package profile before retaining an owned copy of its bytes.
    pub fn open(bytes: &[u8]) -> Result<Self, DocxError> {
        let inspection = inspect_docx(bytes)?;
        Ok(Self {
            original: bytes.to_vec(),
            inspection,
        })
    }

    /// Returns the exact original file, without reconstruction or normalization.
    #[must_use]
    pub fn original_bytes(&self) -> &[u8] {
        &self.original
    }

    /// Returns the package inventory; it is not an editable document model.
    #[must_use]
    pub fn inspection(&self) -> &DocxInspection {
        &self.inspection
    }

    /// Consumes the source and returns its exact original file bytes.
    #[must_use]
    pub fn into_original_bytes(self) -> Vec<u8> {
        self.original
    }
}

impl std::fmt::Debug for DocxSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DocxSource")
            .field("original_byte_count", &self.original.len())
            .field("inspection", &self.inspection)
            .finish_non_exhaustive()
    }
}

/// Inspects a bounded DOCX package without extracting files or modifying input.
///
/// The initial profile accepts stored/deflated classic single-disk ZIP packages,
/// canonical ASCII part names, and bounded UTF-8 XML 1.0. Unsupported container
/// or encoding features are explicit errors. No network requests are performed.
/// Main parts may use Strict or Transitional namespace families. Unsupported
/// document semantics are not converted or silently discarded by this read-only
/// operation; use [`import_docx`] for a checked semantic conversion outcome.
pub fn inspect_docx(bytes: &[u8]) -> Result<DocxInspection, DocxError> {
    let package = package::read_package(bytes)?;
    opc::inspect_package(&package)
}
