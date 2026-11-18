use std::error::Error;
use std::fmt;
use std::sync::Arc;

use crate::{
    CharacterFormat, CharacterFormatPatch, Document, DocumentPosition, GraphemeOffset, Id,
    ListKind, ParagraphFormatPatch, ParagraphMetricsPatch, PositionError, Selection,
    TextCoordinates, Transaction, ValidationError, edit_ops,
};

/// A rejected operation on a transaction draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// An endpoint belongs to another transaction or an earlier draft state.
    WrongContext,
    /// A paragraph, document identity, or grapheme offset is invalid.
    Position(PositionError),
    /// The current draft, requested properties, or candidate violates the model.
    InvalidDocument(ValidationError),
    /// Text replacement would merge section boundaries, whose policy is undefined.
    CrossSectionRange,
    /// Text replacement would join independent table cells or cross a table boundary.
    CrossCellRange,
    /// The table command targets a missing entity or invalid structural index.
    InvalidTableOperation {
        /// Stable explanatory category, suitable for interface translation.
        reason: &'static str,
    },
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CrossCellRange => {
                f.write_str("text replacement cannot cross table or cell boundaries")
            }
            Self::InvalidTableOperation { reason } => {
                write!(f, "invalid table operation: {reason}")
            }
            Self::WrongContext => {
                f.write_str("editing endpoint is stale or belongs to another draft")
            }
            Self::Position(error) => error.fmt(f),
            Self::InvalidDocument(error) => write!(f, "invalid editing candidate: {error}"),
            Self::CrossSectionRange => {
                f.write_str("text replacement across section boundaries is unsupported")
            }
        }
    }
}

impl Error for EditError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Position(error) => Some(error),
            Self::InvalidDocument(error) => Some(error),
            _ => None,
        }
    }
}

impl From<PositionError> for EditError {
    fn from(error: PositionError) -> Self {
        Self::Position(error)
    }
}

impl From<ValidationError> for EditError {
    fn from(error: ValidationError) -> Self {
        Self::InvalidDocument(error)
    }
}

/// A checked grapheme position tied to one transaction's current draft state.
///
/// Create it with [`Transaction::position`]. A changed operation or any call to
/// [`Transaction::draft_mut`] invalidates previously issued endpoints. This
/// protects operation inputs from accidental reuse after text or structure changes.
/// A no-op or failed operation preserves them. No rebasing is implicit.
#[derive(Debug, Clone)]
pub struct EditPosition {
    context: Arc<()>,
    position: DocumentPosition,
}

impl EditPosition {
    /// Returns the underlying inspection position, without editing provenance.
    #[must_use]
    pub fn position(&self) -> &DocumentPosition {
        &self.position
    }
}

impl PartialEq for EditPosition {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.context, &other.context) && self.position == other.position
    }
}

impl Eq for EditPosition {}

/// A directed selection tied to one transaction's current draft state.
///
/// Only [`Transaction::selection`] can create this wrapper. Operators validate
/// its provenance before inspecting or changing the candidate document. Raw 3A
/// inspection selections cannot be passed to editing operations.
#[derive(Debug, Clone)]
pub struct EditSelection {
    context: Arc<()>,
    selection: Selection,
}

impl EditSelection {
    /// Returns the underlying directed inspection selection.
    #[must_use]
    pub fn selection(&self) -> &Selection {
        &self.selection
    }
}

impl PartialEq for EditSelection {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.context, &other.context) && self.selection == other.selection
    }
}

impl Eq for EditSelection {}

impl Transaction {
    /// Builds exact text coordinates for a paragraph in the current valid draft.
    ///
    /// This validates and copies the complete candidate. The returned snapshot
    /// must be refreshed after draft changes; it carries no editing provenance.
    pub fn text_coordinates(&self, paragraph_id: &Id) -> Result<TextCoordinates, EditError> {
        Ok(self.candidate()?.text_coordinates(paragraph_id)?)
    }

    /// Creates a grapheme endpoint bound to this transaction's current draft.
    pub fn position(
        &self,
        paragraph_id: &Id,
        offset: GraphemeOffset,
    ) -> Result<EditPosition, EditError> {
        Ok(EditPosition {
            context: Arc::clone(self.edit_context()),
            position: self.candidate()?.position(paragraph_id, offset)?,
        })
    }

    /// Creates a directed selection from two current endpoints of this draft.
    pub fn selection(
        &self,
        anchor: EditPosition,
        focus: EditPosition,
    ) -> Result<EditSelection, EditError> {
        self.check_context(&anchor.context)?;
        self.check_context(&focus.context)?;
        Ok(EditSelection {
            context: Arc::clone(self.edit_context()),
            selection: self
                .candidate()?
                .selection(anchor.position, focus.position)?,
        })
    }

    /// Inserts exact UTF-8 text with explicit direct character properties.
    ///
    /// Formatting is supplied explicitly; the engine does not guess typing
    /// affinity or resolve styles. CR and LF remain literal run text. A changed
    /// operation invalidates all draft endpoints; reacquire them before another
    /// operation. Inserted text can join neighboring graphemes, so no implicit
    /// result cursor is returned. The boolean reports whether the draft changed.
    pub fn insert_text(
        &mut self,
        at: &EditPosition,
        text: &str,
        format: CharacterFormat,
    ) -> Result<bool, EditError> {
        self.check_context(&at.context)?;
        let before = self.candidate()?;
        let selection = before.selection(at.position.clone(), at.position.clone())?;
        let after = edit_ops::replace_text(&before, &selection, text, &format)?;
        Ok(self.finish_operation(before, after))
    }

    /// Deletes a selection using the same structural rules as text replacement.
    pub fn delete_text(&mut self, selection: &EditSelection) -> Result<bool, EditError> {
        self.replace_text(selection, "", CharacterFormat::default())
    }

    /// Replaces a half-open selection with exact text and explicit formatting.
    ///
    /// Within a section, a multi-paragraph replacement keeps the start paragraph's
    /// ID, style, and properties, appends the retained end suffix, and removes the
    /// covered paragraphs. Other sections and properties are preserved. Structural
    /// replacement across sections is rejected. Reverse selections behave like
    /// their ordered range. Failed operations leave the draft and endpoints intact.
    /// Earlier successful staging remains available; drop the transaction to abort
    /// the entire batch, or handle the error before choosing to commit.
    pub fn replace_text(
        &mut self,
        selection: &EditSelection,
        text: &str,
        format: CharacterFormat,
    ) -> Result<bool, EditError> {
        self.check_context(&selection.context)?;
        let before = self.candidate()?;
        let after = edit_ops::replace_text(&before, &selection.selection, text, &format)?;
        Ok(self.finish_operation(before, after))
    }

    /// Replaces a half-open selection with a structural paragraph boundary.
    ///
    /// A collapsed selection splits its paragraph. Within one section, the
    /// start paragraph retains its ID and prefix; the new paragraph receives
    /// the end paragraph's suffix and inherits the start's style and properties.
    /// Retained runs preserve their exact text and character formatting. The
    /// supplied ID must be valid and absent from every original document entity,
    /// including paragraphs removed by the selection. Cross-section ranges are
    /// rejected. Reverse selections behave like their ordered range.
    ///
    /// Success always changes the draft and invalidates its endpoints; reacquire
    /// positions before staging another operation. Failure preserves the draft
    /// and endpoints. This does not change literal CR/LF text insertion behavior.
    pub fn insert_paragraph_break(
        &mut self,
        selection: &EditSelection,
        new_id: Id,
    ) -> Result<bool, EditError> {
        self.check_context(&selection.context)?;
        let before = self.candidate()?;
        let after = edit_ops::insert_paragraph_break(&before, &selection.selection, &new_id)?;
        Ok(self.finish_operation(before, after))
    }

    /// Applies a character property patch to selected text, across sections if needed.
    ///
    /// Collapsed selections do not change text properties or typing state. Runs
    /// are split only when their selected properties actually change. Unselected
    /// content and empty runs are preserved; no run coalescing is performed.
    pub fn apply_character_format(
        &mut self,
        selection: &EditSelection,
        patch: &CharacterFormatPatch,
    ) -> Result<bool, EditError> {
        self.check_context(&selection.context)?;
        let before = self.candidate()?;
        let after = edit_ops::apply_character_format(&before, &selection.selection, patch)?;
        Ok(self.finish_operation(before, after))
    }

    /// Applies a paragraph property patch to paragraphs touched by a selection.
    ///
    /// A collapsed selection targets its paragraph. A noncollapsed range ending
    /// at offset zero excludes that final paragraph. Section boundaries are not
    /// changed, and direct properties are not resolved against named styles.
    pub fn apply_paragraph_format(
        &mut self,
        selection: &EditSelection,
        patch: &ParagraphFormatPatch,
    ) -> Result<bool, EditError> {
        self.check_context(&selection.context)?;
        let before = self.candidate()?;
        let after = edit_ops::apply_paragraph_format(&before, &selection.selection, patch)?;
        Ok(self.finish_operation(before, after))
    }

    /// Applies an optional set of paragraph metrics as a single undoable edit.
    /// An omitted field remains untouched, including each paragraph's
    /// independent indentation values and existing named-style references.
    pub fn apply_paragraph_metrics(
        &mut self,
        selection: &EditSelection,
        patch: &ParagraphMetricsPatch,
    ) -> Result<bool, EditError> {
        self.check_context(&selection.context)?;
        let before = self.candidate()?;
        let after = edit_ops::apply_paragraph_metrics(&before, &selection.selection, patch)?;
        Ok(self.finish_operation(before, after))
    }

    /// Applies or clears the first-level semantic list classification on
    /// selected paragraphs. The result is a single validated, undoable edit.
    pub fn set_paragraph_list(
        &mut self,
        selection: &EditSelection,
        list: Option<ListKind>,
    ) -> Result<bool, EditError> {
        self.check_context(&selection.context)?;
        let before = self.candidate()?;
        let after = edit_ops::set_paragraph_list(&before, &selection.selection, list)?;
        Ok(self.finish_operation(before, after))
    }

    /// Assigns or clears the named style for the same paragraphs as paragraph formatting.
    ///
    /// A supplied style must exist. Clearing the reference does not clear direct
    /// properties, and no effective-style resolution occurs.
    pub fn set_paragraph_style(
        &mut self,
        selection: &EditSelection,
        style: Option<&Id>,
    ) -> Result<bool, EditError> {
        self.check_context(&selection.context)?;
        let before = self.candidate()?;
        let after = edit_ops::set_paragraph_style(&before, &selection.selection, style)?;
        Ok(self.finish_operation(before, after))
    }

    fn check_context(&self, context: &Arc<()>) -> Result<(), EditError> {
        if !Arc::ptr_eq(self.edit_context(), context) {
            return Err(EditError::WrongContext);
        }
        Ok(())
    }

    fn finish_operation(&mut self, before: Document, after: Document) -> bool {
        let changed = before != after;
        if changed {
            self.accept_candidate(after);
        }
        changed
    }
}
