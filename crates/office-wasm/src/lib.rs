//! Browser-facing session adapter. The independent core owns all committed edits.
//!
//! JSON is a presentation/command transport, never a second document schema.
//! Native files are exchanged through the core's exact `.writer` byte format.
//! Browser UTF-16 endpoints are deliberately snapped outward to Rust extended
//! grapheme boundaries; scalar-invalid surrogate positions are rejected.

mod find;
#[cfg(target_arch = "wasm32")]
mod fonts;
mod formats;
mod images;
mod layout;
mod operations;
mod tables;
mod view;
mod wire;

use office_core::{
    Block, CharacterFormat, Document, DocumentSession, Id, OriginalSource, Paragraph, Section,
    TextCoordinates, Transaction, WriterFile, deserialize_writer_file, serialize_document,
    serialize_writer_file,
};
use office_docx::{CompatibilityReport, DocxImportOutcome};
use serde::Serialize;
use unicode_segmentation::UnicodeSegmentation;
use wire::{BrowserSelection, Command, Point, View};
pub use wire::{CompatibilityDiagnosticView, CompatibilityReportView};

/// Maximum native input/output size accepted by the first browser prototype.
pub const MAX_DOCUMENT_BYTES: usize = 5 * 1024 * 1024;
const MAX_UTF16: usize = 200_000;
const MAX_PARAGRAPHS: usize = 2_000;
const HISTORY_LIMIT: usize = 20;

/// Structured adapter error suitable for a future translated interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EngineError {
    /// Stable machine-readable category.
    pub code: String,
    /// Human-readable diagnostic, without changing the active document.
    pub message: String,
    /// Bounded compatibility details when a source cannot be edited safely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<wire::CompatibilityReportView>,
}

impl EngineError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            compatibility: None,
        }
    }
}

macro_rules! convert_error {
    ($kind:ty, $code:expr) => {
        impl From<$kind> for EngineError {
            fn from(error: $kind) -> Self {
                Self::new($code, error.to_string())
            }
        }
    };
}
convert_error!(office_core::ValidationError, "invalid_document");
convert_error!(office_core::PositionError, "invalid_selection");
convert_error!(office_core::EditError, "invalid_edit");
convert_error!(office_core::TransactionError, "transaction_rejected");
convert_error!(office_core::HistoryError, "history_unavailable");
convert_error!(office_core::PersistenceError, "invalid_file");
convert_error!(office_docx::ConversionError, "invalid_docx");

#[derive(Clone)]
struct CursorHistory {
    before: Option<BrowserSelection>,
    after: Option<BrowserSelection>,
    image_before: Option<Id>,
    image_after: Option<Id>,
}

/// Owns one live validated document, Rust undo history, and browser cursor state.
///
/// The caller supplies a unique namespace (normally `crypto.randomUUID()`). Rust
/// allocates entity identifiers and never exposes revision counters as JS numbers.
pub struct BrowserEngine {
    namespace: String,
    next_id: u64,
    next_session: u64,
    session_id: String,
    session: DocumentSession,
    selection: Option<BrowserSelection>,
    selected_image: Option<Id>,
    typing: Option<CharacterFormat>,
    undo_cursor: Vec<CursorHistory>,
    redo_cursor: Vec<CursorHistory>,
    original_source: Option<OriginalSource>,
    import_warnings: wire::CompatibilityReportView,
    source_overhead: usize,
    layout_state: layout::LayoutState,
}

impl BrowserEngine {
    /// Creates a blank Writer document with one editable paragraph.
    pub fn new(namespace: &str) -> Result<Self, EngineError> {
        if namespace.trim() != namespace || namespace.is_empty() || namespace.len() > 128 {
            return Err(EngineError::new(
                "invalid_namespace",
                "A nonblank namespace of at most 128 bytes is required.",
            ));
        }
        let document = Document::new(Id::new(format!("{namespace}:initial"))?)?;
        let mut engine = Self {
            namespace: namespace.into(),
            next_id: 0,
            next_session: 0,
            session_id: String::new(),
            session: DocumentSession::with_history_limit(document, HISTORY_LIMIT),
            selection: None,
            selected_image: None,
            typing: None,
            undo_cursor: Vec::new(),
            redo_cursor: Vec::new(),
            original_source: None,
            import_warnings: Default::default(),
            source_overhead: 0,
            layout_state: layout::LayoutState::default(),
        };
        engine.new_document()?;
        Ok(engine)
    }

    /// Returns a read-only JSON presentation snapshot with explicit UTF-16 offsets.
    pub fn snapshot(&self) -> String {
        serde_json::to_string(&self.make_view())
            .expect("presentation snapshot contains finite values")
    }

    /// Executes one JSON command and returns `{ok, view}` or `{ok, error}`.
    ///
    /// A rejected command never publishes document mutations. One accepted edit
    /// command is at most one core transaction and one undo step.
    pub fn dispatch(&mut self, json: &str) -> String {
        if json.len() > MAX_DOCUMENT_BYTES * 2 {
            return self.response(Err(limit_error()));
        }
        let result = serde_json::from_str::<Command>(json)
            .map_err(|error| EngineError::new("invalid_command", error.to_string()))
            .and_then(|command| self.execute(command));
        self.response(result)
    }

    /// Serializes an owned native Writer byte snapshot, without browser storage.
    pub fn export_document(&self) -> Result<Vec<u8>, EngineError> {
        self.check_candidate(self.session.document())?;
        let (bytes, _) = checked_file(self.session.document(), self.original_source.clone())?;
        Ok(bytes)
    }

    /// Imports a complete supported DOCX; unsupported content leaves the session intact.
    ///
    /// Browser callers must use a detached candidate and save its source-bearing
    /// native bytes before activating it for editing.
    pub fn import_docx(&mut self, bytes: &[u8], filename: &str) -> String {
        let next_id = self.next_id;
        let result = self.import_word(bytes, filename);
        if result.is_err() {
            self.next_id = next_id;
        }
        self.response(result)
    }

    fn import_word(&mut self, bytes: &[u8], filename: &str) -> Result<(), EngineError> {
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(limit_error());
        }
        let document_id = self.allocate_id()?;
        let imported = match office_docx::import_docx(bytes, document_id)? {
            DocxImportOutcome::Editable(imported) => imported,
            DocxImportOutcome::Preserved { report, .. } => return Err(unsupported_docx(report)),
        };
        let warnings = normalization_warnings(imported.report());
        let (document, source) = imported.into_parts();
        let original = OriginalSource::new(
            "docx",
            office_docx::DOCX_MEDIA_TYPE,
            filename,
            source.into_original_bytes(),
        )?;
        let (_, overhead) = checked_file(&document, Some(original.clone()))?;
        self.replace_session(document)?;
        self.original_source = Some(original);
        self.import_warnings = warnings;
        self.source_overhead = overhead;
        Ok(())
    }

    /// Regenerates DOCX from current supported content, independently of its original.
    pub fn export_docx(&self) -> Result<Vec<u8>, EngineError> {
        self.check_candidate(self.session.document())?;
        let bytes = office_docx::export_docx(self.session.document())?;
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(limit_error());
        }
        Ok(bytes)
    }

    /// Returns exact retained source bytes, including content removed by later edits.
    pub fn export_original(&self) -> Result<Vec<u8>, EngineError> {
        self.original_source
            .as_ref()
            .map(|source| source.bytes().to_vec())
            .ok_or_else(|| {
                EngineError::new("no_original", "This document has no retained original.")
            })
    }

    /// Validates native bytes completely before replacing the active session.
    ///
    /// Empty imported documents remain structurally empty until the first edit.
    /// Opening starts fresh runtime revision, selection, and undo/redo state.
    pub fn open_document(&mut self, bytes: &[u8]) -> String {
        let result = self.open(bytes, None);
        self.response(result)
    }

    /// Opens native bytes only when their identity matches the storage record.
    /// Mismatches preserve the active session and history completely.
    pub fn open_document_verified(&mut self, bytes: &[u8], expected_id: &str) -> String {
        let result = self.open(bytes, Some(expected_id));
        self.response(result)
    }

    fn open(&mut self, bytes: &[u8], expected_id: Option<&str>) -> Result<(), EngineError> {
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(limit_error());
        }
        let file = deserialize_writer_file(bytes)?;
        let (document, original) = file.into_parts();
        if expected_id.is_some_and(|id| document.id().as_str() != id) {
            return Err(EngineError::new(
                "document_mismatch",
                "Stored document identity does not match its contents.",
            ));
        }
        let warnings = match &original {
            Some(source) => validate_source(source, document.id().clone())?,
            None => Default::default(),
        };
        let (_, overhead) = checked_file(&document, original.clone())?;
        self.replace_session(document)?;
        self.original_source = original;
        self.import_warnings = warnings;
        self.source_overhead = overhead;
        Ok(())
    }

    fn response(&self, result: Result<(), EngineError>) -> String {
        #[derive(Serialize)]
        #[serde(untagged)]
        enum Response {
            Success { ok: bool, view: Box<View> },
            Failure { ok: bool, error: EngineError },
        }
        let response = match result {
            Ok(()) => Response::Success {
                ok: true,
                view: Box::new(self.make_view()),
            },
            Err(error) => Response::Failure { ok: false, error },
        };
        serde_json::to_string(&response).expect("response contains finite values")
    }

    fn revision(&self) -> String {
        format!("{}:{}", self.session_id, self.session.revision().number())
    }

    fn execute(&mut self, command: Command) -> Result<(), EngineError> {
        if command
            .expected_revision()
            .is_some_and(|expected| expected != self.revision())
        {
            return Err(EngineError::new(
                "stale_revision",
                "This input belongs to an earlier document revision.",
            ));
        }
        if self.selected_image.is_some() {
            match &command {
                Command::NewDocument {}
                | Command::ForkDocument {}
                | Command::SetTitle { .. }
                | Command::SetSelection { .. }
                | Command::SelectImage { .. }
                | Command::ResizeImage { .. }
                | Command::UpdateImage { .. }
                | Command::SetImageAltText { .. }
                | Command::SetImageAlignment { .. }
                | Command::SetPageSetup { .. }
                | Command::DeleteImage { .. }
                | Command::MoveImageCursor { .. }
                | Command::Undo { .. }
                | Command::Redo { .. } => {}
                Command::Delete { .. } => return self.delete_image(),
                _ => {
                    return Err(EngineError::new(
                        "image_selected",
                        "Press Enter or an arrow key to return to text before editing it.",
                    ));
                }
            }
        }
        match command {
            Command::NewDocument {} => self.new_document(),
            Command::ForkDocument {} => {
                let id = self.allocate_id()?;
                let document = self.session.document();
                let document = Document::from_parts(
                    id,
                    document.metadata().clone(),
                    document.sections().to_vec(),
                    document.styles().to_vec(),
                )?;
                self.check_candidate(&document)?;
                self.replace_session(document)
            }
            Command::SetSelection {
                selection,
                affinity,
                ..
            } => {
                let trailing =
                    affinity.map(|affinity| matches!(affinity, wire::Affinity::Trailing));
                let next = selection
                    .map(|selection| self.normalize_selection_with_affinity(&selection, trailing))
                    .transpose()?;
                self.layout_state.reset_visual();
                if next != self.selection {
                    self.typing = None;
                }
                self.selection = next;
                self.selected_image = None;
                Ok(())
            }
            Command::SelectImage { image_id, .. } => self.select_image(image_id),
            Command::ResizeImage {
                width_pt,
                height_pt,
                lock_aspect,
                ..
            } => self.resize_selected_image(width_pt, height_pt, lock_aspect),
            Command::UpdateImage {
                width_pt,
                height_pt,
                lock_aspect,
                alt_text,
                alignment,
                ..
            } => self.update_selected_image(images::ImageUpdate {
                width_pt,
                height_pt,
                lock_aspect,
                alt_text,
                alignment,
            }),
            Command::SetImageAltText { alt_text, .. } => self.set_image_alt_text(alt_text),
            Command::SetImageAlignment { alignment, .. } => self.set_image_alignment(alignment),
            Command::DeleteImage { .. } => self.delete_image(),
            Command::MoveImageCursor { direction, .. } => self.move_image_cursor(direction),
            Command::SetTitle { title, .. } => {
                let mut transaction = self.session.begin_transaction();
                transaction.draft_mut().metadata.title = title;
                self.commit(transaction, self.selection.clone())
            }
            Command::ReplaceText {
                text, selection, ..
            } => self.replace_text(&text, selection),
            Command::ReplaceAll { query, text, .. } => self.replace_all(&query, &text),
            Command::InsertParagraph { selection, .. } => self.insert_list_paragraph(selection),
            Command::InsertTable {
                rows,
                columns,
                selection,
                ..
            } => self.insert_table(rows, columns, selection),
            Command::TableAction {
                action, selection, ..
            } => self.table_action(action, selection),
            Command::MoveTableCell {
                direction,
                selection,
                ..
            } => self.move_table_cell(direction, selection),
            Command::InsertLineBreak { selection, .. } => self.insert_line_break(selection),
            Command::Delete {
                direction,
                selection,
                ..
            } => self.delete(direction, selection),
            Command::FormatCharacter {
                bold,
                italic,
                underline,
                font_family,
                font_size_pt,
                selection,
                ..
            } => {
                let format = character_patch(bold, italic, underline, font_family, font_size_pt)?;
                self.format_character(format, selection)
            }
            Command::FormatParagraph {
                alignment,
                selection,
                ..
            } => self.format_paragraph(alignment, selection),
            Command::SetPageSetup {
                section_id, page_size, page_margins, ..
            } => self.set_page_setup(&section_id, page_size, page_margins),
            Command::InsertPageBreak { selection, .. } => self.insert_page_break(selection),
            Command::InsertSectionBreak { selection, .. } => self.insert_section_break(selection),
            Command::RemoveSectionBreak { section_id, confirm_layout_change, .. } => {
                self.remove_section_break(&section_id, confirm_layout_change)
            },
            Command::SetPageBreakBefore { enabled, selection, .. } => {
                self.set_page_break_before(enabled, selection)
            }
            Command::SetParagraphMetrics {
                spacing_before_pt,
                spacing_after_pt,
                line_spacing_multiple,
                indent_left_in,
                indent_right_in,
                first_line_in,
                selection,
                ..
            } => self.set_paragraph_metrics(
                operations::ParagraphMetricsInput {
                    spacing_before_pt,
                    spacing_after_pt,
                    line_spacing_multiple,
                    indent_left_in,
                    indent_right_in,
                    first_line_in,
                },
                selection,
            ),
            Command::SetList {
                list, selection, ..
            } => self.set_list(list, selection),
            Command::SetHyperlink {
                url,
                text,
                selection,
                ..
            } => self.set_hyperlink(url, text, selection),
            Command::AssignStyle {
                style_id,
                selection,
                ..
            } => self.assign_style(style_id, selection),
            Command::Undo { .. } => {
                self.session.undo()?;
                self.layout_state.invalidate();
                let cursor = self
                    .undo_cursor
                    .pop()
                    .expect("cursor history follows core history");
                self.selection = cursor.before.clone();
                self.selected_image = cursor.image_before.clone();
                self.redo_cursor.push(cursor);
                self.typing = None;
                self.repair_cursor();
                Ok(())
            }
            Command::Redo { .. } => {
                self.session.redo()?;
                self.layout_state.invalidate();
                let cursor = self
                    .redo_cursor
                    .pop()
                    .expect("cursor history follows core history");
                self.selection = cursor.after.clone();
                self.selected_image = cursor.image_after.clone();
                self.undo_cursor.push(cursor);
                self.typing = None;
                self.repair_cursor();
                Ok(())
            }
        }
    }

    fn new_document(&mut self) -> Result<(), EngineError> {
        let document_id = self.allocate_id()?;
        let section_id = self.allocate_id()?;
        let paragraph_id = self.allocate_id()?;
        let document = Document::from_parts(
            document_id,
            Default::default(),
            vec![Section::new(
                section_id,
                vec![Block::Paragraph(Paragraph::new(paragraph_id, vec![]))],
            )],
            vec![],
        )?;
        self.replace_session(document)?;
        self.original_source = None;
        self.import_warnings = Default::default();
        self.source_overhead = 0;
        Ok(())
    }

    fn replace_session(&mut self, document: Document) -> Result<(), EngineError> {
        let number = self.next_session.checked_add(1).ok_or_else(|| {
            EngineError::new("identifier_exhausted", "Session identifiers exhausted.")
        })?;
        self.session = DocumentSession::with_history_limit(document, HISTORY_LIMIT);
        self.next_session = number;
        self.session_id = format!("{}:s{number}", self.namespace);
        self.selection = first_cursor(self.session.document());
        self.selected_image = None;
        self.typing = None;
        self.undo_cursor.clear();
        self.redo_cursor.clear();
        self.layout_state.invalidate();
        Ok(())
    }

    fn allocate_id(&mut self) -> Result<Id, EngineError> {
        loop {
            self.next_id = self.next_id.checked_add(1).ok_or_else(|| {
                EngineError::new("identifier_exhausted", "Element identifiers exhausted.")
            })?;
            let value = format!("{}:e{}", self.namespace, self.next_id);
            let document = self.session.document();
            let used = document.id().as_str() == value
                || document
                    .styles()
                    .iter()
                    .any(|style| style.id.as_str() == value)
                || document.sections().iter().any(|section| {
                    section.id().as_str() == value
                        || section.blocks().iter().any(|block| match block {
                            Block::Paragraph(paragraph) => paragraph.id().as_str() == value,
                            Block::Image(image) => image.id().as_str() == value,
                            Block::Table(table) => {
                                table.id().as_str() == value
                                    || table.rows().iter().any(|row| {
                                        row.id().as_str() == value
                                            || row.cells().iter().any(|cell| {
                                                cell.id().as_str() == value
                                                    || cell
                                                        .paragraphs()
                                                        .iter()
                                                        .any(|p| p.id().as_str() == value)
                                            })
                                    })
                            }
                        })
                });
            if !used {
                return Ok(Id::new(value)?);
            }
        }
    }

    fn commit(
        &mut self,
        transaction: Transaction,
        after: Option<BrowserSelection>,
    ) -> Result<(), EngineError> {
        self.commit_from(transaction, self.selection.clone(), after)
    }

    fn commit_from(
        &mut self,
        transaction: Transaction,
        before: Option<BrowserSelection>,
        after: Option<BrowserSelection>,
    ) -> Result<(), EngineError> {
        self.commit_with_image(
            transaction,
            before,
            after,
            self.selected_image.clone(),
            self.selected_image.clone(),
        )
    }

    fn commit_with_image(
        &mut self,
        transaction: Transaction,
        before: Option<BrowserSelection>,
        after: Option<BrowserSelection>,
        image_before: Option<Id>,
        image_after: Option<Id>,
    ) -> Result<(), EngineError> {
        let candidate = transaction_document(&transaction)?;
        self.check_candidate(&candidate)?;
        let outcome = self.session.commit(transaction)?;
        if outcome.changed() {
            self.layout_state.invalidate();
            self.undo_cursor.push(CursorHistory {
                before,
                after: after.clone(),
                image_before,
                image_after: image_after.clone(),
            });
            if self.undo_cursor.len() > HISTORY_LIMIT {
                self.undo_cursor.remove(0);
            }
            self.redo_cursor.clear();
        }
        self.selection = after;
        self.selected_image = image_after;
        Ok(())
    }

    fn repair_cursor(&mut self) {
        if self
            .selected_image
            .as_ref()
            .is_some_and(|id| self.image(id).is_none())
        {
            self.selected_image = None;
        }
        self.selection = self
            .selection
            .as_ref()
            .and_then(|selection| self.normalize_selection(selection).ok())
            .or_else(|| first_cursor(self.session.document()));
    }

    fn make_view(&self) -> View {
        let document = self.session.document();
        let paragraphs = view::paragraphs(document);
        let format = self.format_summary(&paragraphs);
        // Count the authoritative, committed document text, not the UI's
        // formatted paragraph projection. This keeps status snapshots tied to
        // the exact core document (including edits spanning formatting runs).
        let word_count = document
            .paragraphs()
            .map(|paragraph| {
                TextCoordinates::new(paragraph)
                    .text()
                    .unicode_words()
                    .count()
            })
            .sum();
        View {
            session_id: self.session_id.clone(),
            revision: self.revision(),
            document_id: document.id().as_str().into(),
            title: document.metadata().title.clone(),
            styles: document
                .styles()
                .iter()
                .map(|style| wire::StyleSummary {
                    id: style.id.as_str().into(),
                    name: style.name.clone(),
                })
                .collect(),
            paragraphs,
            body_blocks: document
                .sections()
                .iter()
                .flat_map(|section| {
                    section.blocks().iter().map(|block| {
                        let (kind, id) = match block {
                            Block::Paragraph(paragraph) => ("paragraph", paragraph.id()),
                            Block::Table(table) => ("table", table.id()),
                            Block::Image(image) => ("image", image.id()),
                        };
                        wire::BodyBlockView {
                            kind,
                            id: id.as_str().into(),
                            section_id: section.id().as_str().into(),
                        }
                    })
                })
                .collect(),
            selection: self.selection.clone(),
            images: self.image_views(),
            selected_image_id: self.selected_image.as_ref().map(|id| id.as_str().into()),
            table: self
                .selection
                .as_ref()
                .filter(|_| self.selected_image.is_none())
                .and_then(|selection| {
                    let anchor = view::table_cell(document, &selection.anchor.paragraph_id)?;
                    let focus = view::table_cell(document, &selection.focus.paragraph_id)?;
                    (anchor.cell_id == focus.cell_id).then_some(focus)
                }),
            page_setup: self.page_setup_view(),
            format,
            can_undo: self.session.can_undo(),
            can_redo: self.session.can_redo(),
            word_count,
            original_source: self
                .original_source
                .as_ref()
                .map(|source| wire::OriginalSourceView {
                    format: source.format().into(),
                    filename: source.filename().into(),
                    byte_length: source.bytes().len(),
                }),
            import_warnings: self.import_warnings.clone(),
        }
    }


    /// Effective page settings for the focused section (or the first section).
    /// This is presentation only; the document retains explicit/absent overrides.
    fn page_setup_view(&self) -> Option<wire::PageSetupView> {
        let document = self.session.document();
        let section = self.selected_image.as_ref().and_then(|id| {
            document.sections().iter().find(|section| section.blocks().iter().any(|block| {
                matches!(block, Block::Image(image) if image.id() == id)
            }))
        }).or_else(|| self.selection.as_ref().and_then(|selection| {
            document.sections().iter().find(|section| {
                section.paragraphs().any(|paragraph| paragraph.id().as_str() == selection.focus.paragraph_id)
            })
        })).or_else(|| document.sections().first())?;
        let defaults = office_layout::LayoutOptions::default();
        Some(wire::PageSetupView {
            section_id: section.id().as_str().into(),
            section_index: document.sections().iter()
                .position(|candidate| candidate.id() == section.id()).unwrap_or(0),
            section_count: document.sections().len(),
            page_size: section.properties().and_then(|value| value.page_size).unwrap_or(defaults.page_size),
            page_margins: section.properties().and_then(|value| value.page_margins).unwrap_or(defaults.page_margins),
        })
    }

    fn check_candidate(&self, document: &Document) -> Result<(), EngineError> {
        let bytes = checked_document(document)?;
        if bytes
            .len()
            .checked_add(self.source_overhead)
            .is_none_or(|length| length > MAX_DOCUMENT_BYTES)
        {
            return Err(limit_error());
        }
        Ok(())
    }
}

fn checked_file(
    document: &Document,
    source: Option<OriginalSource>,
) -> Result<(Vec<u8>, usize), EngineError> {
    let document_length = checked_document(document)?.len();
    let file = match source {
        Some(source) => WriterFile::with_original_source(document.clone(), source)?,
        None => WriterFile::new(document.clone()),
    };
    let bytes = serialize_writer_file(&file)?;
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(limit_error());
    }
    let overhead = bytes
        .len()
        .checked_sub(document_length)
        .ok_or_else(|| EngineError::new("invalid_file", "Native file sizing is inconsistent."))?;
    Ok((bytes, overhead))
}

fn validate_source(
    source: &OriginalSource,
    document_id: Id,
) -> Result<wire::CompatibilityReportView, EngineError> {
    if source.format() != "docx" {
        return formats::validate_source(source, document_id);
    }
    if !source
        .media_type()
        .eq_ignore_ascii_case(office_docx::DOCX_MEDIA_TYPE)
    {
        return Err(EngineError::new(
            "unsupported_source",
            "This Writer file retains an unsupported source format.",
        ));
    }
    match office_docx::import_docx(source.bytes(), document_id)? {
        DocxImportOutcome::Editable(imported) => Ok(normalization_warnings(imported.report())),
        DocxImportOutcome::Preserved { report, .. } => Err(unsupported_docx(report)),
    }
}

fn normalization_warnings(report: &CompatibilityReport) -> wire::CompatibilityReportView {
    wire::CompatibilityReportView {
        omitted_diagnostics: report.omitted_normalizations,
        diagnostics: report
            .normalizations
            .iter()
            .map(|item| wire::CompatibilityDiagnosticView {
                kind: item.kind.code().into(),
                part: item.part.clone(),
                path: item.path.clone(),
                message: item.message.clone(),
            })
            .collect(),
    }
}

fn unsupported_docx(report: CompatibilityReport) -> EngineError {
    EngineError {
        code: "unsupported_docx".into(),
        message: "This DOCX contains unsupported content. Its original is preserved read-only; no editable copy was opened.".into(),
        compatibility: Some(wire::CompatibilityReportView {
            omitted_diagnostics: report.omitted_diagnostics,
            diagnostics: report.diagnostics.into_iter().map(|item| wire::CompatibilityDiagnosticView {
                kind: item.kind.code().into(),
                part: item.part,
                path: item.path,
                message: item.message,
            }).collect(),
        }),
    }
}

fn transaction_document(transaction: &Transaction) -> Result<Document, EngineError> {
    let draft = transaction.draft();
    Ok(Document::from_parts(
        transaction.document_id().clone(),
        draft.metadata.clone(),
        draft.sections.clone(),
        draft.styles.clone(),
    )?)
}

fn first_cursor(document: &Document) -> Option<BrowserSelection> {
    let paragraph = document.paragraphs().next()?;
    let point = Point {
        paragraph_id: paragraph.id().as_str().into(),
        utf16: 0,
    };
    Some(BrowserSelection {
        anchor: point.clone(),
        focus: point,
    })
}

fn checked_document(document: &Document) -> Result<Vec<u8>, EngineError> {
    let mut units = 0usize;
    let mut paragraphs = 0usize;
    let mut runs = 0usize;
    if document.sections().len() > 2_000 || document.styles().len() > 128 {
        return Err(limit_error());
    }
    let styles: std::collections::HashMap<_, _> = document
        .styles()
        .iter()
        .map(|style| (style.id.as_str(), style))
        .collect();
    for style in document.styles() {
        let mut depth = 1;
        let mut parent = style.based_on.as_ref();
        while let Some(id) = parent {
            depth += 1;
            if depth > 32 {
                return Err(limit_error());
            }
            parent = styles[id.as_str()].based_on.as_ref();
        }
    }
    for section in document.sections() {
        for paragraph in section.paragraphs() {
            paragraphs += 1;
            runs = runs.saturating_add(paragraph.runs().len());
            for run in paragraph.runs() {
                units = units.saturating_add(run.text.encode_utf16().count());
            }
        }
    }
    if paragraphs > MAX_PARAGRAPHS || units > MAX_UTF16 || runs > 20_000 {
        return Err(limit_error());
    }
    let bytes = serialize_document(document)?;
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(limit_error());
    }
    Ok(bytes)
}

fn limit_error() -> EngineError {
    EngineError::new(
        "document_too_large",
        "This browser prototype supports at most 5 MiB, 200,000 UTF-16 units, 2,000 paragraphs/sections, 20,000 runs, 128 styles and 32 inheritance levels. The active document is unchanged.",
    )
}

fn character_patch(
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    font_family: Option<String>,
    font_size_pt: Option<f64>,
) -> Result<CharacterFormat, EngineError> {
    let font_size = font_size_pt
        .map(|value| {
            if !value.is_finite()
                || !(0.5..=400.).contains(&value)
                || value * 2. != (value * 2.).round()
            {
                return Err(EngineError::new(
                    "invalid_format",
                    "Font size must be a half-point increment from 0.5 to 400 points.",
                ));
            }
            Ok(office_core::HalfPoints((value * 2.) as u32))
        })
        .transpose()?;
    if font_family
        .as_ref()
        .is_some_and(|family| family.trim().is_empty() || family.len() > 128)
    {
        return Err(EngineError::new(
            "invalid_format",
            "Font family must contain text and be at most 128 bytes.",
        ));
    }
    Ok(CharacterFormat {
        bold,
        italic,
        underline: underline.map(|on| {
            if on {
                office_core::Underline::Single
            } else {
                office_core::Underline::None
            }
        }),
        font_family,
        font_size,
        color: None,
        hyperlink: None,
    })
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::BrowserEngine;
    use wasm_bindgen::prelude::*;

    /// Returns the exact Rust-owned catalog required before browser typography.
    #[wasm_bindgen]
    pub fn font_catalog_manifest() -> String {
        super::fonts::manifest()
    }

    /// Returns readiness and missing IDs without changing any font or document.
    #[wasm_bindgen]
    pub fn font_catalog_status() -> String {
        super::fonts::status()
    }

    /// Stages one pinned font after Rust verifies its ID, length, digest and data.
    #[wasm_bindgen]
    pub fn register_font(font_id: &str, bytes: &[u8]) -> Result<String, JsValue> {
        super::fonts::register(font_id, bytes).map_err(js_error)
    }

    /// Publishes the complete immutable catalog shared by layout and PDF export.
    #[wasm_bindgen]
    pub fn finalize_font_catalog() -> Result<String, JsValue> {
        super::fonts::finalize().map_err(js_error)
    }

    /// Minimal owning JavaScript handle to a Rust Writer session.
    #[wasm_bindgen]
    pub struct WriterEngine {
        inner: BrowserEngine,
    }

    #[wasm_bindgen]
    impl WriterEngine {
        /// Creates a session under a caller-generated unique namespace.
        #[wasm_bindgen(constructor)]
        pub fn new(namespace: &str) -> Result<WriterEngine, JsValue> {
            Ok(Self {
                inner: BrowserEngine::new(namespace).map_err(js_error)?,
            })
        }
        /// Returns immutable presentation JSON.
        pub fn snapshot(&self) -> String {
            self.inner.snapshot()
        }
        /// Computes revision-bound Rust page/glyph geometry without editing content.
        pub fn layout_snapshot(
            &mut self,
            options: &str,
            expected_revision: Option<String>,
        ) -> Result<String, JsValue> {
            self.inner
                .layout_snapshot(options, expected_revision.as_deref())
                .map_err(js_error)
        }
        /// Returns revision-bound native caret and selection rectangles.
        pub fn selection_geometry(
            &mut self,
            options: &str,
            expected_revision: Option<String>,
        ) -> Result<String, JsValue> {
            self.inner
                .selection_geometry(options, expected_revision.as_deref())
                .map_err(js_error)
        }
        /// Applies native pointer hit-testing or visual keyboard navigation.
        pub fn layout_action(
            &mut self,
            options: &str,
            action: &str,
            expected_revision: Option<String>,
        ) -> Result<String, JsValue> {
            self.inner
                .layout_action(options, action, expected_revision.as_deref())
                .map_err(js_error)
        }
        /// Finds bounded literal matches using Rust core grapheme boundaries.
        pub fn find_text(&self, query: &str) -> Result<String, JsValue> {
            self.inner.find_text(query).map_err(js_error)
        }
        /// Copies exact selected plain text using the Rust paragraph policy.
        pub fn selected_text(&self) -> Result<String, JsValue> {
            self.inner.selected_text().map_err(js_error)
        }
        /// Executes one structured command.
        pub fn dispatch(&mut self, command: &str) -> String {
            self.inner.dispatch(command)
        }
        /// Copies the native document into owned JavaScript bytes.
        pub fn export_document(&self) -> Result<Vec<u8>, JsValue> {
            self.inner.export_document().map_err(js_error)
        }
        /// Inserts validated local raster bytes as one revision-guarded edit.
        pub fn insert_image(&mut self, bytes: &[u8], expected_revision: Option<String>) -> String {
            self.inner.insert_image(bytes, expected_revision.as_deref())
        }
        /// Reads canonical PNG presentation bytes without publishing a document edit.
        pub fn image_png(
            &self,
            image_id: &str,
            expected_revision: Option<String>,
        ) -> Result<Vec<u8>, JsValue> {
            self.inner
                .image_png(image_id, expected_revision.as_deref())
                .map_err(js_error)
        }
        /// Imports supported DOCX into a detached candidate before browser preservation.
        pub fn import_docx(&mut self, bytes: &[u8], filename: &str) -> String {
            self.inner.import_docx(bytes, filename)
        }
        /// Regenerates DOCX from current supported model values.
        pub fn export_docx(&self) -> Result<Vec<u8>, JsValue> {
            self.inner.export_docx().map_err(js_error)
        }
        /// Imports a selected local format; PDF is deliberately export-only.
        pub fn import_format(&mut self, bytes: &[u8], format: &str, filename: &str) -> String {
            self.inner.import_format(bytes, format, filename)
        }
        /// Reports conversion losses for a revision-guarded Save As operation.
        pub fn export_assessment(
            &self,
            format: &str,
            expected_revision: Option<String>,
        ) -> Result<String, JsValue> {
            self.inner
                .export_assessment(format, expected_revision.as_deref())
                .map_err(js_error)
        }
        /// Saves the selected format only after its losses are acknowledged.
        pub fn export_format(
            &self,
            format: &str,
            acknowledge_loss: bool,
            expected_revision: Option<String>,
        ) -> Result<Vec<u8>, JsValue> {
            self.inner
                .export_format(format, acknowledge_loss, expected_revision.as_deref())
                .map_err(js_error)
        }
        /// Copies the exact retained original, independently of edited content.
        pub fn export_original(&self) -> Result<Vec<u8>, JsValue> {
            self.inner.export_original().map_err(js_error)
        }
        /// Validates native bytes before replacing the session.
        pub fn open_document(&mut self, bytes: &[u8], expected_id: Option<String>) -> String {
            match expected_id {
                Some(id) => self.inner.open_document_verified(bytes, &id),
                None => self.inner.open_document(bytes),
            }
        }
    }

    fn js_error(error: super::EngineError) -> JsValue {
        JsValue::from_str(&serde_json::to_string(&error).expect("serializable error"))
    }
}
