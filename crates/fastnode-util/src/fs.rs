use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

/// Read a file to string, replacing invalid UTF-8 sequences with the replacement character.
///
/// # Errors
/// Returns an error if the file cannot be read.
pub fn read_to_string_lossy(path: &Path) -> io::Result<String> {
    let bytes = fs::read(path)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Atomically write bytes to a file by writing to a temp file then renaming.
///
/// This provides crash-safety: the file will either have the old contents or
/// the new contents, never a partial write.
///
/// # Errors
/// Returns an error if the write or rename fails.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));

    // Create temp file in the same directory to ensure same filesystem for rename
    let mut temp_path = parent.to_path_buf();
    temp_path.push(format!(
        ".{}.tmp.{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
        std::process::id()
    ));

    // Write to temp file
    {
        let mut file = File::create(&temp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }

    // Try atomic rename
    match fs::rename(&temp_path, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            // On Windows, rename can fail if target exists. Try copy + remove as fallback.
            if cfg!(windows) {
                fs::copy(&temp_path, path)?;
                let _ = fs::remove_file(&temp_path);
                Ok(())
            } else {
                let _ = fs::remove_file(&temp_path);
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::{tempdir, NamedTempFile};

    #[test]
    fn test_read_to_string_lossy_valid_utf8() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"hello world").unwrap();
        file.flush().unwrap();

        let content = read_to_string_lossy(file.path()).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_read_to_string_lossy_invalid_utf8() {
        let mut file = NamedTempFile::new().unwrap();
        // Write invalid UTF-8: valid start, then invalid continuation
        file.write_all(&[0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x80, 0x81])
            .unwrap();
        file.flush().unwrap();

        let content = read_to_string_lossy(file.path()).unwrap();
        assert!(content.starts_with("Hello"));
        assert!(content.contains('\u{FFFD}')); // replacement character
    }

    #[test]
    fn test_atomic_write() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.txt");

        atomic_write(&path, b"hello").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello");

        // Overwrite
        atomic_write(&path, b"world").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "world");
    }

    #[test]
    fn test_atomic_write_no_temp_left_on_success() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.txt");

        atomic_write(&path, b"content").unwrap();

        // No temp files should remain
        let entries: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].as_ref().unwrap().file_name().to_str().unwrap(),
            "test.txt"
        );
    }
}
