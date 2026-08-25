//! Filesystem state fingerprints used to reject stale prepared plans.

use std::{fs, io, path::Path, time::UNIX_EPOCH};

/// Relevant observed filesystem state for one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PathFingerprint {
    Missing,
    Present {
        kind: FileKind,
        len: u64,
        modified_unix_nanos: Option<u128>,
        identity: Option<FileIdentity>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileKind {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FileIdentity {
    device: u64,
    inode: u64,
}

impl PathFingerprint {
    pub(crate) fn capture(path: &Path) -> io::Result<Self> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::Missing),
            Err(error) => return Err(error),
        };
        let file_type = metadata.file_type();
        let kind = if file_type.is_file() {
            FileKind::File
        } else if file_type.is_dir() {
            FileKind::Directory
        } else if file_type.is_symlink() {
            FileKind::Symlink
        } else {
            FileKind::Other
        };
        let modified_unix_nanos = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos());
        #[cfg(unix)]
        let identity = {
            use std::os::unix::fs::MetadataExt;
            Some(FileIdentity {
                device: metadata.dev(),
                inode: metadata.ino(),
            })
        };
        #[cfg(not(unix))]
        let identity = None;
        Ok(Self::Present {
            kind,
            len: metadata.len(),
            modified_unix_nanos,
            identity,
        })
    }
}
