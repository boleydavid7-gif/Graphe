//! Filesystem-independent file-format adapters around the shared Rust model.
//!
//! Supported projections are editable only when every source construct has a
//! supported interpretation. Unsupported rich sources remain exact originals,
//! without publishing a partial editable document. External formats are not a
//! substitute for native Writer files: stable IDs, exact native property
//! distinctions and retained originals are not portable conversion guarantees.

use std::{error::Error, fmt};

use office_core::{
    Alignment, Block, CharacterFormat, Document, Id, OriginalSource, PageOrientation, TextColor,
    Underline, WriterFile,
};

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
    /// Whether imported formatting may differ, tracked before notice truncation.
    ///
    /// This runtime summary lets source-aware export disclose a meaningful
    /// approximation even when individual notices exceed the display limit.
    pub has_formatting_normalizations: bool,
}

/// Non-blocking notices have an independent budget from unsafe-content findings.
#[derive(Default)]
pub(crate) struct ImportNotices {
    warnings: Vec<FormatDiagnostic>,
    omitted: usize,
    meaningful_formatting: bool,
}

impl ImportNotices {
    pub(crate) fn record(
        &mut self,
        part: &str,
        path: impl Into<String>,
        message: impl Into<String>,
        meaningful: bool,
    ) {
        self.meaningful_formatting |= meaningful;
        if self.warnings.len() < 128 {
            self.warnings.push(FormatDiagnostic::new(
                "normalized_formatting",
                part,
                path,
                message,
            ));
        } else {
            self.omitted = self.omitted.saturating_add(1);
        }
    }

    pub(crate) fn finish(self, outcome: ImportOutcome) -> ImportReport {
        ImportReport {
            outcome,
            warnings: self.warnings,
            omitted_warnings: self.omitted,
            has_formatting_normalizations: self.meaningful_formatting,
        }
    }
}

/// Meaningful conversion losses to present before saving a copy.
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
    let mut has_formatting_normalizations = false;
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
                has_formatting_normalizations = report.has_formatting_normalizations;
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
        DocumentFormat::Odt => {
            return odt::import_document_with_warnings(bytes, document_id, filename);
        }
        DocumentFormat::Rtf => {
            return rtf::import_document_with_warnings(bytes, document_id, filename);
        }
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
        has_formatting_normalizations,
    })
}

/// Describes expected conversion losses before external-format saving.
///
/// The semantic model is never modified to make a conversion fit. Native
/// identifiers, unused property distinctions and retained-original bytes are
/// not user-visible formatting losses. Source-aware callers use [`assess_file`].
pub fn assess_export(
    format: DocumentFormat,
    document: &Document,
) -> Result<ExportReport, FormatError> {
    document.validate()?;
    let mut warnings = Vec::new();
    match format {
        DocumentFormat::Txt => {
            text::validate_export(document)?;
            if text_loses_formatting(document) {
                warnings.push(FormatDiagnostic::new(
                    "plain_text_only",
                    "txt",
                    "document",
                    "Plain text will lose this document's formatting or page layout.",
                ));
            }
        }
        DocumentFormat::Pdf => {
            warnings.push(FormatDiagnostic::new(
                "outlined_pdf",
                "pdf",
                "document",
                "Text in this PDF cannot be searched, selected or read by screen readers.",
            ));
            for diagnostic in pdf::assess_document(document)? {
                let (code, message) = match diagnostic.code.as_str() {
                    "pdf_layout_missing_glyph" => (
                        "pdf_layout_missing_glyph",
                        "Some characters cannot be shown in the PDF.",
                    ),
                    _ => (
                        "pdf_layout_changes",
                        "Some fonts or text placement may change or extend beyond the PDF page.",
                    ),
                };
                if !warnings.iter().any(|warning| warning.code == code) {
                    warnings.push(FormatDiagnostic::new(code, "pdf", "document", message));
                }
            }
        }
        _ => {}
    }
    Ok(ExportReport { warnings })
}

/// Describes meaningful losses when saving a complete native file as a copy.
///
/// Native saving preserves the complete file without a conversion warning.
/// Interfaces separately disclose that an included original can contain removed
/// text. DOCX copies preserve validated source information when available.
pub fn assess_file(format: DocumentFormat, file: &WriterFile) -> Result<ExportReport, FormatError> {
    let mut report = assess_export(format, file.document())?;
    if format != DocumentFormat::Writer {
        assess_normalized_original(format, file, &mut report)?;
    }
    if format != DocumentFormat::Writer
        && let Some(source) = docx_source(file)?
    {
        let assessment = office_docx::assess_docx_copy(file.document(), &source)
            .map_err(|error| FormatError::new("unsupported_docx_export", error.to_string()))?;
        if assessment.formatting_may_differ {
            let (code, message) = if format == DocumentFormat::Docx {
                (
                    "docx_formatting_changes",
                    "Some original formatting may look different in this Word copy.",
                )
            } else {
                (
                    "original_formatting_changes",
                    "Some original formatting may look different in this copy. The exact original remains in the native Writer document.",
                )
            };
            report.warnings.push(FormatDiagnostic::new(
                code,
                format.extension(),
                "document",
                message,
            ));
        }
        if format == DocumentFormat::Docx && assessment.theme_omitted {
            report.warnings.push(FormatDiagnostic::new(
                "docx_theme_omitted",
                "docx",
                "document",
                "Some original theme details won't be included in this Word copy.",
            ));
        }
        if assessment.bookmarks_omitted {
            let (code, message) = if format == DocumentFormat::Docx {
                (
                    "docx_bookmarks_omitted",
                    "Bookmarks in the original will not be included in this Word copy.",
                )
            } else {
                (
                    "bookmarks_omitted",
                    "Bookmarks in the original won't be included in this copy.",
                )
            };
            report.warnings.push(FormatDiagnostic::new(
                code,
                format.extension(),
                "document",
                message,
            ));
        }
        if format != DocumentFormat::Docx && assessment.has_document_properties {
            report.warnings.push(FormatDiagnostic::new(
                "document_properties_omitted",
                format.extension(),
                "document",
                "Some document properties won't be included in this copy.",
            ));
        }
    }
    Ok(report)
}

fn assess_normalized_original(
    destination: DocumentFormat,
    file: &WriterFile,
    report: &mut ExportReport,
) -> Result<(), FormatError> {
    let Some(original) = file.original_source() else {
        return Ok(());
    };
    let source_format = match original.format() {
        "odt" => DocumentFormat::Odt,
        "rtf" => DocumentFormat::Rtf,
        _ => return Ok(()),
    };
    if !original
        .media_type()
        .eq_ignore_ascii_case(source_format.media_type())
    {
        return Err(FormatError::new(
            "invalid_original_source",
            "The original document has inconsistent format information.",
        ));
    }
    let imported = import_document_with_warnings(
        source_format,
        original.bytes(),
        Id::new("writer-original-assessment")?,
        original.filename(),
    )?;
    if !matches!(imported.outcome, ImportOutcome::Editable(_)) {
        return Err(FormatError::new(
            "unsupported_original_source",
            "The original cannot be projected completely; an editable copy cannot be assessed safely.",
        ));
    }
    if imported.has_formatting_normalizations {
        report.warnings.push(FormatDiagnostic::new(
            "original_formatting_changes",
            destination.extension(),
            "document",
            "Some original formatting may look different in this copy. The exact original remains in the native Writer document.",
        ));
    }
    Ok(())
}

fn docx_source(file: &WriterFile) -> Result<Option<office_docx::DocxSource>, FormatError> {
    let Some(original) = file
        .original_source()
        .filter(|original| original.format() == "docx")
    else {
        return Ok(None);
    };
    if !original
        .media_type()
        .eq_ignore_ascii_case(office_docx::DOCX_MEDIA_TYPE)
    {
        return Err(FormatError::new(
            "invalid_docx_source",
            "The original Word document has inconsistent format information.",
        ));
    }
    office_docx::DocxSource::open(original.bytes())
        .map(Some)
        .map_err(|error| FormatError::new("invalid_docx_source", error.to_string()))
}

fn text_loses_formatting(document: &Document) -> bool {
    let defaults = office_layout::LayoutOptions::default();
    if document.sections().len() > 1 {
        return true;
    }
    for section in document.sections() {
        if let Some(properties) = section.properties() {
            if let Some(size) = properties.page_size
                && (size.width != defaults.page_size.width
                    || size.height != defaults.page_size.height
                    || size.orientation == Some(PageOrientation::Landscape))
            {
                return true;
            }
            if let Some(margins) = properties.page_margins
                && (margins.top != defaults.page_margins.top
                    || margins.right != defaults.page_margins.right
                    || margins.bottom != defaults.page_margins.bottom
                    || margins.left != defaults.page_margins.left
                    || margins.gutter != defaults.page_margins.gutter)
            {
                return true;
            }
        }
        for block in section.blocks() {
            let Block::Paragraph(paragraph) = block;
            let inherited = office_layout::resolve_paragraph_style(document, paragraph);
            let paragraph_format = inherited.paragraph;
            if paragraph_format.alignment.unwrap_or(Alignment::Left) != Alignment::Left
                || paragraph_format
                    .spacing_before
                    .is_some_and(|value| value.0 != 0)
                || paragraph_format
                    .spacing_after
                    .unwrap_or(defaults.spacing_after)
                    != defaults.spacing_after
                || paragraph_format
                    .line_spacing
                    .unwrap_or(defaults.line_spacing)
                    != defaults.line_spacing
                || paragraph_format.page_break_before == Some(true)
                || paragraph_format.indentation.is_some_and(|indentation| {
                    [indentation.left, indentation.right, indentation.first_line]
                        .into_iter()
                        .flatten()
                        .any(|value| value.0 != 0)
                })
            {
                return true;
            }
            if paragraph.runs().is_empty() && nondefault_character(&inherited.character, &defaults)
            {
                return true;
            }
            for run in paragraph.runs() {
                let mut format = inherited.character.clone();
                office_layout::overlay_character(&mut format, &run.format);
                if nondefault_character(&format, &defaults) || run.text.contains(['\n', '\r']) {
                    return true;
                }
            }
        }
    }
    false
}

fn nondefault_character(format: &CharacterFormat, defaults: &office_layout::LayoutOptions) -> bool {
    format.bold == Some(true)
        || format.italic == Some(true)
        || format
            .underline
            .is_some_and(|value| value != Underline::None)
        || format
            .font_family
            .as_deref()
            .is_some_and(|family| !family.eq_ignore_ascii_case(&defaults.font_family))
        || format
            .font_size
            .is_some_and(|size| size != defaults.font_size)
        || format.color.is_some_and(|color| {
            !matches!(
                color,
                TextColor::Auto {}
                    | TextColor::Rgb {
                        red: 36,
                        green: 33,
                        blue: 29
                    }
            )
        })
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
    checked_export(format, bytes)
}

fn checked_export(format: DocumentFormat, bytes: Vec<u8>) -> Result<Vec<u8>, FormatError> {
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
/// that before sharing. DOCX copies also retain validated source metadata and
/// safe auxiliary information without restoring removed body text. Callers
/// present [`assess_file`] warnings before publishing a converted copy.
pub fn export_file(format: DocumentFormat, file: &WriterFile) -> Result<Vec<u8>, FormatError> {
    if format == DocumentFormat::Writer {
        Ok(office_core::serialize_writer_file(file)?)
    } else if format == DocumentFormat::Docx
        && let Some(source) = docx_source(file)?
    {
        let bytes = office_docx::export_docx_with_source(file.document(), &source)
            .map_err(|error| FormatError::new("unsupported_docx_export", error.to_string()))?;
        checked_export(format, bytes)
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
