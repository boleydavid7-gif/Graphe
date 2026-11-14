use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use tempfile::{Builder, NamedTempFile};

use crate::{Document, PersistenceError, deserialize_document, serialize_document};

/// The filesystem operation that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageOperation {
    /// Resolve the supplied native path against the current working directory.
    ResolvePath,
    /// Inspect an existing destination's type and permissions.
    InspectDestination,
    /// Open the destination's parent directory before replacement (Unix).
    OpenDirectory,
    /// Exclusively create a uniquely named temporary file in that directory.
    CreateTemporary,
    /// Copy the existing regular file's ordinary permissions to the temporary file.
    SetPermissions,
    /// Write all serialized bytes to the temporary file.
    WriteTemporary,
    /// Flush the temporary file's writer.
    FlushTemporary,
    /// Synchronize the temporary file's data and metadata.
    SyncTemporary,
    /// Replace the destination pathname with the prepared temporary file.
    ReplaceDestination,
    /// Synchronize the parent directory after replacement (Unix).
    SyncDirectory,
    /// Open a document for reading.
    OpenDocument,
    /// Inspect the opened document's file type.
    InspectDocument,
    /// Read the opened document's complete bytes.
    ReadDocument,
}

/// What a failed save establishes about destination replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacementState {
    /// No replacement operation was attempted; the original file was not modified.
    NotAttempted,
    /// Replacement returned an error; remote or unusual filesystems can be ambiguous.
    Uncertain,
    /// Replacement succeeded, but a later directory synchronization failed.
    Completed,
}

/// An additional failure while explicitly removing a temporary file.
#[derive(Debug)]
pub struct TemporaryCleanupError {
    /// The temporary pathname, retained so callers can report a possible leftover.
    pub path: PathBuf,
    /// The cleanup I/O error, in addition to the original save failure.
    pub source: io::Error,
}

impl fmt::Display for TemporaryCleanupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "could not remove temporary file {}: {}",
            self.path.display(),
            self.source
        )
    }
}

impl Error for TemporaryCleanupError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

/// A native storage failure, preserving path, stage, and underlying error.
#[derive(Debug)]
pub enum StorageError {
    /// Serialization or deserialization failed; no destination replacement occurred.
    Format {
        /// The requested document path.
        path: PathBuf,
        /// The structured native-format error.
        source: PersistenceError,
    },
    /// A filesystem operation failed, possibly with a secondary cleanup failure.
    Io {
        /// The operation that failed.
        operation: StorageOperation,
        /// The relevant native path, without conversion to a UTF-8 string.
        path: PathBuf,
        /// The original operating-system I/O error.
        source: io::Error,
        /// Whether replacement was attempted or completed before this error.
        replacement: ReplacementState,
        /// A cleanup failure; the temporary path may remain on disk.
        cleanup: Option<TemporaryCleanupError>,
    },
}

impl StorageError {
    /// Returns the replacement state so callers can distinguish pre-save failures
    /// from a new file installed with uncertain durability.
    #[must_use]
    pub fn replacement_state(&self) -> ReplacementState {
        match self {
            Self::Format { .. } => ReplacementState::NotAttempted,
            Self::Io { replacement, .. } => *replacement,
        }
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format { path, source } => {
                write!(f, "invalid Writer file {}: {source}", path.display())
            }
            Self::Io {
                operation,
                path,
                source,
                replacement,
                cleanup,
            } => {
                write!(
                    f,
                    "{operation:?} failed for {}: {source} (replacement: {replacement:?})",
                    path.display()
                )?;
                if let Some(cleanup) = cleanup {
                    write!(f, "; {cleanup}")?;
                }
                Ok(())
            }
        }
    }
}

impl Error for StorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Format { source, .. } => Some(source),
            Self::Io { source, .. } => Some(source),
        }
    }
}

fn io_error(operation: StorageOperation, path: &Path, source: io::Error) -> StorageError {
    StorageError::Io {
        operation,
        path: path.into(),
        source,
        replacement: ReplacementState::NotAttempted,
        cleanup: None,
    }
}

fn remove_temporary(temporary: NamedTempFile, mut error: StorageError) -> StorageError {
    let path = temporary.path().to_path_buf();
    if let Err(source) = temporary.close()
        && source.kind() != io::ErrorKind::NotFound
        && let StorageError::Io { cleanup, .. } = &mut error
    {
        *cleanup = Some(TemporaryCleanupError { path, source });
    }
    error
}

/// Saves a complete validated document through a same-directory temporary file.
///
/// Serialization finishes before filesystem changes. The temporary file is
/// written, flushed, and synchronized before replacement. Existing destinations
/// are never deleted first. Symlink, non-regular, and read-only destinations are
/// rejected; existing ordinary permissions are copied to the replacement.
///
/// On Unix the parent directory is synchronized after replacement. A failure
/// then reports [`ReplacementState::Completed`]: the new file is already present.
/// Windows has no equivalent directory-sync step here. Replacement semantics and
/// crash durability depend on the operating system and filesystem; no universal
/// atomicity or power-loss guarantee is made. There is no writer locking, parent
/// directory creation, extension enforcement, or ACL/ownership/xattr preservation.
/// Concurrent saves are last-writer-wins; parent paths must remain stable.
pub fn save_document(document: &Document, path: impl AsRef<Path>) -> Result<(), StorageError> {
    let path = path.as_ref();
    let bytes = serialize_document(document).map_err(|source| StorageError::Format {
        path: path.into(),
        source,
    })?;
    save_bytes(path, &bytes, &mut |_, _| Ok(()))
}

// A private operation hook provides deterministic failure tests without a public
// filesystem abstraction. Production always uses the no-op hook above.
fn save_bytes(
    path: &Path,
    bytes: &[u8],
    hook: &mut impl FnMut(StorageOperation, &mut File) -> io::Result<()>,
) -> Result<(), StorageError> {
    let destination = std::path::absolute(path)
        .map_err(|source| io_error(StorageOperation::ResolvePath, path, source))?;
    if destination.file_name().is_none() {
        return Err(io_error(
            StorageOperation::InspectDestination,
            &destination,
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "destination must name a regular file",
            ),
        ));
    }
    let permissions = match fs::symlink_metadata(&destination) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(io_error(
                    StorageOperation::InspectDestination,
                    &destination,
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "destination must be a regular file, not a symlink",
                    ),
                ));
            }
            if metadata.permissions().readonly() {
                return Err(io_error(
                    StorageOperation::InspectDestination,
                    &destination,
                    io::Error::new(io::ErrorKind::PermissionDenied, "destination is read-only"),
                ));
            }
            Some(metadata.permissions())
        }
        Err(source) if source.kind() == io::ErrorKind::NotFound => None,
        Err(source) => {
            return Err(io_error(
                StorageOperation::InspectDestination,
                &destination,
                source,
            ));
        }
    };
    let parent = destination
        .parent()
        .expect("absolute file path has a parent");
    #[cfg(unix)]
    let mut directory = File::open(parent)
        .map_err(|source| io_error(StorageOperation::OpenDirectory, parent, source))?;
    let mut temporary = Builder::new()
        .prefix(".writer-")
        .suffix(".tmp")
        .tempfile_in(parent)
        .map_err(|source| io_error(StorageOperation::CreateTemporary, parent, source))?;
    for operation in [
        StorageOperation::WriteTemporary,
        StorageOperation::FlushTemporary,
    ] {
        let result = hook(operation, temporary.as_file_mut()).and_then(|()| match operation {
            StorageOperation::WriteTemporary => temporary.write_all(bytes),
            StorageOperation::FlushTemporary => temporary.flush(),
            _ => unreachable!("only temporary preparation operations are listed"),
        });
        if let Err(source) = result {
            let error = io_error(operation, temporary.path(), source);
            return Err(remove_temporary(temporary, error));
        }
    }
    // Keep the newly created temporary file private until its content is complete.
    // Apply permissions after writing (which can clear special Unix mode bits),
    // then synchronize both content and permissions before replacement.
    if let Some(permissions) = permissions
        && let Err(source) = hook(StorageOperation::SetPermissions, temporary.as_file_mut())
            .and_then(|()| temporary.as_file().set_permissions(permissions))
    {
        let error = io_error(StorageOperation::SetPermissions, temporary.path(), source);
        return Err(remove_temporary(temporary, error));
    }
    if let Err(source) = hook(StorageOperation::SyncTemporary, temporary.as_file_mut())
        .and_then(|()| temporary.as_file().sync_all())
    {
        let error = io_error(StorageOperation::SyncTemporary, temporary.path(), source);
        return Err(remove_temporary(temporary, error));
    }
    if let Err(source) = hook(
        StorageOperation::ReplaceDestination,
        temporary.as_file_mut(),
    ) {
        let error = io_error(StorageOperation::ReplaceDestination, &destination, source);
        return Err(remove_temporary(temporary, error));
    }
    match temporary.persist(&destination) {
        Ok(file) => drop(file),
        Err(error) => {
            let mut failure = io_error(
                StorageOperation::ReplaceDestination,
                &destination,
                error.error,
            );
            if let StorageError::Io { replacement, .. } = &mut failure {
                *replacement = ReplacementState::Uncertain;
            }
            return Err(remove_temporary(error.file, failure));
        }
    }
    #[cfg(unix)]
    if let Err(source) =
        hook(StorageOperation::SyncDirectory, &mut directory).and_then(|()| directory.sync_all())
    {
        let mut failure = io_error(StorageOperation::SyncDirectory, parent, source);
        if let StorageError::Io { replacement, .. } = &mut failure {
            *replacement = ReplacementState::Completed;
        }
        return Err(failure);
    }
    Ok(())
}

/// Loads a regular native Writer file, returning only a fully validated document.
///
/// Paths are native [`Path`] values, including Unicode filenames. The full file
/// is read into memory and passed to the filesystem-independent decoder. Opening
/// symlinks follows the operating system's normal read behavior. History,
/// revisions, and editing endpoints are not restored.
pub fn load_document(path: impl AsRef<Path>) -> Result<Document, StorageError> {
    let path = path.as_ref();
    // Refuse static directories/devices/pipes before opening, then recheck the
    // opened handle below. Hostile concurrent path substitution is not prevented.
    let metadata = fs::metadata(path)
        .map_err(|source| io_error(StorageOperation::InspectDocument, path, source))?;
    if !metadata.is_file() {
        return Err(io_error(
            StorageOperation::InspectDocument,
            path,
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "document must be a regular file",
            ),
        ));
    }
    let mut file = File::open(path)
        .map_err(|source| io_error(StorageOperation::OpenDocument, path, source))?;
    let metadata = file
        .metadata()
        .map_err(|source| io_error(StorageOperation::InspectDocument, path, source))?;
    if !metadata.is_file() {
        return Err(io_error(
            StorageOperation::InspectDocument,
            path,
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "document must be a regular file",
            ),
        ));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|source| io_error(StorageOperation::ReadDocument, path, source))?;
    deserialize_document(&bytes).map_err(|source| StorageError::Format {
        path: path.into(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Seek;

    use super::*;
    use crate::{DocumentMetadata, Id};

    fn document(title: &str) -> Document {
        Document::from_parts(
            Id::new("document").unwrap(),
            DocumentMetadata {
                title: title.into(),
            },
            vec![],
            vec![],
        )
        .unwrap()
    }

    fn assert_only_destination(directory: &Path, destination: &Path) {
        let entries: Vec<_> = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(entries, vec![destination.to_path_buf()]);
    }

    #[test]
    fn preparation_and_pre_replace_failures_preserve_old_bytes_and_remove_temporary_files() {
        for failed in [
            StorageOperation::WriteTemporary,
            StorageOperation::FlushTemporary,
            StorageOperation::SetPermissions,
            StorageOperation::SyncTemporary,
            StorageOperation::ReplaceDestination,
        ] {
            let directory = tempfile::tempdir().unwrap();
            let destination = directory.path().join("report.writer");
            let old = document("Original valid document");
            save_document(&old, &destination).unwrap();
            let original = fs::read(&destination).unwrap();
            let new = serialize_document(&document("Replacement")).unwrap();
            let error = save_bytes(&destination, &new, &mut |operation, file| {
                if operation == failed {
                    if operation == StorageOperation::WriteTemporary {
                        // Simulate a partial write rather than only an empty file.
                        file.write_all(b"partial document")?;
                    }
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "injected I/O failure",
                    ));
                }
                Ok(())
            })
            .unwrap_err();
            assert!(matches!(error, StorageError::Io {
                operation, ref source, cleanup: None,
                replacement: ReplacementState::NotAttempted, ..
            } if operation == failed && source.kind() == io::ErrorKind::PermissionDenied));
            assert_eq!(fs::read(&destination).unwrap(), original);
            assert_eq!(load_document(&destination).unwrap(), old);
            assert_only_destination(directory.path(), &destination);
        }
    }

    #[test]
    fn failed_first_save_leaves_no_destination_or_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("new.writer");
        let bytes = serialize_document(&document("New")).unwrap();
        let result = save_bytes(&destination, &bytes, &mut |operation, _| {
            if operation == StorageOperation::SyncTemporary {
                return Err(io::Error::other("injected synchronization failure"));
            }
            Ok(())
        });
        assert!(result.is_err());
        assert!(!destination.exists());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn preparation_finishes_before_replacement_and_directory_sync_follows_it() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("report.writer");
        save_document(&document("Original"), &destination).unwrap();
        let old_bytes = fs::read(&destination).unwrap();
        let bytes = serialize_document(&document("New")).unwrap();
        let mut observed = Vec::new();
        save_bytes(&destination, &bytes, &mut |operation, file| {
            observed.push(operation);
            if operation == StorageOperation::ReplaceDestination {
                assert_eq!(fs::read(&destination).unwrap(), old_bytes);
                file.rewind()?;
                let mut prepared = Vec::new();
                file.read_to_end(&mut prepared)?;
                assert_eq!(prepared, bytes);
                let entries: Vec<_> = fs::read_dir(directory.path())
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .collect();
                assert_eq!(entries.len(), 2);
                assert!(entries.iter().any(|entry| {
                    entry
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(".writer-")
                }));
            }
            if operation == StorageOperation::SyncDirectory {
                assert_eq!(fs::read(&destination).unwrap(), bytes);
            }
            Ok(())
        })
        .unwrap();
        let mut expected = vec![
            StorageOperation::WriteTemporary,
            StorageOperation::FlushTemporary,
            StorageOperation::SetPermissions,
            StorageOperation::SyncTemporary,
            StorageOperation::ReplaceDestination,
        ];
        if cfg!(unix) {
            expected.push(StorageOperation::SyncDirectory);
        }
        assert_eq!(observed, expected);
        assert_only_destination(directory.path(), &destination);
    }

    #[cfg(unix)]
    #[test]
    fn directory_sync_failure_reports_that_replacement_already_completed() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("report.writer");
        save_document(&document("Original"), &destination).unwrap();
        let replacement = document("New valid document");
        let bytes = serialize_document(&replacement).unwrap();
        let error = save_bytes(&destination, &bytes, &mut |operation, _| {
            if operation == StorageOperation::SyncDirectory {
                return Err(io::Error::other("injected directory sync failure"));
            }
            Ok(())
        })
        .unwrap_err();
        assert_eq!(error.replacement_state(), ReplacementState::Completed);
        assert!(matches!(
            error,
            StorageError::Io {
                operation: StorageOperation::SyncDirectory,
                cleanup: None,
                ..
            }
        ));
        assert_eq!(load_document(&destination).unwrap(), replacement);
        assert_only_destination(directory.path(), &destination);
    }

    #[cfg(unix)]
    #[test]
    fn an_actual_persist_error_preserves_old_file_and_reports_uncertain_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("report.writer");
        let original = document("Original");
        save_document(&original, &destination).unwrap();
        let bytes = serialize_document(&document("New")).unwrap();
        let error = save_bytes(&destination, &bytes, &mut |operation, _| {
            if operation == StorageOperation::ReplaceDestination {
                // POSIX permits unlinking an open file. Removing the source
                // deterministically makes persist itself fail without touching
                // the valid destination or depending on process privileges.
                let temporary = fs::read_dir(directory.path())?
                    .map(|entry| entry.unwrap().path())
                    .find(|path| path != &destination)
                    .unwrap();
                fs::remove_file(temporary)?;
            }
            Ok(())
        })
        .unwrap_err();
        assert!(matches!(
            error,
            StorageError::Io {
                operation: StorageOperation::ReplaceDestination,
                replacement: ReplacementState::Uncertain,
                cleanup: None,
                ..
            }
        ));
        assert_eq!(load_document(&destination).unwrap(), original);
        assert_only_destination(directory.path(), &destination);
    }

    #[cfg(unix)]
    #[test]
    fn explicit_cleanup_failure_retains_the_primary_error_and_possible_leftover_path() {
        let directory = tempfile::tempdir().unwrap();
        let temporary = Builder::new().tempfile_in(directory.path()).unwrap();
        let path = temporary.path().to_path_buf();
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        fs::write(path.join("marker"), b"keep").unwrap();
        let failure = io_error(
            StorageOperation::WriteTemporary,
            &path,
            io::Error::new(io::ErrorKind::PermissionDenied, "original failure"),
        );
        let error = remove_temporary(temporary, failure);
        let StorageError::Io {
            operation,
            source,
            cleanup,
            replacement,
            ..
        } = error
        else {
            panic!("expected an I/O error");
        };
        assert_eq!(operation, StorageOperation::WriteTemporary);
        assert_eq!(source.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(replacement, ReplacementState::NotAttempted);
        let cleanup = cleanup.unwrap();
        assert_eq!(cleanup.path, path);
        assert!(cleanup.source.kind() != io::ErrorKind::NotFound);
        assert_eq!(fs::read(path.join("marker")).unwrap(), b"keep");
    }
}
