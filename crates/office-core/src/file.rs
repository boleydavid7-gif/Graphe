//! Format-neutral native-file state, separate from editable document semantics.

use std::{fmt, sync::Arc};

use crate::{Document, DocumentError, PersistenceError};

/// Maximum exact original-source payload retained in one Writer file (16 MiB).
pub const MAX_ORIGINAL_SOURCE_BYTES: usize = 16 * 1024 * 1024;
/// Maximum length of a source format identifier in ASCII bytes.
pub const MAX_SOURCE_FORMAT_BYTES: usize = 64;
/// Maximum length of an unparameterized source media type in ASCII bytes.
pub const MAX_SOURCE_MEDIA_TYPE_BYTES: usize = 255;
/// Maximum length of a source filename in UTF-8 bytes.
pub const MAX_SOURCE_FILENAME_BYTES: usize = 1024;

/// Immutable original file bytes and bounded, format-neutral identification.
///
/// This is a retained original, not a claim of import support, provenance, or
/// faithful export after edits. A format adapter separately validates the
/// bytes and determines whether editing is supported. Cloning shares immutable
/// bytes through [`Arc`]; source state is outside document undo snapshots.
#[derive(Clone, PartialEq, Eq)]
pub struct OriginalSource {
    format: String,
    media_type: String,
    filename: String,
    bytes: Arc<[u8]>,
}

impl OriginalSource {
    /// Retains an original after checking metadata and payload resource limits.
    ///
    /// Format identifiers use lowercase ASCII letters/digits, periods,
    /// underscores and hyphens. Media types use RFC 6838 restricted names,
    /// without parameters. Filenames are nonblank leaf names without controls
    /// or separators; their Unicode spelling is preserved exactly.
    /// Input conversion must preserve its reported length; the retained bytes
    /// are checked again after conversion.
    pub fn new(
        format: impl Into<String>,
        media_type: impl Into<String>,
        filename: impl Into<String>,
        bytes: impl AsRef<[u8]> + Into<Arc<[u8]>>,
    ) -> Result<Self, PersistenceError> {
        let format = format.into();
        let media_type = media_type.into();
        let filename = filename.into();
        validate_metadata(&format, &media_type, &filename)?;
        let length = bytes.as_ref().len();
        if length > MAX_ORIGINAL_SOURCE_BYTES {
            return Err(PersistenceError::LimitExceeded {
                resource: "original_source_bytes",
                limit: MAX_ORIGINAL_SOURCE_BYTES,
                actual: length,
            });
        }
        if length == 0 {
            return Err(invalid("bytes", "an original source must not be empty"));
        }
        let bytes: Arc<[u8]> = bytes.into();
        let actual = bytes.len();
        if actual > MAX_ORIGINAL_SOURCE_BYTES {
            return Err(PersistenceError::LimitExceeded {
                resource: "original_source_bytes",
                limit: MAX_ORIGINAL_SOURCE_BYTES,
                actual,
            });
        }
        if actual == 0 {
            return Err(invalid("bytes", "an original source must not be empty"));
        }
        if actual != length {
            return Err(invalid(
                "bytes",
                "source conversion must preserve the reported byte length",
            ));
        }
        Ok(Self {
            format,
            media_type,
            filename,
            bytes,
        })
    }

    /// Returns the opaque, canonical source format identifier.
    #[must_use]
    pub fn format(&self) -> &str {
        &self.format
    }

    /// Returns the original media type without inferring semantics from it.
    #[must_use]
    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    /// Returns the retained original filename; no path is stored or opened.
    #[must_use]
    pub fn filename(&self) -> &str {
        &self.filename
    }

    /// Returns exact original bytes, never reconstruction from the document.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Debug for OriginalSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OriginalSource")
            .field("format", &self.format)
            .field("media_type", &self.media_type)
            .field("filename", &self.filename)
            .field("byte_count", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

/// A validated document and optional immutable original in the native file layer.
///
/// Sources remain outside [`Document`] and editing history. Native files with
/// originals can retain content removed from the current document; exporting or
/// sharing such a file must disclose original-source retention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterFile {
    document: Document,
    original_source: Option<OriginalSource>,
}

impl WriterFile {
    /// Wraps an already validated document without adding source state.
    #[must_use]
    pub fn new(document: Document) -> Self {
        Self {
            document,
            original_source: None,
        }
    }

    /// Associates a validated document with an immutable retained original.
    ///
    /// This checks model invariants, not semantic correspondence with the
    /// original. Only the relevant format adapter can establish that association.
    pub fn with_original_source(
        document: Document,
        original_source: OriginalSource,
    ) -> Result<Self, PersistenceError> {
        document.validate().map_err(DocumentError::Validation)?;
        Ok(Self {
            document,
            original_source: Some(original_source),
        })
    }

    /// Returns the validated editable model.
    #[must_use]
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Returns the retained source independently of the editable model.
    #[must_use]
    pub fn original_source(&self) -> Option<&OriginalSource> {
        self.original_source.as_ref()
    }

    /// Separates model and source state without discarding either.
    #[must_use]
    pub fn into_parts(self) -> (Document, Option<OriginalSource>) {
        (self.document, self.original_source)
    }
}

pub(crate) fn validate_metadata(
    format: &str,
    media_type: &str,
    filename: &str,
) -> Result<(), PersistenceError> {
    for (field, value, limit) in [
        ("source_format_bytes", format, MAX_SOURCE_FORMAT_BYTES),
        (
            "source_media_type_bytes",
            media_type,
            MAX_SOURCE_MEDIA_TYPE_BYTES,
        ),
        ("source_filename_bytes", filename, MAX_SOURCE_FILENAME_BYTES),
    ] {
        if value.len() > limit {
            return Err(PersistenceError::LimitExceeded {
                resource: field,
                limit,
                actual: value.len(),
            });
        }
    }
    if format.is_empty()
        || !format.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
        || !format.as_bytes()[0].is_ascii_alphanumeric()
    {
        return Err(invalid(
            "format",
            "expected a lowercase ASCII format identifier",
        ));
    }
    let restricted_name = |name: &str| {
        !name.is_empty()
            && name.len() <= 127
            && name.as_bytes()[0].is_ascii_alphanumeric()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$&^_.+-".contains(&byte))
    };
    if !media_type
        .split_once('/')
        .is_some_and(|(kind, subtype)| restricted_name(kind) && restricted_name(subtype))
    {
        return Err(invalid(
            "media_type",
            "expected an unparameterized ASCII media type",
        ));
    }
    if filename.trim().is_empty()
        || matches!(filename, "." | "..")
        || filename
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
    {
        return Err(invalid(
            "filename",
            "expected a nonblank filename without controls or separators",
        ));
    }
    Ok(())
}

fn invalid(field: &'static str, reason: &'static str) -> PersistenceError {
    PersistenceError::InvalidSourceMetadata { field, reason }
}
