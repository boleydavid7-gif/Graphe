//! Interface-independent foundations for the Office Engine.
//!
//! This crate is intended to support the engine's shared document semantics and
//! invariants as those responsibilities become necessary. Editing, layout,
//! rendering, and file formats will have explicit boundaries; their eventual
//! crate organization will follow demonstrated needs.
//!
//! The core must remain independent of React, JavaScript, and browser APIs so
//! that future native applications and WebAssembly integrations can share it.
//! The model stores exact UTF-8 text. [`TextCoordinates`] distinguishes bytes,
//! UTF-16 code units, and extended graphemes. [`DocumentPosition`] and [`Selection`]
//! identify logical cursor boundaries using stable document element IDs.
//! [`DocumentSession`] publishes validated transaction drafts and issues
//! session-scoped [`Revision`] tokens without changing the document schema.
//! Transactions stage text and formatting operations using [`EditPosition`] and
//! [`EditSelection`]; sessions retain bounded exact snapshots for undo and redo.
//! See the repository architecture document for the versioned JSON contract,
//! validation rules, and Unicode coordinate conventions.
//!
//! # Example
//!
//! ```
//! use office_core::{Block, Document, DocumentMetadata, Id, Paragraph, Section, TextRun};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let paragraph = Paragraph::new(Id::new("p-1")?, vec![TextRun::new("Hello, world!")]);
//! let section = Section::new(Id::new("s-1")?, vec![Block::Paragraph(paragraph)]);
//! let document = Document::from_parts(
//!     Id::new("doc-1")?,
//!     DocumentMetadata { title: "Example".into() },
//!     vec![section],
//!     vec![],
//! )?;
//! let json = document.to_json()?;
//! assert_eq!(Document::from_json(&json)?, document);
//! # Ok(())
//! # }
//! ```

mod coordinates;
mod document;
mod edit_ops;
mod editing;
mod error;
mod file;
#[cfg(not(target_arch = "wasm32"))]
mod filesystem;
mod format_patch;
mod formatting;
mod persistence;
mod position;
mod transaction;

pub use coordinates::{
    GRAPHEME_UNICODE_VERSION, GraphemeOffset, TextCoordinates, Utf8Offset, Utf16Offset,
};
pub use document::{
    Block, Document, DocumentMetadata, Id, ListKind, NamedStyle, Paragraph, SCHEMA_VERSION, Section,
    SectionProperties, TextRun,
};
pub use editing::{EditError, EditPosition, EditSelection};
pub use error::{DocumentError, ValidationError};
pub use file::{
    MAX_ORIGINAL_SOURCE_BYTES, MAX_SOURCE_FILENAME_BYTES, MAX_SOURCE_FORMAT_BYTES,
    MAX_SOURCE_MEDIA_TYPE_BYTES, OriginalSource, WriterFile,
};
#[cfg(not(target_arch = "wasm32"))]
pub use filesystem::{
    ReplacementState, StorageError, StorageOperation, TemporaryCleanupError, load_document,
    load_writer_file, save_document, save_writer_file,
};
pub use format_patch::{CharacterFormatPatch, ParagraphFormatPatch, ParagraphMetricsPatch, PropertyChange};
pub use formatting::{
    Alignment, CharacterFormat, HalfPoints, Indentation, LineSpacing, PageMargins, PageOrientation,
    PageSize, ParagraphFormat, TextColor, Twips, Underline, valid_hyperlink,
};
pub use persistence::{
    MAX_WRITER_FILE_BYTES, PersistenceError, WRITER_EXTENSION, WRITER_FORMAT_NAME,
    WRITER_FORMAT_VERSION, deserialize_document, deserialize_writer_file, serialize_document,
    serialize_writer_file,
};
pub use position::{DocumentPosition, OffsetUnit, PositionError, Selection};
pub use transaction::{
    CommitOutcome, DEFAULT_HISTORY_LIMIT, DocumentDraft, DocumentSession, HistoryError, Revision,
    Transaction, TransactionError,
};

/// Returns the version of the engine core, as declared in its Cargo manifest.
#[must_use]
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_available() {
        let version = super::version();

        assert!(!version.is_empty());
        assert_eq!(version, env!("CARGO_PKG_VERSION"));
    }
}
