use std::error::Error;
use std::fmt;
use std::sync::Arc;

use crate::{Document, DocumentMetadata, Id, NamedStyle, Section, ValidationError};

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
/// be published by [`DocumentSession::commit`]. This is a staging boundary for
/// future operations, not a text-editing or formatting-operation API.
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
    pub fn draft_mut(&mut self) -> &mut DocumentDraft {
        &mut self.draft
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

/// Owns a validated committed document and its session-local revision lineage.
///
/// Committed access is immutable. Detached transactions clone the content at
/// begin; commit validates a complete candidate before replacing anything.
/// An exclusive Rust borrow serializes publication; this is in-memory atomicity,
/// not a durable transaction, collaboration protocol, or undo/redo history.
/// The session cannot be cloned. Saving and loading the document preserves model
/// values, not revision tokens or session identity.
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
}

impl DocumentSession {
    /// Takes ownership of an already validated document and starts revision zero.
    #[must_use]
    pub fn new(document: Document) -> Self {
        Self {
            document,
            revision: Revision {
                session: Arc::new(()),
                number: 0,
            },
        }
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
        }
    }

    /// Validates and atomically publishes a transaction, consuming it in all cases.
    ///
    /// Error order is foreign session, stale revision, invalid candidate, then
    /// revision exhaustion for a changed candidate. Every rejection leaves the
    /// committed document and revision untouched. Validation uses the existing
    /// model validator and never repairs or normalizes a draft. The document ID
    /// is retained. Structural no-ops succeed without advancing the revision.
    /// No user code or callbacks run during publication.
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
            self.document = candidate;
            self.revision.number = next;
        }
        Ok(CommitOutcome {
            revision: self.revision(),
            changed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DocumentSession, TransactionError};
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
    }
}
