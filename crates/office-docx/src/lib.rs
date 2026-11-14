//! Bounded, filesystem-independent DOCX package inspection.
//!
//! Step 5A validates a documented OPC/ZIP and XML profile and locates the main
//! WordprocessingML part through relationships. It does not import into the
//! document model, export DOCX, render content, execute embedded data, or fetch
//! external relationships. Successful inspection is not a claim of semantic
//! support for every document feature. Keep the original bytes for later use.

mod error;
mod opc;
mod package;
mod xml;

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
/// operation; DOCX import/export is a later gated implementation.
pub fn inspect_docx(bytes: &[u8]) -> Result<DocxInspection, DocxError> {
    let package = package::read_package(bytes)?;
    opc::inspect_package(&package)
}
