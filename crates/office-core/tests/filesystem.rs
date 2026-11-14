//! Native filesystem persistence contracts, using isolated temporary directories.

#![cfg(not(target_arch = "wasm32"))]

use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::Path;

use office_core::{
    CharacterFormat, Document, DocumentError, DocumentMetadata, DocumentSession, GraphemeOffset,
    Id, PersistenceError, ReplacementState, StorageError, StorageOperation, TransactionError,
    ValidationError, deserialize_document, load_document, save_document, serialize_document,
};
use serde_json::json;

const SAMPLE: &[u8] = include_bytes!("../../../tests/fixtures/native/writer-v1.writer");

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn empty() -> Document {
    Document::new(id("empty-document")).unwrap()
}

fn sample() -> Document {
    deserialize_document(SAMPLE).unwrap()
}

fn entries(directory: &Path) -> Vec<OsString> {
    let mut names = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    names.sort();
    names
}

#[test]
fn an_empty_document_saves_loads_and_leaves_no_temporary_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("empty.writer");
    let document = empty();

    save_document(&document, &path).unwrap();

    assert_eq!(load_document(&path).unwrap(), document);
    assert_eq!(
        fs::read(&path).unwrap(),
        serialize_document(&document).unwrap()
    );
    assert_eq!(entries(directory.path()), [OsString::from("empty.writer")]);
    directory.close().unwrap();
}

#[test]
fn the_realistic_fixture_preserves_all_supported_values_through_storage() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("research.writer");
    let document = sample();
    assert_eq!(document.sections().len(), 2);
    assert_eq!(document.styles().len(), 2);

    save_document(&document, &path).unwrap();
    let loaded = load_document(&path).unwrap();

    assert_eq!(loaded, document);
    assert_eq!(loaded.validate(), Ok(()));
    assert_eq!(loaded.id(), document.id());
    assert_eq!(loaded.metadata(), document.metadata());
    assert_eq!(loaded.sections(), document.sections());
    assert_eq!(loaded.styles(), document.styles());
    assert_eq!(
        loaded.text_coordinates(&id("intro")).unwrap().text(),
        "Cafe\u{301} research — 👩🏽‍💻 team"
    );
    assert_eq!(
        entries(directory.path()),
        [OsString::from("research.writer")]
    );
}

#[test]
fn unicode_directory_and_file_names_round_trip_without_string_conversion() {
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("研究 — café — 📝");
    fs::create_dir(&parent).unwrap();
    let filename = OsString::from("résumé — e\u{301} — العربية.writer");
    let path = parent.join(&filename);

    save_document(&sample(), &path).unwrap();

    assert_eq!(load_document(&path).unwrap(), sample());
    // A filesystem may normalize a filename's Unicode spelling. The native path
    // must still address the file, and no temporary sibling may remain.
    assert_eq!(entries(&parent).len(), 1);
    let saved = fs::read_dir(&parent)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(load_document(saved).unwrap(), sample());
    directory.close().unwrap();
}

#[test]
fn relative_paths_work_without_changing_the_process_working_directory() {
    let directory = tempfile::TempDir::new_in(".").unwrap();
    let current = std::env::current_dir().unwrap();
    let path = directory
        .path()
        .strip_prefix(&current)
        .unwrap()
        .join("relative.writer");
    assert!(!path.is_absolute());

    save_document(&sample(), &path).unwrap();

    assert_eq!(load_document(&path).unwrap(), sample());
    assert_eq!(
        entries(directory.path()),
        [OsString::from("relative.writer")]
    );
    directory.close().unwrap();
}

#[test]
fn replacing_an_existing_valid_document_produces_complete_deterministic_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("report.writer");
    let original = sample();
    save_document(&original, &path).unwrap();
    let before = fs::read(&path).unwrap();
    let updated = Document::from_parts(
        original.id().clone(),
        DocumentMetadata {
            title: "Revised report — 世界".into(),
        },
        original.sections().to_vec(),
        original.styles().to_vec(),
    )
    .unwrap();

    for _ in 0..3 {
        save_document(&updated, &path).unwrap();
        assert_eq!(
            fs::read(&path).unwrap(),
            serialize_document(&updated).unwrap()
        );
        assert_eq!(load_document(&path).unwrap(), updated);
        assert_eq!(entries(directory.path()), [OsString::from("report.writer")]);
    }
    assert_ne!(fs::read(&path).unwrap(), before);
    assert_eq!(updated.sections(), original.sections());
    assert_eq!(updated.styles(), original.styles());
}

#[test]
fn paths_are_used_as_supplied_without_appending_or_enforcing_the_extension() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("chosen-name.native");

    save_document(&empty(), &path).unwrap();

    assert_eq!(load_document(&path).unwrap(), empty());
    assert!(!path.with_extension("writer").exists());
    assert_eq!(
        entries(directory.path()),
        [OsString::from("chosen-name.native")]
    );
}

#[test]
fn missing_files_return_an_io_error_with_the_original_path() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("missing.writer");

    let error = load_document(&path).unwrap_err();

    assert_eq!(error.replacement_state(), ReplacementState::NotAttempted);
    assert!(error.source().unwrap().is::<io::Error>());
    assert!(matches!(
        error,
        StorageError::Io { path: actual, source, cleanup: None, .. }
            if actual == path && source.kind() == io::ErrorKind::NotFound
    ));
    assert!(entries(directory.path()).is_empty());
}

#[test]
fn saving_does_not_create_missing_parent_directories_or_temporary_leftovers() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join("missing-parent")
        .join("report.writer");

    let error = save_document(&sample(), &path).unwrap_err();

    assert_eq!(error.replacement_state(), ReplacementState::NotAttempted);
    assert!(matches!(
        error,
        StorageError::Io { source, cleanup: None, .. }
            if source.kind() == io::ErrorKind::NotFound
    ));
    assert!(!path.exists());
    assert!(entries(directory.path()).is_empty());
}

#[test]
fn a_directory_destination_is_rejected_without_removing_its_contents() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("directory.writer");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("sentinel"), b"preserve directory contents").unwrap();

    let error = save_document(&sample(), &path).unwrap_err();

    assert!(matches!(
        error,
        StorageError::Io {
            operation: StorageOperation::InspectDestination,
            replacement: ReplacementState::NotAttempted,
            ..
        }
    ));
    assert_eq!(
        fs::read(path.join("sentinel")).unwrap(),
        b"preserve directory contents"
    );
    assert_eq!(
        entries(directory.path()),
        [OsString::from("directory.writer")]
    );
}

#[test]
fn opening_a_directory_returns_a_structured_error_instead_of_a_document() {
    let directory = tempfile::tempdir().unwrap();

    let error = load_document(directory.path()).unwrap_err();

    assert!(matches!(error, StorageError::Io { .. }));
    assert_eq!(error.replacement_state(), ReplacementState::NotAttempted);
    assert!(entries(directory.path()).is_empty());
}

#[test]
fn loading_corrupt_files_preserves_the_path_and_format_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("corrupt.writer");
    for bytes in [b"{".as_slice(), b"not JSON", b"\xff\xfe", b""] {
        fs::write(&path, bytes).unwrap();
        let before = fs::read(&path).unwrap();

        let error = load_document(&path).unwrap_err();

        assert_eq!(error.replacement_state(), ReplacementState::NotAttempted);
        assert!(error.source().unwrap().is::<PersistenceError>());
        assert!(matches!(
            error,
            StorageError::Format { path: actual, .. } if actual == path
        ));
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(
            entries(directory.path()),
            [OsString::from("corrupt.writer")]
        );
    }
}

#[test]
fn unsupported_native_and_model_versions_remain_distinct_storage_errors() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("future.writer");
    fs::write(
        &path,
        br#"{"format":"office-writer","format_version":2,"different_payload":true}"#,
    )
    .unwrap();
    assert!(matches!(
        load_document(&path),
        Err(StorageError::Format {
            source: PersistenceError::UnsupportedFormatVersion {
                found: 2,
                supported: 1
            },
            ..
        })
    ));

    let mut payload: serde_json::Value = serde_json::from_slice(SAMPLE).unwrap();
    payload["document"]["schema_version"] = json!(99);
    fs::write(&path, serde_json::to_vec(&payload).unwrap()).unwrap();
    assert!(matches!(
        load_document(&path),
        Err(StorageError::Format {
            source: PersistenceError::Document(DocumentError::UnsupportedSchemaVersion {
                found: 99,
                supported: 1,
            }),
            ..
        })
    ));
}

#[test]
fn loading_reuses_document_validation_without_repairing_invalid_ids() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("invalid.writer");
    let mut payload: serde_json::Value = serde_json::from_slice(SAMPLE).unwrap();
    let duplicate = payload["document"]["sections"][0]["id"].clone();
    payload["document"]["sections"][1]["id"] = duplicate;
    let bytes = serde_json::to_vec(&payload).unwrap();
    fs::write(&path, &bytes).unwrap();

    assert!(matches!(
        load_document(&path),
        Err(StorageError::Format {
            source: PersistenceError::Document(DocumentError::Validation(
                ValidationError::DuplicateId { .. }
            )),
            ..
        })
    ));
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn a_read_only_destination_is_preserved_and_reports_permission_denied() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("read-only.writer");
    let original = sample();
    save_document(&original, &path).unwrap();
    let original_bytes = fs::read(&path).unwrap();
    let original_permissions = fs::metadata(&path).unwrap().permissions();
    let mut read_only = original_permissions.clone();
    read_only.set_readonly(true);
    fs::set_permissions(&path, read_only).unwrap();

    let result = save_document(&empty(), &path);
    // Restore before assertions so Windows can remove the fixture during cleanup.
    fs::set_permissions(&path, original_permissions).unwrap();

    assert!(matches!(
        result,
        Err(StorageError::Io {
            operation: StorageOperation::InspectDestination,
            source,
            replacement: ReplacementState::NotAttempted,
            cleanup: None,
            ..
        }) if source.kind() == io::ErrorKind::PermissionDenied
    ));
    assert_eq!(fs::read(&path).unwrap(), original_bytes);
    assert_eq!(load_document(&path).unwrap(), original);
    assert_eq!(
        entries(directory.path()),
        [OsString::from("read-only.writer")]
    );
    directory.close().unwrap();
}

#[test]
fn saving_an_edited_document_preserves_session_history_and_active_draft_endpoints() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("edited.writer");
    let original = sample();
    let mut session = DocumentSession::new(original.clone());
    let mut edit = session.begin_transaction();
    let offset = edit.text_coordinates(&id("intro")).unwrap().grapheme_len();
    let position = edit.position(&id("intro"), offset).unwrap();
    edit.insert_text(&position, " — 新しい研究 👨‍👩‍👧‍👦", CharacterFormat::default())
        .unwrap();
    session.commit(edit).unwrap();
    let committed = session.document().clone();
    let revision = session.revision();

    let mut pending = session.begin_transaction();
    let at = pending
        .position(&id("intro"), GraphemeOffset::new(0))
        .unwrap();
    let pending_before = pending.draft().clone();
    save_document(session.document(), &path).unwrap();
    assert_eq!(load_document(&path).unwrap(), committed);
    assert_eq!(session.document(), &committed);
    assert_eq!(session.revision(), revision);
    assert!(session.can_undo());
    assert!(!session.can_redo());
    assert_eq!(pending.draft(), &pending_before);
    assert_eq!(
        pending.insert_text(&at, "", CharacterFormat::default()),
        Ok(false)
    );

    let reopened = DocumentSession::new(load_document(&path).unwrap());
    assert_eq!(reopened.document(), &committed);
    assert_eq!(reopened.revision().number(), 0);
    assert!(!reopened.can_undo());
    assert!(!reopened.can_redo());
    assert_eq!(
        reopened.validate_revision(&revision),
        Err(TransactionError::WrongSession)
    );

    session.undo().unwrap();
    assert_eq!(session.document(), &original);
    session.redo().unwrap();
    assert_eq!(session.document(), &committed);
}

#[test]
fn saving_committed_content_does_not_publish_a_pending_transaction() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("committed.writer");
    let mut session = DocumentSession::new(sample());
    let before = session.document().clone();
    let revision = session.revision();
    let mut pending = session.begin_transaction();
    pending.draft_mut().metadata.title = "Pending title".into();

    save_document(session.document(), &path).unwrap();

    assert_eq!(load_document(&path).unwrap(), before);
    assert_eq!(session.revision(), revision);
    assert!(!session.can_undo());
    assert!(session.commit(pending).unwrap().changed());
    assert_eq!(load_document(&path).unwrap(), before);
    save_document(session.document(), &path).unwrap();
    assert_eq!(
        load_document(&path).unwrap().metadata().title,
        "Pending title"
    );
}

#[cfg(unix)]
#[test]
fn replacement_preserves_existing_unix_permission_bits() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mode.writer");
    save_document(&sample(), &path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

    save_document(&empty(), &path).unwrap();

    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(load_document(&path).unwrap(), empty());
}

#[cfg(unix)]
#[test]
fn saving_rejects_symlink_destinations_without_changing_link_or_target() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("target.writer");
    let link = directory.path().join("link.writer");
    let original = sample();
    save_document(&original, &target).unwrap();
    let before = fs::read(&target).unwrap();
    symlink(&target, &link).unwrap();

    let error = save_document(&empty(), &link).unwrap_err();

    assert!(matches!(
        error,
        StorageError::Io {
            operation: StorageOperation::InspectDestination,
            replacement: ReplacementState::NotAttempted,
            ..
        }
    ));
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&target).unwrap(), before);
    assert_eq!(load_document(&link).unwrap(), original);
    assert_eq!(
        entries(directory.path()),
        [
            OsString::from("link.writer"),
            OsString::from("target.writer")
        ]
    );
}

#[cfg(target_os = "linux")]
#[test]
fn native_paths_need_not_be_representable_as_utf8() {
    use std::os::unix::ffi::OsStringExt;

    let directory = tempfile::tempdir().unwrap();
    let filename = OsString::from_vec(b"report-\xff.writer".to_vec());
    let path = directory.path().join(&filename);
    assert!(path.to_str().is_none());

    save_document(&sample(), &path).unwrap();

    assert_eq!(load_document(&path).unwrap(), sample());
    assert_eq!(entries(directory.path()), [filename]);
}
