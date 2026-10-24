//! Crash-safe file writes and tolerant JSON reads.

use std::io::{self, Write};
use std::path::Path;

use serde::de::DeserializeOwned;

/// Writes `bytes` to a temp file in the target's folder, syncs it and renames it over `path`,
/// so a crash leaves either the old file or the new one. Creates the folder when missing.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    std::fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// The JSON in `path`, or `default` when the file is missing, unreadable or not valid for `T`.
pub fn load_json<T: DeserializeOwned>(path: &Path, default: T) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(default)
}

/// The JSON in `path`: `None` when the file is missing, an error when it cannot be read or is not
/// valid for `T`. For the files whose loss the player must hear about (spec §6).
pub fn read_json<T: DeserializeOwned>(path: &Path) -> io::Result<Option<T>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_whole_file_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("sub").join("settings.json");
        atomic_write(&target, b"first version, long").unwrap();
        atomic_write(&target, b"second").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"second");
        let names: Vec<_> = std::fs::read_dir(target.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("settings.json")]);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_keeps_the_old_file_when_the_folder_is_read_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("settings.json");
        atomic_write(&target, b"old").unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
        let result = atomic_write(&target, b"new");
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        // Root ignores the mode bits, so only assert the contract when the write failed.
        if result.is_err() {
            assert_eq!(std::fs::read(&target).unwrap(), b"old");
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn read_json_tells_a_missing_file_from_a_broken_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.json");
        assert!(matches!(read_json::<Vec<u8>>(&path), Ok(None)));
        std::fs::write(&path, b"{not json").unwrap();
        assert!(read_json::<Vec<u8>>(&path).is_err());
        std::fs::write(&path, b"[4, 5]").unwrap();
        assert_eq!(read_json::<Vec<u8>>(&path).unwrap(), Some(vec![4, 5]));
        assert!(read_json::<Vec<u8>>(dir.path()).is_err());
    }

    #[test]
    fn load_json_returns_default_on_missing_or_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.json");
        assert_eq!(load_json(&path, vec![1]), vec![1]);
        std::fs::write(&path, b"{not json").unwrap();
        assert_eq!(load_json(&path, vec![1]), vec![1]);
        std::fs::write(&path, b"{\"a\": 1}").unwrap();
        assert_eq!(load_json(&path, vec![1]), vec![1]);
        std::fs::write(&path, b"[4, 5]").unwrap();
        assert_eq!(load_json(&path, vec![1]), vec![4, 5]);
    }
}
