use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

use crate::{Document, DocumentMetadata, Id, NamedStyle, Section, ValidationError};

/// Default maximum number of complete document snapshots retained for undo.
pub const DEFAULT_HISTORY_LIMIT: usize = 100;

/// An opaque revision token issued by one live [`DocumentSession`].
///
/// Tokens from independent sessions are unequal even when their document IDs and
/// revision numbers match. Tokens are runtime values, not document schema data.
/// Keep the originating token alongside snapshot-derived positions or selections
/// and call [`DocumentSession::validate_revision`] before reusing them. Raw 3A
/// position types do not carry tokens, and callers must retain that association.
#[derive(Debug, Clone)]
pub struct Revision {
    session: Arc<()>,
    number: u64,
}

impl Revision {
    /// Returns the session-local counter, starting at zero.
    ///
    /// The number alone is not a revision identity; use the complete token for
    /// equality and validation. Loading a document creates a new session lineage.
    #[must_use]
    pub fn number(&self) -> u64 {
        self.number
    }
}

impl PartialEq for Revision {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.session, &other.session) && self.number == other.number
    }
}

impl Eq for Revision {}

/// Mutable construction records staged privately inside a transaction.
///
/// Intermediate drafts may be invalid. Only a complete, validated candidate can
/// be published by [`DocumentSession::commit`]. Editing operations can stage
/// validated candidates without exposing mutable committed state.
/// The session's document ID and schema cannot be changed through this record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentDraft {
    /// Candidate metadata.
    pub metadata: DocumentMetadata,
    /// Candidate sections in document order.
    pub sections: Vec<Section>,
    /// Candidate named styles in their preserved order.
    pub styles: Vec<NamedStyle>,
}

/// A detached draft tied to the session and revision where it began.
///
/// Creating or changing a draft never mutates the committed document. Dropping
/// a transaction aborts it. Commit consumes the transaction on success or failure;
/// retry a conflict by starting from the session's current state. Transactions
/// cannot be cloned or committed twice.
#[derive(Debug)]
#[must_use = "a transaction has no effect unless passed to DocumentSession::commit"]
pub struct Transaction {
    base_revision: Revision,
    document_id: Id,
    draft: DocumentDraft,
    edit_context: Arc<()>,
}

impl Transaction {
    /// Returns the complete originating revision token.
    #[must_use]
    pub fn base_revision(&self) -> &Revision {
        &self.base_revision
    }

    /// Returns the fixed document identity retained by this transaction.
    #[must_use]
    pub fn document_id(&self) -> &Id {
        &self.document_id
    }

    /// Inspects the draft without asserting that it is currently valid.
    #[must_use]
    pub fn draft(&self) -> &DocumentDraft {
        &self.draft
    }

    /// Returns mutable candidate records without exposing committed state.
    ///
    /// Merely requesting mutable access invalidates all editing positions and
    /// selections minted for the previous draft, even if no value is changed.
    pub fn draft_mut(&mut self) -> &mut DocumentDraft {
        self.edit_context = Arc::new(());
        &mut self.draft
    }

    pub(crate) fn edit_context(&self) -> &Arc<()> {
        &self.edit_context
    }

    pub(crate) fn candidate(&self) -> Result<Document, ValidationError> {
        Document::from_parts(
            self.document_id.clone(),
            self.draft.metadata.clone(),
            self.draft.sections.clone(),
            self.draft.styles.clone(),
        )
    }

    pub(crate) fn accept_candidate(&mut self, candidate: Document) {
        debug_assert_eq!(candidate.id(), &self.document_id);
        let draft = DocumentDraft {
            metadata: candidate.metadata().clone(),
            sections: candidate.sections().to_vec(),
            styles: candidate.styles().to_vec(),
        };
        let edit_context = Arc::new(());
        self.draft = draft;
        self.edit_context = edit_context;
    }
}

/// The result of a successful commit, including an unchanged candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitOutcome {
    revision: Revision,
    changed: bool,
}

impl CommitOutcome {
    /// Returns the session revision after commit.
    #[must_use]
    pub fn revision(&self) -> &Revision {
        &self.revision
    }

    /// Reports whether structural document equality changed.
    ///
    /// A no-op commit leaves the revision unchanged. Different run boundaries or
    /// property representations count as changes even if text looks identical.
    #[must_use]
    pub fn changed(&self) -> bool {
        self.changed
    }
}

/// A rejected transaction or originating revision token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionError {
    /// The token or transaction was issued by another session.
    WrongSession,
    /// The originating revision no longer matches the committed state.
    StaleRevision {
        /// The session's current revision number.
        expected: u64,
        /// The older revision number supplied by the caller.
        actual: u64,
    },
    /// The final candidate violates a model invariant.
    InvalidDocument(ValidationError),
    /// A changed document cannot be published because the revision would overflow.
    RevisionExhausted,
}

impl fmt::Display for TransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongSession => f.write_str("transaction or revision belongs to another session"),
            Self::StaleRevision { expected, actual } => {
                write!(f, "stale revision {actual}; current revision is {expected}")
            }
            Self::InvalidDocument(error) => write!(f, "invalid transaction candidate: {error}"),
            Self::RevisionExhausted => f.write_str("session revision counter is exhausted"),
        }
    }
}

impl Error for TransactionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidDocument(error) => Some(error),
            _ => None,
        }
    }
}

/// A rejected undo or redo request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryError {
    /// No earlier committed snapshot is retained.
    NothingToUndo,
    /// No undone snapshot is available to restore.
    NothingToRedo,
    /// The snapshot cannot be published under the transaction invariants.
    Transaction(TransactionError),
}

impl fmt::Display for HistoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NothingToUndo => f.write_str("nothing to undo"),
            Self::NothingToRedo => f.write_str("nothing to redo"),
            Self::Transaction(error) => write!(f, "history restoration rejected: {error}"),
        }
    }
}

impl Error for HistoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transaction(error) => Some(error),
            _ => None,
        }
    }
}

impl From<TransactionError> for HistoryError {
    fn from(error: TransactionError) -> Self {
        Self::Transaction(error)
    }
}

/// Owns a validated committed document and its session-local revision lineage.
///
/// Committed access is immutable. Detached transactions clone the content at
/// begin; commit validates a complete candidate before replacing anything.
/// An exclusive Rust borrow serializes publication; this is in-memory atomicity,
/// not a durable transaction or collaboration protocol. Bounded in-memory history
/// retains one complete snapshot for each changed commit; an entire transaction
/// is one undo step. Undo and redo publish fresh monotonically increasing revisions.
/// The session cannot be cloned. Saving and loading the document preserves model
/// values, not revision tokens, session identity, or undo/redo history.
///
/// ```
/// use office_core::{Document, DocumentSession, Id};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut session = DocumentSession::new(Document::new(Id::new("document")?)?);
/// let origin = session.revision();
/// let mut transaction = session.begin_transaction();
/// transaction.draft_mut().metadata.title = "Research report".into();
/// assert_eq!(session.document().metadata().title, "");
/// let outcome = session.commit(transaction)?;
/// assert!(outcome.changed());
/// assert_eq!(session.document().metadata().title, "Research report");
/// assert!(session.validate_revision(&origin).is_err());
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct DocumentSession {
    document: Document,
    revision: Revision,
    history_limit: usize,
    undo: VecDeque<Document>,
    redo: VecDeque<Document>,
}

impl DocumentSession {
    /// Takes ownership of an already validated document and starts revision zero.
    #[must_use]
    pub fn new(document: Document) -> Self {
        Self::with_history_limit(document, DEFAULT_HISTORY_LIMIT)
    }

    /// Starts a session with a maximum retained undo snapshot count.
    ///
    /// A limit of zero disables history. The limit counts whole document
    /// snapshots rather than bytes; committed document data remains unchanged.
    #[must_use]
    pub fn with_history_limit(document: Document, history_limit: usize) -> Self {
        Self {
            document,
            revision: Revision {
                session: Arc::new(()),
                number: 0,
            },
            history_limit,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
        }
    }

    /// Returns the configured maximum number of retained undo snapshots.
    #[must_use]
    pub fn history_limit(&self) -> usize {
        self.history_limit
    }

    /// Reports whether a committed snapshot is available to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Reports whether an undone snapshot is available to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Discards undo and redo snapshots without changing document or revision.
    pub fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Returns the current immutable committed document.
    #[must_use]
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Captures a token identifying this session's current committed state.
    #[must_use]
    pub fn revision(&self) -> Revision {
        self.revision.clone()
    }

    /// Rejects foreign sessions first, then any earlier revision of this session.
    ///
    /// A real commit conservatively invalidates all previous tokens, including
    /// positions in unchanged paragraphs. No-op commits preserve tokens. This
    /// method checks the token, not an association with a separately supplied
    /// position. Callers must retain the token from that position's actual origin.
    pub fn validate_revision(&self, revision: &Revision) -> Result<(), TransactionError> {
        if !Arc::ptr_eq(&self.revision.session, &revision.session) {
            return Err(TransactionError::WrongSession);
        }
        if self.revision.number != revision.number {
            return Err(TransactionError::StaleRevision {
                expected: self.revision.number,
                actual: revision.number,
            });
        }
        Ok(())
    }

    /// Begins an isolated draft of the current document with its originating token.
    ///
    /// This clones metadata, sections, and styles in O(document size) time and
    /// memory. Multiple drafts may exist; only the first changed commit from a
    /// given revision can succeed. No mutable borrow of this session is retained.
    pub fn begin_transaction(&self) -> Transaction {
        Transaction {
            base_revision: self.revision(),
            document_id: self.document.id().clone(),
            draft: DocumentDraft {
                metadata: self.document.metadata().clone(),
                sections: self.document.sections().to_vec(),
                styles: self.document.styles().to_vec(),
            },
            edit_context: Arc::new(()),
        }
    }

    /// Validates and atomically publishes a transaction, consuming it in all cases.
    ///
    /// Error order is foreign session, stale revision, invalid candidate, then
    /// revision exhaustion for a changed candidate. Every rejection leaves the
    /// committed document and revision untouched. Validation uses the existing
    /// model validator and never repairs or normalizes a draft. The document ID
    /// is retained. Structural no-ops succeed without advancing the revision.
    /// A changed commit records one undo snapshot and clears redo. No-ops and
    /// rejected commits preserve both history stacks. No user code or callbacks
    /// run during publication.
    pub fn commit(&mut self, transaction: Transaction) -> Result<CommitOutcome, TransactionError> {
        self.validate_revision(&transaction.base_revision)?;
        let candidate = Document::from_parts(
            transaction.document_id,
            transaction.draft.metadata,
            transaction.draft.sections,
            transaction.draft.styles,
        )
        .map_err(TransactionError::InvalidDocument)?;
        let changed = candidate != self.document;
        if changed {
            let next = self
                .revision
                .number
                .checked_add(1)
                .ok_or(TransactionError::RevisionExhausted)?;
            // All fallible work precedes publication; both values are owned and
            // no observer can access the session while this exclusive borrow lives.
            if self.history_limit > 0 {
                self.undo.reserve(1);
                let previous = std::mem::replace(&mut self.document, candidate);
                self.undo.push_back(previous);
                if self.undo.len() > self.history_limit {
                    self.undo.pop_front();
                }
            } else {
                self.document = candidate;
            }
            self.redo.clear();
            self.revision.number = next;
        }
        Ok(CommitOutcome {
            revision: self.revision(),
            changed,
        })
    }

    /// Restores the previous committed document as one atomic history step.
    ///
    /// The restored model is validated before publication. Success advances the
    /// revision, so positions and pending drafts from earlier revisions remain
    /// stale even if an older document value is restored. Failure preserves the
    /// current document, revision, and both history stacks.
    pub fn undo(&mut self) -> Result<CommitOutcome, HistoryError> {
        self.restore_history(true)
    }

    /// Restores the most recently undone document under a fresh revision.
    ///
    /// New changed commits clear redo, while no-op commits preserve it. Failure
    /// preserves the current document, revision, and both history stacks.
    pub fn redo(&mut self) -> Result<CommitOutcome, HistoryError> {
        self.restore_history(false)
    }

    fn restore_history(&mut self, undoing: bool) -> Result<CommitOutcome, HistoryError> {
        let target = if undoing {
            self.undo.back().ok_or(HistoryError::NothingToUndo)?
        } else {
            self.redo.back().ok_or(HistoryError::NothingToRedo)?
        };
        target
            .validate()
            .map_err(TransactionError::InvalidDocument)?;
        let next = self
            .revision
            .number
            .checked_add(1)
            .ok_or(TransactionError::RevisionExhausted)?;

        // Reserve destination capacity before touching either stack or state.
        // All stored snapshots came from changed commits or exact restorations.
        if undoing {
            self.redo.reserve(1);
            let target = self.undo.pop_back().expect("undo target was checked");
            let previous = std::mem::replace(&mut self.document, target);
            self.redo.push_back(previous);
        } else {
            self.undo.reserve(1);
            let target = self.redo.pop_back().expect("redo target was checked");
            let previous = std::mem::replace(&mut self.document, target);
            self.undo.push_back(previous);
        }
        self.revision.number = next;
        Ok(CommitOutcome {
            revision: self.revision(),
            changed: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DocumentSession, HistoryError, TransactionError};
    use crate::{Document, Id, NamedStyle, ValidationError};

    #[test]
    fn exhausted_revision_rejects_changes_without_wrapping_but_accepts_no_ops() {
        let mut session =
            DocumentSession::new(Document::new(Id::new("document").unwrap()).unwrap());
        session.revision.number = u64::MAX;
        let original = session.document().clone();
        let token = session.revision();
        let mut transaction = session.begin_transaction();
        transaction.draft_mut().metadata.title = "Cannot publish".into();
        assert_eq!(
            session.commit(transaction),
            Err(TransactionError::RevisionExhausted)
        );
        assert_eq!(session.document(), &original);
        assert_eq!(session.revision(), token);

        let mut invalid = session.begin_transaction();
        invalid
            .draft_mut()
            .styles
            .push(NamedStyle::new(Id::new("style").unwrap(), " "));
        assert!(matches!(
            session.commit(invalid),
            Err(TransactionError::InvalidDocument(
                ValidationError::InvalidStyleName { .. }
            ))
        ));
        assert_eq!(session.document(), &original);
        assert_eq!(session.revision(), token);

        let transaction = session.begin_transaction();
        let outcome = session.commit(transaction).unwrap();
        assert!(!outcome.changed());
        assert_eq!(outcome.revision(), &token);
        assert_eq!(session.revision().number(), u64::MAX);
        assert!(!session.can_undo());
        assert!(!session.can_redo());
    }

    #[test]
    fn history_revision_exhaustion_preserves_both_stacks_and_document() {
        let mut session =
            DocumentSession::new(Document::new(Id::new("document").unwrap()).unwrap());
        for title in ["First", "Second"] {
            let mut transaction = session.begin_transaction();
            transaction.draft_mut().metadata.title = title.into();
            session.commit(transaction).unwrap();
        }
        session.undo().unwrap();
        assert!(session.can_undo());
        assert!(session.can_redo());
        session.revision.number = u64::MAX;
        let original = session.document.clone();
        let revision = session.revision();
        let undo = session.undo.clone();
        let redo = session.redo.clone();

        for undoing in [true, false] {
            assert_eq!(
                session.restore_history(undoing),
                Err(HistoryError::Transaction(
                    TransactionError::RevisionExhausted
                ))
            );
            assert_eq!(session.document, original);
            assert_eq!(session.revision(), revision);
            assert_eq!(session.undo, undo);
            assert_eq!(session.redo, redo);
        }

        let mut transaction = session.begin_transaction();
        transaction.draft_mut().metadata.title = "Cannot create a new branch".into();
        assert_eq!(
            session.commit(transaction),
            Err(TransactionError::RevisionExhausted)
        );
        assert_eq!(session.document, original);
        assert_eq!(session.revision(), revision);
        assert_eq!(session.undo, undo);
        assert_eq!(session.redo, redo);
    }
}
