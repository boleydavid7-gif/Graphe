//! Browser-facing session adapter. The independent core owns all committed edits.
//!
//! JSON is a presentation/command transport, never a second document schema.
//! Native files are exchanged through the core's exact `.writer` byte format.
//! Browser UTF-16 endpoints are deliberately snapped outward to Rust extended
//! grapheme boundaries; scalar-invalid surrogate positions are rejected.

mod operations;
mod view;
mod wire;

use office_core::{
    Block, CharacterFormat, Document, DocumentSession, Id, Paragraph, Section, Transaction,
    deserialize_document, serialize_document,
};
use serde::Serialize;
use unicode_segmentation::UnicodeSegmentation;
use wire::{BrowserSelection, Command, Point, View};

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
}

impl EngineError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
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

#[derive(Clone)]
struct CursorHistory {
    before: Option<BrowserSelection>,
    after: Option<BrowserSelection>,
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
    typing: Option<CharacterFormat>,
    undo_cursor: Vec<CursorHistory>,
    redo_cursor: Vec<CursorHistory>,
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
            typing: None,
            undo_cursor: Vec::new(),
            redo_cursor: Vec::new(),
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
        checked_document(self.session.document())
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
        let document = deserialize_document(bytes)?;
        if expected_id.is_some_and(|id| document.id().as_str() != id) {
            return Err(EngineError::new(
                "document_mismatch",
                "Stored document identity does not match its contents.",
            ));
        }
        checked_document(&document)?;
        self.replace_session(document)
    }

    fn response(&self, result: Result<(), EngineError>) -> String {
        #[derive(Serialize)]
        #[serde(untagged)]
        enum Response {
            Success { ok: bool, view: View },
            Failure { ok: bool, error: EngineError },
        }
        let response = match result {
            Ok(()) => Response::Success {
                ok: true,
                view: self.make_view(),
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
                checked_document(&document)?;
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
                if next != self.selection {
                    self.typing = None;
                }
                self.selection = next;
                Ok(())
            }
            Command::SetTitle { title, .. } => {
                let mut transaction = self.session.begin_transaction();
                transaction.draft_mut().metadata.title = title;
                self.commit(transaction, self.selection.clone())
            }
            Command::ReplaceText {
                text, selection, ..
            } => self.replace_text(&text, selection),
            Command::InsertParagraph { selection, .. } => self.replace_text("\n", selection),
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
            Command::Undo { .. } => {
                self.session.undo()?;
                let cursor = self
                    .undo_cursor
                    .pop()
                    .expect("cursor history follows core history");
                self.selection = cursor.before.clone();
                self.redo_cursor.push(cursor);
                self.typing = None;
                self.repair_cursor();
                Ok(())
            }
            Command::Redo { .. } => {
                self.session.redo()?;
                let cursor = self
                    .redo_cursor
                    .pop()
                    .expect("cursor history follows core history");
                self.selection = cursor.after.clone();
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
        self.replace_session(document)
    }

    fn replace_session(&mut self, document: Document) -> Result<(), EngineError> {
        let number = self.next_session.checked_add(1).ok_or_else(|| {
            EngineError::new("identifier_exhausted", "Session identifiers exhausted.")
        })?;
        self.session = DocumentSession::with_history_limit(document, HISTORY_LIMIT);
        self.next_session = number;
        self.session_id = format!("{}:s{number}", self.namespace);
        self.selection = first_cursor(self.session.document());
        self.typing = None;
        self.undo_cursor.clear();
        self.redo_cursor.clear();
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
                        || section.blocks().iter().any(|block| {
                            let Block::Paragraph(paragraph) = block;
                            paragraph.id().as_str() == value
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
        let candidate = transaction_document(&transaction)?;
        checked_document(&candidate)?;
        let outcome = self.session.commit(transaction)?;
        if outcome.changed() {
            self.undo_cursor.push(CursorHistory {
                before,
                after: after.clone(),
            });
            if self.undo_cursor.len() > HISTORY_LIMIT {
                self.undo_cursor.remove(0);
            }
            self.redo_cursor.clear();
        }
        self.selection = after;
        Ok(())
    }

    fn repair_cursor(&mut self) {
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
        let word_count = paragraphs
            .iter()
            .map(|paragraph| {
                paragraph
                    .runs
                    .iter()
                    .map(|run| run.text.as_str())
                    .collect::<String>()
                    .unicode_words()
                    .count()
            })
            .sum();
        View {
            session_id: self.session_id.clone(),
            revision: self.revision(),
            document_id: document.id().as_str().into(),
            title: document.metadata().title.clone(),
            paragraphs,
            selection: self.selection.clone(),
            format,
            can_undo: self.session.can_undo(),
            can_redo: self.session.can_redo(),
            word_count,
        }
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
    let paragraph = document
        .sections()
        .iter()
        .flat_map(|section| section.blocks())
        .next()?;
    let Block::Paragraph(paragraph) = paragraph;
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
        for block in section.blocks() {
            let Block::Paragraph(paragraph) = block;
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
    })
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::BrowserEngine;
    use wasm_bindgen::prelude::*;

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
        /// Executes one structured command.
        pub fn dispatch(&mut self, command: &str) -> String {
            self.inner.dispatch(command)
        }
        /// Copies the native document into owned JavaScript bytes.
        pub fn export_document(&self) -> Result<Vec<u8>, JsValue> {
            self.inner.export_document().map_err(js_error)
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
