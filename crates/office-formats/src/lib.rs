//! Filesystem-independent file-format adapters around the shared Rust model.
//!
//! Supported projections are editable only when every source construct has a
//! supported interpretation. Unsupported rich sources remain exact originals,
//! without publishing a partial editable document. External formats are not a
//! substitute for native Writer files: stable IDs, exact native property
//! distinctions and retained originals are not portable conversion guarantees.

use std::{error::Error, fmt};

use office_core::{Document, Id, OriginalSource, WriterFile};

mod odt;
mod pdf;
mod rtf;
mod text;

/// Maximum source file size accepted by these adapters (16 MiB).
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
/// Maximum complete exported file size (32 MiB).
pub const MAX_OUTPUT_BYTES: usize = 32 * 1024 * 1024;

/// Concrete formats offered by Writer, independent of its semantic model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentFormat {
    /// Full native Writer state, including a retained original when present.
    Writer,
    /// Supported Microsoft Word OOXML profile.
    Docx,
    /// Supported OpenDocument text profile.
    Odt,
    /// Supported Rich Text Format profile.
    Rtf,
    /// Strict Unicode plain text.
    Txt,
    /// Fixed-page PDF export; never an editable import.
    Pdf,
}

impl DocumentFormat {
    /// Parses a canonical lowercase extension without a leading period.
    pub fn parse(value: &str) -> Result<Self, FormatError> {
        match value {
            "writer" => Ok(Self::Writer),
            "docx" => Ok(Self::Docx),
            "odt" => Ok(Self::Odt),
            "rtf" => Ok(Self::Rtf),
            "txt" => Ok(Self::Txt),
            "pdf" => Ok(Self::Pdf),
            _ => Err(FormatError::new(
                "unknown_format",
                "Unsupported document format.",
            )),
        }
    }

    /// Returns the canonical filename extension and source-format identifier.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Writer => "writer",
            Self::Docx => "docx",
            Self::Odt => "odt",
            Self::Rtf => "rtf",
            Self::Txt => "txt",
            Self::Pdf => "pdf",
        }
    }

    /// Returns the exported media type, without encoding parameters.
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Writer => "application/json",
            Self::Docx => office_docx::DOCX_MEDIA_TYPE,
            Self::Odt => "application/vnd.oasis.opendocument.text",
            Self::Rtf => "application/rtf",
            Self::Txt => "text/plain",
            Self::Pdf => "application/pdf",
        }
    }
}

/// A bounded structured compatibility warning or preserved-source diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatDiagnostic {
    /// Stable machine-readable category.
    pub code: String,
    /// File part or format in which the issue occurs.
    pub part: String,
    /// Property or element location, not executable source text.
    pub path: String,
    /// Human-readable explanation suitable for a future translated interface.
    pub message: String,
}

impl FormatDiagnostic {
    /// Constructs a diagnostic without interpreting its labels as document data.
    #[must_use]
    pub fn new(
        code: impl Into<String>,
        part: impl Into<String>,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: bounded_label(code.into(), 64),
            part: bounded_label(part.into(), 512),
            path: bounded_label(path.into(), 512),
            message: bounded_label(message.into(), 2048),
        }
    }
}

/// A conversion failure. No partial output or live document replacement occurs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatError {
    /// Stable category suitable for interface error handling.
    pub code: String,
    /// Explanation; callers must not use message text as a category.
    pub message: String,
}

impl FormatError {
    /// Constructs an explicit conversion or resource-limit error.
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: bounded_label(code.into(), 64),
            message: bounded_label(message.into(), 4096),
        }
    }
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for FormatError {}

impl From<office_core::ValidationError> for FormatError {
    fn from(error: office_core::ValidationError) -> Self {
        Self::new("invalid_model", error.to_string())
    }
}

impl From<office_core::DocumentError> for FormatError {
    fn from(error: office_core::DocumentError) -> Self {
        Self::new("invalid_model", error.to_string())
    }
}

impl From<office_core::PersistenceError> for FormatError {
    fn from(error: office_core::PersistenceError) -> Self {
        Self::new("invalid_file", error.to_string())
    }
}

/// An import is complete and editable, or its original is retained read-only.
#[derive(Debug)]
pub enum ImportOutcome {
    /// Validated complete projection, retaining any nonempty external original.
    Editable(WriterFile),
    /// No partial semantic projection is published for unsupported rich content.
    Preserved {
        /// Exact source bytes and format-neutral identifying metadata.
        original: OriginalSource,
        /// Reasons editing cannot safely be activated.
        report: Vec<FormatDiagnostic>,
    },
}

/// A single parsed import outcome and its non-blocking formatting notices.
///
/// Notices are runtime conversion information, not a second document model or
/// fields in the native Writer envelope. Unsupported content still produces
/// [`ImportOutcome::Preserved`] rather than a partial editable projection.
#[derive(Debug)]
pub struct ImportReport {
    /// Complete editable file or intact preserved source.
    pub outcome: ImportOutcome,
    /// At most 128 concrete formatting approximation notices.
    pub warnings: Vec<FormatDiagnostic>,
    /// Additional notices omitted independently from blocking diagnostics.
    pub omitted_warnings: usize,
}

/// Explicit conversion losses and fidelity limitations requiring acknowledgement.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportReport {
    /// Warnings to present before publishing converted bytes.
    pub warnings: Vec<FormatDiagnostic>,
}

/// Imports a file without changing an editing session or accessing storage.
pub fn import_document(
    format: DocumentFormat,
    bytes: &[u8],
    document_id: Id,
    filename: &str,
) -> Result<ImportOutcome, FormatError> {
    Ok(import_document_with_warnings(format, bytes, document_id, filename)?.outcome)
}

/// Imports once and returns non-blocking notices alongside the existing outcome.
///
/// User-facing callers should present these notices after preserving the full
/// native file and original. The legacy [`import_document`] API retains its
/// return type; it performs this same import without exposing the notice report.
pub fn import_document_with_warnings(
    format: DocumentFormat,
    bytes: &[u8],
    document_id: Id,
    filename: &str,
) -> Result<ImportReport, FormatError> {
    let limit = if format == DocumentFormat::Writer {
        office_core::MAX_WRITER_FILE_BYTES
    } else {
        MAX_INPUT_BYTES
    };
    if bytes.len() > limit {
        return Err(FormatError::new(
            "file_too_large",
            format!("Document exceeds the {limit}-byte import limit."),
        ));
    }
    if document_id.as_str().len() > 1024 {
        return Err(FormatError::new(
            "invalid_identifier",
            "Import document identifiers are limited to 1,024 UTF-8 bytes.",
        ));
    }
    let mut warnings = Vec::new();
    let mut omitted_warnings = 0;
    let outcome = match format {
        DocumentFormat::Writer => Ok(ImportOutcome::Editable(
            office_core::deserialize_writer_file(bytes)?,
        )),
        DocumentFormat::Docx => match office_docx::import_docx(bytes, document_id)
            .map_err(|e| FormatError::new("invalid_docx", e.to_string()))?
        {
            office_docx::DocxImportOutcome::Editable(imported) => {
                let report = imported.report();
                warnings.extend(report.normalizations.iter().map(|notice| {
                    FormatDiagnostic::new(
                        notice.kind.code(),
                        notice.part.clone(),
                        notice.path.clone(),
                        notice.message.clone(),
                    )
                }));
                omitted_warnings = report.omitted_normalizations;
                let (document, _) = imported.into_parts();
                Ok(ImportOutcome::Editable(retain_source(
                    format, document, bytes, filename,
                )?))
            }
            office_docx::DocxImportOutcome::Preserved { report, .. } => {
                Ok(ImportOutcome::Preserved {
                    original: original_source(format, bytes, filename)?.ok_or_else(|| {
                        FormatError::new("invalid_docx", "DOCX original is empty.")
                    })?,
                    report: report
                        .diagnostics
                        .into_iter()
                        .map(|d| FormatDiagnostic::new(d.kind.code(), d.part, d.path, d.message))
                        .collect(),
                })
            }
        },
        DocumentFormat::Odt => odt::import_document(bytes, document_id, filename),
        DocumentFormat::Rtf => rtf::import_document(bytes, document_id, filename),
        DocumentFormat::Txt => text::import_document(bytes, document_id, filename),
        DocumentFormat::Pdf => Err(FormatError::new(
            "export_only",
            "PDF is export only; open an editable document format.",
        )),
    }?;
    Ok(ImportReport {
        outcome,
        warnings,
        omitted_warnings,
    })
}

/// Describes expected conversion losses before external-format saving.
///
/// The semantic model is never modified to make a conversion fit. Callers must
/// separately preserve full native files and acknowledge these warnings.
pub fn assess_export(
    format: DocumentFormat,
    document: &Document,
) -> Result<ExportReport, FormatError> {
    document.validate()?;
    let mut warnings = Vec::new();
    if format != DocumentFormat::Writer {
        warnings.push(FormatDiagnostic::new("native_information", format.extension(), "document", "This format does not retain native Writer identifiers, exact run/property distinctions or retained original files. Keep a .writer copy for full native preservation. Editing history is not persisted by any current file format."));
    }
    match format {
        DocumentFormat::Odt => {
            if document
                .sections()
                .iter()
                .filter_map(|section| section.properties())
                .filter_map(|properties| properties.page_margins)
                .any(|margins| margins.header.0 != 0 || margins.footer.0 != 0)
            {
                warnings.push(FormatDiagnostic::new("odt_header_footer_offsets", "odt", "section/page_margins", "ODT export preserves body dimensions and margins, but does not retain the native model's currently unused header/footer offset values. Keep the .writer copy to retain them."));
            }
        }
        DocumentFormat::Txt => {
            text::validate_export(document)?;
            warnings.push(FormatDiagnostic::new("plain_text_only", "txt", "document", "TXT keeps text only. Formatting, styles, title metadata, page geometry and section/soft-line-break distinctions are not preserved. Output is UTF-8 with LF line endings."));
        }
        DocumentFormat::Pdf => {
            warnings.push(FormatDiagnostic::new("outlined_pdf", "pdf", "document", "PDF preserves laid-out appearance as vector outlines. Text is not searchable or selectable, and this export is not PDF/UA tagged or an editable Writer document."));
            warnings.extend(pdf::assess_document(document)?);
        }
        _ => {}
    }
    Ok(ExportReport { warnings })
}

/// Produces complete external-format bytes from an editable model.
///
/// This low-level function does not present dialogs; user-facing callers must
/// present [`assess_export`] warnings before publishing external output.
/// Native Writer output is refused here because a bare document cannot prove
/// whether a caller has an original to retain. Use [`export_file`] for Writer.
pub fn export_document(
    format: DocumentFormat,
    document: &Document,
) -> Result<Vec<u8>, FormatError> {
    document.validate()?;
    let bytes = match format {
        DocumentFormat::Writer => {
            return Err(FormatError::new(
                "source_aware_native_required",
                "Native Writer export requires a complete WriterFile; use export_file to preserve retained originals.",
            ));
        }
        DocumentFormat::Docx => office_docx::export_docx(document)
            .map_err(|e| FormatError::new("unsupported_docx_export", e.to_string()))?,
        DocumentFormat::Odt => odt::export_document(document)?,
        DocumentFormat::Rtf => rtf::export_document(document)?,
        DocumentFormat::Txt => text::export_document(document)?,
        DocumentFormat::Pdf => pdf::export_document(document)?,
    };
    if bytes.len() > MAX_OUTPUT_BYTES {
        return Err(FormatError::new(
            "file_too_large",
            "Export exceeds the 32 MiB output limit.",
        ));
    }
    // An exporter must not publish a copy that exceeds this adapter's own
    // editable profile (for example token counts, XML limits or table labels).
    // This is a bounded reopenability check, not a claim of semantic fidelity
    // or interoperability; independent-consumer tests remain necessary.
    if format != DocumentFormat::Pdf {
        let checked = import_document(
            format,
            &bytes,
            Id::new("writer-export-validation")?,
            &format!("copy.{}", format.extension()),
        );
        match checked {
            Ok(ImportOutcome::Editable(_)) => {}
            Ok(ImportOutcome::Preserved { report, .. }) => {
                return Err(FormatError::new(
                    "unsupported_export",
                    format!(
                        "{} export cannot be reopened within Writer's editable profile: {}",
                        format.extension().to_ascii_uppercase(),
                        report
                            .first()
                            .map_or("unsupported content", |diagnostic| diagnostic
                                .message
                                .as_str())
                    ),
                ));
            }
            Err(error) => {
                return Err(FormatError::new(
                    "unsupported_export",
                    format!(
                        "{} export cannot be reopened within Writer's editable profile: {}",
                        format.extension().to_ascii_uppercase(),
                        error.message
                    ),
                ));
            }
        }
    }
    Ok(bytes)
}

/// Exports a full native file, or converts its current document to another format.
///
/// Writer output retains the exact original when present. Such a file may
/// contain content removed from the current document; interfaces must disclose
/// that before sharing. External output retains current supported content only
/// and callers must present [`assess_export`] warnings before publishing it.
pub fn export_file(format: DocumentFormat, file: &WriterFile) -> Result<Vec<u8>, FormatError> {
    if format == DocumentFormat::Writer {
        Ok(office_core::serialize_writer_file(file)?)
    } else {
        export_document(format, file.document())
    }
}

pub(crate) fn original_source(
    format: DocumentFormat,
    bytes: &[u8],
    filename: &str,
) -> Result<Option<OriginalSource>, FormatError> {
    if bytes.is_empty() && format == DocumentFormat::Txt {
        if filename.len() > 1024
            || filename.trim().is_empty()
            || matches!(filename, "." | "..")
            || filename
                .chars()
                .any(|ch| ch.is_control() || matches!(ch, '/' | '\\'))
        {
            return Err(FormatError::new(
                "invalid_file",
                "Expected a nonblank leaf filename of at most 1,024 UTF-8 bytes.",
            ));
        }
        return Ok(None);
    }
    Ok(Some(OriginalSource::new(
        format.extension(),
        format.media_type(),
        filename,
        bytes.to_vec(),
    )?))
}

fn bounded_label(mut text: String, limit: usize) -> String {
    if text.len() > limit {
        let mut end = limit.saturating_sub(3);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push('…');
    }
    text
}

pub(crate) fn retain_source(
    format: DocumentFormat,
    document: Document,
    bytes: &[u8],
    filename: &str,
) -> Result<WriterFile, FormatError> {
    match original_source(format, bytes, filename)? {
        Some(source) => Ok(WriterFile::with_original_source(document, source)?),
        None => Ok(WriterFile::new(document)),
    }
}
