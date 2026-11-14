//! Atomic publication, isolation, and revision-lineage contracts for transactions.

use std::error::Error;
use std::panic::catch_unwind;

use office_core::{
    Block, CharacterFormat, Document, DocumentDraft, DocumentMetadata, DocumentSession,
    GraphemeOffset, HalfPoints, Id, NamedStyle, Paragraph, ParagraphFormat, Section, TextRun,
    TransactionError, Twips, ValidationError,
};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn document() -> Document {
    Document::from_parts(
        id("document"),
        DocumentMetadata { title: "A".into() },
        vec![Section::new(
            id("section"),
            vec![Block::Paragraph(
                Paragraph::new(
                    id("paragraph"),
                    vec![TextRun::new("e"), TextRun::new("\u{301}😀")],
                )
                .with_style(id("normal")),
            )],
        )],
        vec![NamedStyle::new(id("normal"), "Normal")],
    )
    .unwrap()
}

fn session() -> DocumentSession {
    DocumentSession::new(document())
}

fn commit_title(session: &mut DocumentSession, title: &str) {
    let mut transaction = session.begin_transaction();
    transaction.draft_mut().metadata.title = title.into();
    assert!(session.commit(transaction).unwrap().changed());
}

#[test]
fn a_session_starts_at_revision_zero_without_changing_the_document() {
    let original = document();
    let session = DocumentSession::new(original.clone());
    assert_eq!(session.revision().number(), 0);
    assert_eq!(session.validate_revision(&session.revision()), Ok(()));
    assert_eq!(session.revision(), session.revision().clone());
    assert_eq!(session.document(), &original);
    assert_eq!(session.document().schema_version(), 1);
    assert_eq!(session.document().validate(), Ok(()));
}

#[test]
fn detached_drafts_are_isolated_from_the_session_and_each_other() {
    let session = session();
    let original = session.document().clone();
    let token = session.revision();
    let mut first = session.begin_transaction();
    let second = session.begin_transaction();
    assert_eq!(first.base_revision(), &token);
    assert_eq!(first.document_id(), original.id());
    assert_eq!(first.draft(), second.draft());

    first.draft_mut().metadata.title = "Uncommitted".into();
    first.draft_mut().sections.clear();
    first.draft_mut().styles.clear();
    assert_eq!(second.draft().metadata.title, "A");
    assert_eq!(second.draft().sections.len(), 1);
    assert_eq!(second.draft().styles.len(), 1);
    assert_eq!(session.document(), &original);
    assert_eq!(session.revision(), token);
    drop(first);
    drop(second);
}

#[test]
fn one_commit_publishes_all_candidate_records_and_advances_once() {
    let mut session = session();
    let origin = session.revision();
    let mut transaction = session.begin_transaction();
    transaction.draft_mut().metadata.title = "Research report".into();
    transaction
        .draft_mut()
        .sections
        .push(Section::new(id("appendix"), vec![]));
    transaction
        .draft_mut()
        .styles
        .push(NamedStyle::new(id("caption"), "Caption"));
    assert_eq!(session.document().metadata().title, "A");
    assert_eq!(session.document().sections().len(), 1);
    assert_eq!(session.document().styles().len(), 1);

    let outcome = session.commit(transaction).unwrap();
    assert!(outcome.changed());
    assert_eq!(outcome.revision().number(), 1);
    assert_eq!(outcome.revision(), &session.revision());
    assert_eq!(session.document().id(), &id("document"));
    assert_eq!(session.document().metadata().title, "Research report");
    assert_eq!(session.document().sections().len(), 2);
    assert_eq!(session.document().sections()[1].id(), &id("appendix"));
    assert_eq!(session.document().styles().len(), 2);
    assert_eq!(session.document().styles()[1].id, id("caption"));
    assert_eq!(session.document().validate(), Ok(()));
    assert_eq!(
        session.validate_revision(&origin),
        Err(TransactionError::StaleRevision {
            expected: 1,
            actual: 0,
        })
    );
}

#[test]
fn dropping_a_changed_or_invalid_draft_aborts_without_touching_state() {
    let session = session();
    let original_json = session.document().to_json().unwrap();
    let origin = session.revision();
    let mut transaction = session.begin_transaction();
    transaction.draft_mut().metadata.title = "Abandoned".into();
    transaction.draft_mut().styles.clear();
    drop(transaction);
    assert_eq!(session.document().to_json().unwrap(), original_json);
    assert_eq!(session.revision(), origin);
}

#[test]
fn a_panic_in_caller_staging_drops_the_detached_draft_without_partial_publication() {
    let session = session();
    let original_json = session.document().to_json().unwrap();
    let origin = session.revision();
    let mut transaction = session.begin_transaction();
    let result = catch_unwind(move || {
        transaction.draft_mut().metadata.title = "Before panic".into();
        transaction.draft_mut().styles.clear();
        panic!("caller staging interrupted");
    });
    assert!(result.is_err());
    assert_eq!(session.document().to_json().unwrap(), original_json);
    assert_eq!(session.revision(), origin);
}

#[test]
fn duplicate_ids_reject_the_whole_compound_candidate_without_partial_changes() {
    let mut session = session();
    let original_json = session.document().to_json().unwrap();
    let origin = session.revision();
    let mut transaction = session.begin_transaction();
    transaction.draft_mut().metadata.title = "Must roll back".into();
    transaction
        .draft_mut()
        .styles
        .push(NamedStyle::new(id("caption"), "Caption"));
    transaction
        .draft_mut()
        .sections
        .push(Section::new(id("paragraph"), vec![]));

    let error = session.commit(transaction).unwrap_err();
    assert_eq!(
        error,
        TransactionError::InvalidDocument(ValidationError::DuplicateId {
            id: "paragraph".into(),
        })
    );
    assert!(error.source().unwrap().is::<ValidationError>());
    assert_eq!(session.document().to_json().unwrap(), original_json);
    assert_eq!(session.revision(), origin);

    // A rejected candidate does not poison the session or consume a revision.
    commit_title(&mut session, "Recovery");
    assert_eq!(session.revision().number(), 1);
}

#[test]
fn missing_style_references_and_inheritance_cycles_preserve_committed_state() {
    let mut session = session();
    let original = session.document().clone();
    let origin = session.revision();

    let mut transaction = session.begin_transaction();
    transaction.draft_mut().metadata.title = "Missing style".into();
    transaction.draft_mut().styles.clear();
    assert!(matches!(
        session.commit(transaction),
        Err(TransactionError::InvalidDocument(
            ValidationError::MissingStyle { id, .. }
        )) if id == "normal"
    ));
    assert_eq!(session.document(), &original);
    assert_eq!(session.revision(), origin);

    let mut transaction = session.begin_transaction();
    transaction.draft_mut().metadata.title = "Cyclic style".into();
    transaction.draft_mut().styles[0].based_on = Some(id("normal"));
    assert!(matches!(
        session.commit(transaction),
        Err(TransactionError::InvalidDocument(ValidationError::StyleCycle { id }))
        if id == "normal"
    ));
    assert_eq!(session.document(), &original);
    assert_eq!(session.revision(), origin);
}

#[test]
fn invalid_character_and_paragraph_formatting_reject_all_staged_records() {
    let mut session = session();
    let original_json = session.document().to_json().unwrap();
    let origin = session.revision();
    for paragraph in [
        Paragraph::new(
            id("paragraph"),
            vec![TextRun {
                text: "Candidate".into(),
                format: CharacterFormat {
                    font_size: Some(HalfPoints(0)),
                    ..Default::default()
                },
            }],
        ),
        Paragraph::new(id("paragraph"), vec![TextRun::new("Candidate")]).with_format(
            ParagraphFormat {
                spacing_after: Some(Twips(-1)),
                ..Default::default()
            },
        ),
    ] {
        let mut transaction = session.begin_transaction();
        transaction.draft_mut().metadata.title = "Must not leak".into();
        transaction.draft_mut().sections = vec![Section::new(
            id("section"),
            vec![Block::Paragraph(paragraph)],
        )];
        assert!(matches!(
            session.commit(transaction),
            Err(TransactionError::InvalidDocument(
                ValidationError::InvalidFormatting { .. }
            ))
        ));
        assert_eq!(session.document().to_json().unwrap(), original_json);
        assert_eq!(session.revision(), origin);
    }
}

#[test]
fn temporary_invalid_intermediate_records_can_be_repaired_before_commit() {
    let mut session = session();
    let mut transaction = session.begin_transaction();
    let normal = transaction.draft_mut().styles.remove(0);
    // The staged paragraph now has a dangling style; committed state is valid.
    assert!(transaction.draft().styles.is_empty());
    assert_eq!(session.document().validate(), Ok(()));
    transaction.draft_mut().metadata.title = "Final valid candidate".into();
    transaction.draft_mut().styles.push(normal);

    assert!(session.commit(transaction).unwrap().changed());
    assert_eq!(session.document().metadata().title, "Final valid candidate");
    assert_eq!(session.document().validate(), Ok(()));
    assert_eq!(session.revision().number(), 1);
}

#[test]
fn revision_identity_distinguishes_sessions_with_the_same_document_and_counter() {
    let mut first = session();
    let mut second = session();
    let first_origin = first.revision();
    let second_origin = second.revision();
    assert_eq!(first_origin.number(), second_origin.number());
    assert_eq!(first.document().id(), second.document().id());
    assert_ne!(first_origin, second_origin);
    assert_eq!(
        first.validate_revision(&second_origin),
        Err(TransactionError::WrongSession)
    );

    let mut foreign = second.begin_transaction();
    foreign.draft_mut().metadata.title = "Foreign candidate".into();
    assert_eq!(first.commit(foreign), Err(TransactionError::WrongSession));
    assert_eq!(first.revision(), first_origin);
    assert_eq!(first.document().metadata().title, "A");
    commit_title(&mut second, "Separate change");
    assert_eq!(first.document().metadata().title, "A");
}

#[test]
fn only_the_first_changed_transaction_from_a_revision_can_commit() {
    let mut session = session();
    let mut first = session.begin_transaction();
    let mut second = session.begin_transaction();
    first.draft_mut().metadata.title = "Winner".into();
    second.draft_mut().metadata.title = "Loser".into();
    session.commit(first).unwrap();
    let committed = session.document().to_json().unwrap();
    let token = session.revision();

    assert_eq!(
        session.commit(second),
        Err(TransactionError::StaleRevision {
            expected: 1,
            actual: 0,
        })
    );
    assert_eq!(session.document().to_json().unwrap(), committed);
    assert_eq!(session.revision(), token);
}

#[test]
fn stale_candidates_are_rejected_before_equality_or_validation_can_accept_them() {
    let mut session = session();
    let mut same_as_winner = session.begin_transaction();
    same_as_winner.draft_mut().metadata.title = "Winner".into();
    let mut invalid = session.begin_transaction();
    invalid.draft_mut().styles.clear();
    commit_title(&mut session, "Winner");
    let committed = session.document().clone();
    let token = session.revision();

    for candidate in [same_as_winner, invalid] {
        assert_eq!(
            session.commit(candidate),
            Err(TransactionError::StaleRevision {
                expected: 1,
                actual: 0,
            })
        );
        assert_eq!(session.document(), &committed);
        assert_eq!(session.revision(), token);
    }
}

#[test]
fn foreign_session_is_rejected_before_a_candidates_other_failures() {
    let mut local = session();
    let foreign = session();
    let mut candidate = foreign.begin_transaction();
    candidate.draft_mut().styles.clear();
    commit_title(&mut local, "Local revision one");
    let committed = local.document().clone();
    let token = local.revision();

    assert_eq!(local.commit(candidate), Err(TransactionError::WrongSession));
    assert_eq!(local.document(), &committed);
    assert_eq!(local.revision(), token);
}

#[test]
fn no_ops_preserve_tokens_and_do_not_conflict_with_other_pending_drafts() {
    let mut session = session();
    let origin = session.revision();
    let mut pending = session.begin_transaction();
    pending.draft_mut().metadata.title = "Later change".into();
    let untouched = session.begin_transaction();
    let outcome = session.commit(untouched).unwrap();
    assert!(!outcome.changed());
    assert_eq!(outcome.revision(), &origin);
    assert_eq!(session.validate_revision(&origin), Ok(()));

    let mut restored = session.begin_transaction();
    restored.draft_mut().metadata.title = "Temporary".into();
    restored.draft_mut().metadata.title = "A".into();
    assert!(!session.commit(restored).unwrap().changed());
    assert_eq!(session.revision(), origin);
    assert!(session.commit(pending).unwrap().changed());
    assert_eq!(session.revision().number(), 1);
}

#[test]
fn returning_to_an_earlier_document_value_does_not_make_old_revisions_current() {
    let mut session = session();
    let original = session.document().clone();
    let origin = session.revision();
    let stale = session.begin_transaction();
    commit_title(&mut session, "B");
    commit_title(&mut session, "A");
    assert_eq!(session.document(), &original);
    assert_eq!(session.revision().number(), 2);
    assert_ne!(session.revision(), origin);
    let expected = TransactionError::StaleRevision {
        expected: 2,
        actual: 0,
    };
    assert_eq!(session.validate_revision(&origin), Err(expected.clone()));
    assert_eq!(session.commit(stale), Err(expected));
    assert_eq!(session.document(), &original);
    assert_eq!(session.revision().number(), 2);
}

#[test]
fn real_commits_invalidate_position_context_even_when_paragraph_text_is_unchanged() {
    let mut session = session();
    let origin = session.revision();
    let start = session
        .document()
        .position(&id("paragraph"), GraphemeOffset::new(0))
        .unwrap();
    let end = session
        .document()
        .position(&id("paragraph"), GraphemeOffset::new(2))
        .unwrap();
    let selection = session.document().selection(start, end).unwrap();
    assert_eq!(session.validate_revision(&origin), Ok(()));

    commit_title(&mut session, "Metadata change only");
    assert_eq!(
        session.validate_revision(&origin),
        Err(TransactionError::StaleRevision {
            expected: 1,
            actual: 0,
        })
    );
    // Raw 3A structural checks do not know revision lineage. The original token
    // must be checked before reuse; a fresh token cannot establish that origin.
    assert_eq!(
        session.document().validate_position(selection.anchor()),
        Ok(())
    );
    assert!(selection.ordered_range(session.document()).is_ok());
}

#[test]
fn a_structural_change_counts_even_when_the_logical_text_is_identical() {
    let mut session = session();
    let before = session
        .document()
        .text_coordinates(&id("paragraph"))
        .unwrap();
    let mut candidate = session.begin_transaction();
    candidate.draft_mut().sections = vec![Section::new(
        id("section"),
        vec![Block::Paragraph(
            Paragraph::new(id("paragraph"), vec![TextRun::new("e\u{301}😀")])
                .with_style(id("normal")),
        )],
    )];
    assert!(session.commit(candidate).unwrap().changed());
    assert_eq!(session.revision().number(), 1);
    let after = session
        .document()
        .text_coordinates(&id("paragraph"))
        .unwrap();
    assert_eq!(before.text(), after.text());
    let Block::Paragraph(paragraph) = &session.document().sections()[0].blocks()[0];
    assert_eq!(paragraph.runs().len(), 1);
}

#[test]
fn drafts_can_be_replaced_as_records_without_replacing_document_identity() {
    let mut session = session();
    let mut candidate = session.begin_transaction();
    *candidate.draft_mut() = DocumentDraft {
        metadata: DocumentMetadata {
            title: "Blank candidate".into(),
        },
        sections: vec![],
        styles: vec![],
    };
    assert_eq!(candidate.document_id(), &id("document"));
    session.commit(candidate).unwrap();
    assert_eq!(session.document().id(), &id("document"));
    assert_eq!(session.document().schema_version(), 1);
    assert!(session.document().sections().is_empty());
}

#[test]
fn unicode_fixture_identity_runs_and_serialization_survive_a_metadata_commit() {
    let fixture = include_str!("../../../tests/fixtures/writer-sample-v1.json");
    let original = Document::from_json(fixture).unwrap();
    let mut session = DocumentSession::new(original.clone());
    commit_title(&mut session, "Updated report metadata");
    assert_eq!(session.document().id(), original.id());
    assert_eq!(session.document().sections(), original.sections());
    assert_eq!(session.document().styles(), original.styles());
    let map = session.document().text_coordinates(&id("intro")).unwrap();
    assert_eq!(map.text(), "Cafe\u{301} research — 👩🏽‍💻 team");

    let saved = session.document().to_json().unwrap();
    let payload: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(payload["schema_version"], 1);
    assert!(payload.get("revision").is_none());
    assert!(payload.get("session").is_none());
    assert_eq!(Document::from_json(&saved).unwrap(), *session.document());
}

#[test]
fn reloading_a_document_starts_a_new_lineage_even_if_the_old_session_is_dropped() {
    let mut session = session();
    commit_title(&mut session, "Saved revision one");
    let origin = session.revision();
    let saved = session.document().to_json().unwrap();
    let pending = session.begin_transaction();
    drop(session);

    let mut reopened = DocumentSession::new(Document::from_json(&saved).unwrap());
    assert_eq!(reopened.revision().number(), 0);
    assert_ne!(reopened.revision(), origin);
    assert_eq!(
        reopened.validate_revision(&origin),
        Err(TransactionError::WrongSession)
    );
    assert_eq!(
        reopened.commit(pending),
        Err(TransactionError::WrongSession)
    );
    assert_eq!(reopened.document().metadata().title, "Saved revision one");
    assert_eq!(reopened.revision().number(), 0);
}
