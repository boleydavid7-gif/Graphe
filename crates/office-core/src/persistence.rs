//! Filesystem-independent, versioned native Writer serialization.
//!
//! The full-file API retains original-source state outside the semantic model.
//! Legacy formats 1 and 2 migrate explicitly; format 3 carries semantic lists.

use std::{borrow::Cow, error::Error, fmt, io, str::Utf8Error};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::{
    Document, DocumentError, OriginalSource, SCHEMA_VERSION, WriterFile,
    file::{MAX_ORIGINAL_SOURCE_BYTES, validate_metadata},
};

/// Identifier of the provisional native Writer envelope.
pub const WRITER_FORMAT_NAME: &str = "office-writer";
/// Current native envelope version, independent of engine and model versions.
pub const WRITER_FORMAT_VERSION: u32 = 3;
/// Maximum serialized native file accepted or emitted, including retained source.
pub const MAX_WRITER_FILE_BYTES: usize = 32 * 1024 * 1024;
/// Provisional filename extension; serialization never changes supplied paths.
pub const WRITER_EXTENSION: &str = "writer";

/// A native Writer encoding, format, source-record, or document-model failure.
#[derive(Debug)]
pub enum PersistenceError {
    /// Input is not UTF-8.
    InvalidUtf8(Utf8Error),
    /// JSON syntax or required shape is invalid.
    Json(serde_json::Error),
    /// The envelope identifies another native format.
    UnknownFormat {
        /// Format identifier found in input.
        found: String,
    },
    /// The native envelope version is unsupported.
    UnsupportedFormatVersion {
        /// Version found in input.
        found: u64,
        /// Current native envelope version written by this engine.
        supported: u32,
    },
    /// A known model schema is not permitted within this envelope version.
    IncompatibleModelSchema {
        /// Native envelope version found in input.
        format_version: u32,
        /// Model schema found in input.
        found: u64,
        /// Model schema required by this envelope profile.
        expected: u32,
    },
    /// The enclosed document fails existing strict model loading or validation.
    Document(DocumentError),
    /// A document-only loader would lose an original; use the full-file API.
    SourcePresent,
    /// A retained source's identifying metadata is invalid.
    InvalidSourceMetadata {
        /// Invalid field within the original-source record.
        field: &'static str,
        /// Human-readable reason; no values are silently repaired.
        reason: &'static str,
    },
    /// The original source is not strict standard padded base64.
    InvalidSourceEncoding {
        /// Human-readable encoding diagnostic.
        reason: String,
    },
    /// File, metadata, or original-source bytes exceed the documented profile.
    LimitExceeded {
        /// Stable resource category.
        resource: &'static str,
        /// Maximum accepted value.
        limit: usize,
        /// Requested or observed value.
        actual: usize,
    },
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUtf8(error) => {
                write!(formatter, "native Writer input is not UTF-8: {error}")
            }
            Self::Json(error) => write!(formatter, "invalid native Writer JSON: {error}"),
            Self::UnknownFormat { found } => {
                write!(formatter, "unrecognized native document format: {found}")
            }
            Self::UnsupportedFormatVersion { found, supported } => write!(
                formatter,
                "unsupported Writer format {found}; current format: {supported}"
            ),
            Self::IncompatibleModelSchema {
                format_version,
                found,
                expected,
            } => write!(
                formatter,
                "Writer format {format_version} requires model schema {expected}, found {found}"
            ),
            Self::Document(error) => write!(formatter, "invalid native Writer document: {error}"),
            Self::SourcePresent => formatter.write_str(
                "this Writer file retains an original source; use the full-file API to preserve it",
            ),
            Self::InvalidSourceMetadata { field, reason } => {
                write!(formatter, "invalid original-source {field}: {reason}")
            }
            Self::InvalidSourceEncoding { reason } => {
                write!(formatter, "invalid original-source base64: {reason}")
            }
            Self::LimitExceeded {
                resource,
                limit,
                actual,
            } => write!(
                formatter,
                "Writer {resource} limit {limit} exceeded by {actual}"
            ),
        }
    }
}

impl Error for PersistenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidUtf8(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Document(error) => Some(error),
            _ => None,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<OutputSource<'a>>,
}

#[derive(Serialize)]
struct OutputSource<'a> {
    format: &'a str,
    media_type: &'a str,
    filename: &'a str,
    bytes_base64: String,
}

#[derive(Deserialize)]
struct Header {
    format: String,
    format_version: u64,
}

#[derive(Deserialize)]
struct ModelHeader {
    schema_version: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyEnvelope<'a> {
    format: String,
    format_version: u64,
    #[serde(borrow)]
    document: &'a RawValue,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputEnvelope<'a> {
    format: String,
    format_version: u64,
    #[serde(borrow)]
    document: &'a RawValue,
    #[serde(default, borrow)]
    source: Option<InputSource<'a>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputSource<'a> {
    #[serde(borrow)]
    format: Cow<'a, str>,
    #[serde(borrow)]
    media_type: Cow<'a, str>,
    #[serde(borrow)]
    filename: Cow<'a, str>,
    #[serde(borrow)]
    bytes_base64: Cow<'a, str>,
}

fn check_format(format: &str, version: u64) -> Result<(), PersistenceError> {
    if format != WRITER_FORMAT_NAME {
        return Err(PersistenceError::UnknownFormat {
            found: format.into(),
        });
    }
    if !matches!(version, 1 | 2 | 3) {
        return Err(PersistenceError::UnsupportedFormatVersion {
            found: version,
            supported: WRITER_FORMAT_VERSION,
        });
    }
    Ok(())
}

fn load_model(raw: &RawValue, format_version: u32) -> Result<Document, PersistenceError> {
    let header: ModelHeader = serde_json::from_str(raw.get()).map_err(DocumentError::Json)?;
    let expected = format_version;
    if header.schema_version != u64::from(expected) {
        if !matches!(header.schema_version, 1 | 2 | 3) {
            return Err(DocumentError::UnsupportedSchemaVersion {
                found: header.schema_version,
                supported: SCHEMA_VERSION,
            }
            .into());
        }
        return Err(PersistenceError::IncompatibleModelSchema {
            format_version,
            found: header.schema_version,
            expected,
        });
    }
    // Preserve raw object text so duplicate keys survive until strict decoding.
    Ok(Document::from_json(raw.get())?)
}

/// Validates and serializes a complete Writer file, including its exact original.
///
/// Current output is UTF-8 JSON format 3, fixed field order, two-space indentation
/// and one trailing LF. The retained source uses strict standard padded base64.
/// Output is capped during serialization, not after an unbounded allocation.
/// No history, session revisions, engine version or timestamps are added.
///
/// ```
/// use office_core::{Document, Id, OriginalSource, WriterFile,
///     deserialize_writer_file, serialize_writer_file};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let document = Document::new(Id::new("document-1")?)?;
/// let source = OriginalSource::new("example", "application/octet-stream",
///     "original.bin", b"exact original bytes".as_slice())?;
/// let file = WriterFile::with_original_source(document, source)?;
/// let restored = deserialize_writer_file(&serialize_writer_file(&file)?)?;
/// assert_eq!(restored, file);
/// # Ok(())
/// # }
/// ```
pub fn serialize_writer_file(file: &WriterFile) -> Result<Vec<u8>, PersistenceError> {
    serialize_parts(file.document(), file.original_source())
}

/// Validates and serializes a source-free document in the current native format.
///
/// This API deliberately accepts only a document. Call [`serialize_writer_file`]
/// to retain file-level original-source state.
///
/// ```
/// use office_core::{Document, Id, deserialize_document, serialize_document};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let document = Document::new(Id::new("document-1")?)?;
/// let bytes = serialize_document(&document)?;
/// assert_eq!(deserialize_document(&bytes)?, document);
/// assert_eq!(serialize_document(&document)?, bytes);
/// # Ok(())
/// # }
/// ```
pub fn serialize_document(document: &Document) -> Result<Vec<u8>, PersistenceError> {
    serialize_parts(document, None)
}

fn serialize_parts(
    document: &Document,
    source: Option<&OriginalSource>,
) -> Result<Vec<u8>, PersistenceError> {
    document.validate().map_err(DocumentError::Validation)?;
    let source = source.map(|source| OutputSource {
        format: source.format(),
        media_type: source.media_type(),
        filename: source.filename(),
        bytes_base64: STANDARD.encode(source.bytes()),
    });
    let envelope = OutputEnvelope {
        format: WRITER_FORMAT_NAME,
        format_version: WRITER_FORMAT_VERSION,
        document,
        source,
    };
    let mut writer = BoundedWriter::default();
    if let Err(error) = serde_json::to_writer_pretty(&mut writer, &envelope) {
        return Err(writer
            .exceeded
            .map_or(PersistenceError::Json(error), file_limit));
    }
    io::Write::write_all(&mut writer, b"\n")
        .map_err(|_| file_limit(writer.exceeded.expect("bounded writer reports its limit")))?;
    Ok(writer.bytes)
}

/// Decodes a complete Writer file without dropping retained original-source data.
///
/// Legacy format 1 accepts schema 1; format 2 accepts schema 2 via migration.
/// Current format 3 accepts schema 3. Envelope/model
/// versions remain separate checks. Future versions, unknown/duplicate fields,
/// malformed base64 and exceeded budgets are errors; nothing is repaired.
/// Generic source identification does not validate a particular external format.
pub fn deserialize_writer_file(bytes: &[u8]) -> Result<WriterFile, PersistenceError> {
    if bytes.len() > MAX_WRITER_FILE_BYTES {
        return Err(file_limit(bytes.len()));
    }
    let json = std::str::from_utf8(bytes)?;
    let header: Header = serde_json::from_str(json)?;
    check_format(&header.format, header.format_version)?;
    if header.format_version == 1 {
        let envelope: LegacyEnvelope<'_> = serde_json::from_str(json)?;
        check_format(&envelope.format, envelope.format_version)?;
        return Ok(WriterFile::new(load_model(envelope.document, 1)?));
    }
    let envelope: InputEnvelope<'_> = serde_json::from_str(json)?;
    check_format(&envelope.format, envelope.format_version)?;
    let document = load_model(envelope.document, envelope.format_version as u32)?;
    match envelope.source {
        Some(source) => WriterFile::with_original_source(document, decode_source(source)?),
        None => Ok(WriterFile::new(document)),
    }
}

/// Loads a source-free native document, or explicitly refuses original-data loss.
///
/// Legacy version-1 documents migrate through the model loader. A source-bearing
/// source-bearing file returns [`PersistenceError::SourcePresent`]; callers must use
/// [`deserialize_writer_file`] and retain both returned parts instead.
pub fn deserialize_document(bytes: &[u8]) -> Result<Document, PersistenceError> {
    let file = deserialize_writer_file(bytes)?;
    if file.original_source().is_some() {
        return Err(PersistenceError::SourcePresent);
    }
    Ok(file.into_parts().0)
}

fn decode_source(source: InputSource<'_>) -> Result<OriginalSource, PersistenceError> {
    validate_metadata(&source.format, &source.media_type, &source.filename)?;
    let encoded = source.bytes_base64.as_bytes();
    let max_encoded = MAX_ORIGINAL_SOURCE_BYTES.div_ceil(3) * 4;
    if encoded.len() > max_encoded {
        return Err(PersistenceError::LimitExceeded {
            resource: "source_base64_bytes",
            limit: max_encoded,
            actual: encoded.len(),
        });
    }
    if !encoded.len().is_multiple_of(4) {
        return Err(PersistenceError::InvalidSourceEncoding {
            reason: "canonical padded base64 length must be a multiple of four".into(),
        });
    }
    let padding = encoded
        .iter()
        .rev()
        .take_while(|&&byte| byte == b'=')
        .count();
    if padding > 2 {
        return Err(PersistenceError::InvalidSourceEncoding {
            reason: "base64 contains excessive trailing padding".into(),
        });
    }
    let decoded_length = encoded.len() / 4 * 3 - padding;
    if decoded_length > MAX_ORIGINAL_SOURCE_BYTES {
        return Err(PersistenceError::LimitExceeded {
            resource: "original_source_bytes",
            limit: MAX_ORIGINAL_SOURCE_BYTES,
            actual: decoded_length,
        });
    }
    let bytes =
        STANDARD
            .decode(encoded)
            .map_err(|error| PersistenceError::InvalidSourceEncoding {
                reason: error.to_string(),
            })?;
    OriginalSource::new(
        source.format.into_owned(),
        source.media_type.into_owned(),
        source.filename.into_owned(),
        bytes,
    )
}

fn file_limit(actual: usize) -> PersistenceError {
    PersistenceError::LimitExceeded {
        resource: "writer_file_bytes",
        limit: MAX_WRITER_FILE_BYTES,
        actual,
    }
}

#[derive(Default)]
struct BoundedWriter {
    bytes: Vec<u8>,
    exceeded: Option<usize>,
}

impl io::Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let actual = self.bytes.len().saturating_add(bytes.len());
        if actual > MAX_WRITER_FILE_BYTES {
            self.exceeded = Some(actual);
            return Err(io::Error::other(
                "native Writer output exceeds its byte budget",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
