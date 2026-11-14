//! Filesystem-independent serialization for the provisional native Writer format.
//!
//! A versioned envelope contains the existing document JSON without changing its
//! schema or validation rules. Storage adapters can transport the resulting UTF-8
//! bytes without depending on a native filesystem or a browser API.

use std::error::Error;
use std::fmt;
use std::str::Utf8Error;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::{Document, DocumentError};

/// Identifier of the provisional native Writer envelope.
pub const WRITER_FORMAT_NAME: &str = "office-writer";

/// The only supported native Writer format version, independent of engine and model versions.
pub const WRITER_FORMAT_VERSION: u32 = 1;

/// Provisional native Writer filename extension, without the leading dot.
///
/// The format is internal and may evolve before public release. Serialization
/// neither examines filenames nor appends this extension automatically.
pub const WRITER_EXTENSION: &str = "writer";

/// A native Writer encoding, format, or document-model failure.
#[derive(Debug)]
pub enum PersistenceError {
    /// The supplied bytes are not valid UTF-8.
    InvalidUtf8(Utf8Error),
    /// The envelope has invalid JSON syntax or does not match its required shape.
    Json(serde_json::Error),
    /// The envelope does not identify the native Writer format.
    UnknownFormat {
        /// The decoded format identifier found in the input.
        found: String,
    },
    /// The envelope advertises a native format version this engine does not support.
    UnsupportedFormatVersion {
        /// Version found in the input.
        found: u64,
        /// Native format version supported by this engine.
        supported: u32,
    },
    /// The enclosed document has an unsupported schema, invalid shape, or invalid values.
    Document(DocumentError),
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUtf8(error) => write!(f, "native Writer input is not UTF-8: {error}"),
            Self::Json(error) => write!(f, "invalid native Writer JSON: {error}"),
            Self::UnknownFormat { found } => {
                write!(f, "unrecognized native document format: {found}")
            }
            Self::UnsupportedFormatVersion { found, supported } => {
                write!(
                    f,
                    "unsupported Writer format {found}; supported: {supported}"
                )
            }
            Self::Document(error) => write!(f, "invalid native Writer document: {error}"),
        }
    }
}

impl Error for PersistenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidUtf8(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Document(error) => Some(error),
            Self::UnknownFormat { .. } | Self::UnsupportedFormatVersion { .. } => None,
        }
    }
}

impl From<serde_json::Error> for PersistenceError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<DocumentError> for PersistenceError {
    fn from(error: DocumentError) -> Self {
        Self::Document(error)
    }
}

impl From<Utf8Error> for PersistenceError {
    fn from(error: Utf8Error) -> Self {
        Self::InvalidUtf8(error)
    }
}

#[derive(Serialize)]
struct OutputEnvelope<'a> {
    format: &'static str,
    format_version: u32,
    document: &'a Document,
}

// The header intentionally ignores payload shape until its version is checked.
// Owned strings accept escaped JSON marker spellings as well as literal ones.
#[derive(Deserialize)]
struct Header {
    format: String,
    format_version: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputEnvelope<'a> {
    format: String,
    format_version: u64,
    #[serde(borrow)]
    document: &'a RawValue,
}

fn check_format(format: &str, version: u64) -> Result<(), PersistenceError> {
    if format != WRITER_FORMAT_NAME {
        return Err(PersistenceError::UnknownFormat {
            found: format.into(),
        });
    }
    if version != u64::from(WRITER_FORMAT_VERSION) {
        return Err(PersistenceError::UnsupportedFormatVersion {
            found: version,
            supported: WRITER_FORMAT_VERSION,
        });
    }
    Ok(())
}

/// Validates a document and encodes it as complete native Writer UTF-8 JSON bytes.
///
/// Output uses two-space pretty indentation, fixed envelope field order, and a
/// single trailing LF. Supported document values and array order are preserved.
/// Repeated serialization of the same model is deterministic; this is not a
/// cryptographic JSON canonicalization scheme. No engine version, timestamp,
/// session revision, editing position, or undo history is added.
///
/// ```
/// use office_core::{Document, Id, deserialize_document, serialize_document};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let document = Document::new(Id::new("document-1")?)?;
/// let bytes = serialize_document(&document)?;
/// assert_eq!(deserialize_document(&bytes)?, document);
/// assert_eq!(serialize_document(&document)?, bytes);
/// # Ok(())
/// # }
/// ```
pub fn serialize_document(document: &Document) -> Result<Vec<u8>, PersistenceError> {
    document.validate().map_err(DocumentError::Validation)?;
    let envelope = OutputEnvelope {
        format: WRITER_FORMAT_NAME,
        format_version: WRITER_FORMAT_VERSION,
        document,
    };
    let mut bytes = serde_json::to_vec_pretty(&envelope)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Decodes a complete native Writer file without accessing storage.
///
/// The format identifier and version are checked before decoding document
/// fields. Unknown fields, duplicate object keys, malformed structures, and
/// unsupported model schemas are rejected through the existing model loader.
/// UTF-8 text, stable IDs, formatting, and ordering are never normalized or
/// repaired. A BOM is rejected as invalid JSON; ordinary JSON whitespace is
/// accepted. No migration or fallback to bare document JSON is implicit.
pub fn deserialize_document(bytes: &[u8]) -> Result<Document, PersistenceError> {
    let json = std::str::from_utf8(bytes)?;
    let header: Header = serde_json::from_str(json)?;
    check_format(&header.format, header.format_version)?;

    let envelope: InputEnvelope<'_> = serde_json::from_str(json)?;
    check_format(&envelope.format, envelope.format_version)?;
    // Keep the original object text: a Value intermediate would erase duplicate
    // nested keys before the document loader could reject them.
    Ok(Document::from_json(envelope.document.get())?)
}
