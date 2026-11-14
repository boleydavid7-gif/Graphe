//! Format selection, source preservation and revision-bound Save As transport.

use office_core::{Id, OriginalSource};
use office_formats::{DocumentFormat, FormatDiagnostic, FormatError, ImportOutcome};
use serde::Serialize;

use crate::{BrowserEngine, EngineError, MAX_DOCUMENT_BYTES, checked_file, limit_error, wire};

#[derive(Serialize)]
struct ExportWarning {
    code: String,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportAssessment<'a> {
    session_id: &'a str,
    document_id: &'a str,
    revision: String,
    format: &'a str,
    warnings: Vec<ExportWarning>,
}

impl BrowserEngine {
    /// Imports a selected format completely before replacing this session.
    ///
    /// PDF is export-only. Nonempty external inputs retain their exact original
    /// outside history. Browser callers must persist the detached candidate's
    /// full native file before activating it for editing. Empty TXT has no
    /// retained source because the native source record requires nonempty bytes.
    pub fn import_format(&mut self, bytes: &[u8], format: &str, filename: &str) -> String {
        let next_id = self.next_id;
        let result = self.import_selected_format(bytes, format, filename);
        if result.is_err() {
            // Rejected conversion must not consume identifiers from later edits.
            self.next_id = next_id;
        }
        self.response(result)
    }

    fn import_selected_format(
        &mut self,
        bytes: &[u8],
        format: &str,
        filename: &str,
    ) -> Result<(), EngineError> {
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(limit_error());
        }
        let format = DocumentFormat::parse(format).map_err(format_error)?;
        if format == DocumentFormat::Writer {
            return self.open(bytes, None);
        }
        if format == DocumentFormat::Pdf {
            return Err(EngineError::new(
                "export_only_format",
                "PDF is available for export only. Open an editable document format instead.",
            ));
        }
        let id = self.allocate_id()?;
        let file = match office_formats::import_document(format, bytes, id, filename)
            .map_err(format_error)?
        {
            ImportOutcome::Editable(file) => file,
            ImportOutcome::Preserved { report, .. } => {
                return Err(unsupported_import(format, report));
            }
        };
        let (document, original) = file.into_parts();
        if !bytes.is_empty()
            && !original.as_ref().is_some_and(|source| {
                source.bytes() == bytes
                    && source.format() == format.extension()
                    && source
                        .media_type()
                        .eq_ignore_ascii_case(format.media_type())
                    && source.filename() == filename
            })
        {
            return Err(EngineError::new(
                "invalid_import",
                "The converted document did not retain its exact original source. The active document is unchanged.",
            ));
        }
        let (_, overhead) = checked_file(&document, original.clone())?;
        self.replace_session(document)?;
        self.original_source = original;
        self.source_overhead = overhead;
        Ok(())
    }

    /// Reports conversion losses for an exact Rust document revision.
    ///
    /// The returned origin must still match before confirming Save As. Native
    /// source retention is disclosed because originals may contain removed text.
    pub fn export_assessment(
        &self,
        format: &str,
        expected_revision: Option<&str>,
    ) -> Result<String, EngineError> {
        self.check_export_revision(expected_revision)?;
        let format = DocumentFormat::parse(format).map_err(format_error)?;
        let warnings = self.export_warnings(format)?;
        let response = ExportAssessment {
            session_id: &self.session_id,
            document_id: self.session.document().id().as_str(),
            revision: self.revision(),
            format: format.extension(),
            warnings,
        };
        serde_json::to_string(&response)
            .map_err(|error| EngineError::new("export_failed", error.to_string()))
    }

    /// Exports current content after checking origin and acknowledging losses.
    ///
    /// Failure never changes document content, selection, source or history.
    /// Output is fully generated and bounded before bytes are returned.
    pub fn export_format(
        &self,
        format: &str,
        acknowledge_loss: bool,
        expected_revision: Option<&str>,
    ) -> Result<Vec<u8>, EngineError> {
        self.check_export_revision(expected_revision)?;
        let format = DocumentFormat::parse(format).map_err(format_error)?;
        let warnings = self.export_warnings(format)?;
        if !warnings.is_empty() && !acknowledge_loss {
            return Err(EngineError::new(
                "export_confirmation_required",
                "Review and acknowledge this format's warnings before saving a copy.",
            ));
        }
        let bytes = if format == DocumentFormat::Writer {
            self.export_document()?
        } else {
            office_formats::export_document(format, self.session.document())
                .map_err(format_error)?
        };
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(limit_error());
        }
        Ok(bytes)
    }

    fn check_export_revision(&self, expected_revision: Option<&str>) -> Result<(), EngineError> {
        if expected_revision.is_some_and(|expected| expected != self.revision()) {
            return Err(EngineError::new(
                "stale_revision",
                "The document changed after Save As was prepared. Review its warnings again.",
            ));
        }
        Ok(())
    }

    fn export_warnings(&self, format: DocumentFormat) -> Result<Vec<ExportWarning>, EngineError> {
        self.check_candidate(self.session.document())?;
        let report =
            office_formats::assess_export(format, self.session.document()).map_err(format_error)?;
        let omitted = report.warnings.len().saturating_sub(127);
        let mut warnings = report
            .warnings
            .into_iter()
            .take(127)
            .map(|warning| ExportWarning {
                code: bounded_label(warning.code, 64),
                message: bounded_label(warning.message, 2048),
            })
            .collect::<Vec<_>>();
        if omitted > 0 {
            warnings.push(ExportWarning {
                code: "additional_warnings".into(),
                message: format!(
                    "{omitted} additional compatibility warnings were omitted from this bounded report. Keep a native Writer copy to preserve the full document."
                ),
            });
        }
        if format == DocumentFormat::Writer && self.original_source.is_some() {
            warnings.push(ExportWarning {
                code: "retained_original".into(),
                message: "This native Writer file includes the exact imported original, which may contain text or metadata removed from the edited document.".into(),
            });
        }
        Ok(warnings)
    }
}

pub(super) fn validate_source(source: &OriginalSource, id: Id) -> Result<(), EngineError> {
    let format = DocumentFormat::parse(source.format()).map_err(|_| {
        EngineError::new(
            "unsupported_source",
            "This Writer file retains an unsupported source format.",
        )
    })?;
    if !matches!(
        format,
        DocumentFormat::Odt | DocumentFormat::Rtf | DocumentFormat::Txt
    ) || !source
        .media_type()
        .eq_ignore_ascii_case(format.media_type())
    {
        return Err(EngineError::new(
            "unsupported_source",
            "This Writer file's original source format and media type are unsupported or inconsistent.",
        ));
    }
    match office_formats::import_document(format, source.bytes(), id, source.filename())
        .map_err(format_error)?
    {
        ImportOutcome::Editable(_) => Ok(()),
        ImportOutcome::Preserved { report, .. } => Err(unsupported_import(format, report)),
    }
}

fn format_error(error: FormatError) -> EngineError {
    EngineError::new(&error.code, error.message)
}

fn unsupported_import(format: DocumentFormat, report: Vec<FormatDiagnostic>) -> EngineError {
    let omitted = report.len().saturating_sub(128);
    EngineError {
        code: "unsupported_format".into(),
        message: format!(
            "This {} contains unsupported content. Its exact original is preserved read-only; no editable copy was opened.",
            format.extension().to_ascii_uppercase()
        ),
        compatibility: Some(wire::CompatibilityReportView {
            omitted_diagnostics: omitted,
            diagnostics: report
                .into_iter()
                .take(128)
                .map(|diagnostic| wire::CompatibilityDiagnosticView {
                    kind: bounded_label(diagnostic.code, 64),
                    part: bounded_label(diagnostic.part, 512),
                    path: bounded_label(diagnostic.path, 512),
                    message: bounded_label(diagnostic.message, 2048),
                })
                .collect(),
        }),
    }
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
