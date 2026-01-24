use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

/// Compute the BLAKE3 hash of a file, returning the hex-encoded digest.
///
/// Streams the file content to minimize memory usage.
///
/// # Errors
/// Returns an error if the file cannot be opened or read.
pub fn blake3_file(path: &Path) -> io::Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

/// Compute the BLAKE3 hash of a byte slice, returning the hex-encoded digest.
#[must_use]
pub fn blake3_bytes(data: &[u8]) -> String {
    blake3::hash(data).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_blake3_file() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"hello world").unwrap();
        file.flush().unwrap();

        let hash = blake3_file(file.path()).unwrap();

        // Known BLAKE3 hash of "hello world"
        assert_eq!(
            hash,
            "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24"
        );
    }

    #[test]
    fn test_blake3_bytes() {
        let hash = blake3_bytes(b"hello world");
        assert_eq!(
            hash,
            "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24"
        );
    }

    #[test]
    fn test_blake3_file_not_found() {
        let result = blake3_file(Path::new("/nonexistent/file"));
        assert!(result.is_err());
    }
}
