use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use sha2::{Digest, Sha256};

/// Ebook formats we recognise during a library scan.
const EBOOK_EXTENSIONS: &[&str] = &[
    "epub", "pdf", "mobi", "azw", "azw3", "fb2", "cbz", "cbr", "djvu", "txt",
];

/// Global flag: true while a full scan is in progress.
/// The job scheduler checks this to avoid concurrent scans.
pub static SCAN_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// A single discovered file ready for import.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub path: PathBuf,
    pub format: String,
    pub size_bytes: u64,
    pub hash: String,
}

/// Walk `library_path`, returning every recognised ebook file.
///
/// Uses `find` subprocess for directory listing to work around NFS/VirtioFS
/// readdir caching bugs on Docker Desktop for macOS, where `std::fs::read_dir`
/// returns stale partial results from long-running processes.
pub fn scan_directory(library_path: &Path) -> Result<Vec<ScannedFile>, ScanError> {
    if !library_path.is_dir() {
        return Err(ScanError::NotADirectory(library_path.to_path_buf()));
    }

    let ext_args: Vec<String> = EBOOK_EXTENSIONS
        .iter()
        .enumerate()
        .flat_map(|(i, ext)| {
            let mut args = Vec::new();
            if i > 0 {
                args.push("-o".to_string());
            }
            args.push("-iname".to_string());
            args.push(format!("*.{ext}"));
            args
        })
        .collect();

    let mut cmd = std::process::Command::new("find");
    cmd.arg(library_path)
        .arg("-type").arg("f")
        .arg("(");
    for arg in &ext_args {
        cmd.arg(arg);
    }
    cmd.arg(")");

    let output = cmd.output()
        .map_err(|e| ScanError::Io(library_path.to_path_buf(), e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!("find had errors: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let paths: Vec<PathBuf> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect();

    tracing::info!("find discovered {} ebook files in {}", paths.len(), library_path.display());

    let mut files = Vec::new();
    let mut skipped = 0u64;
    for path in paths {
        let metadata = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("skipping {}: {e}", path.display());
                skipped += 1;
                continue;
            }
        };

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();

        let hash = match hash_file(&path) {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!("skipping {}: {e}", path.display());
                skipped += 1;
                continue;
            }
        };

        files.push(ScannedFile {
            path,
            format: ext,
            size_bytes: metadata.len(),
            hash,
        });
    }

    if skipped > 0 {
        tracing::warn!("{skipped} files skipped due to errors");
    }

    Ok(files)
}

/// SHA-256 hash of a file, returned as a lowercase hex string.
pub fn hash_file(path: &Path) -> Result<String, ScanError> {
    use std::io::Read;

    let mut file =
        std::fs::File::open(path).map_err(|e| ScanError::Io(path.to_path_buf(), e))?;

    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| ScanError::Io(path.to_path_buf(), e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportResult {
    pub imported: u64,
    pub updated: u64,
    pub skipped: u64,
    pub total_scanned: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("not a directory: {0}")]
    NotADirectory(PathBuf),

    #[error("I/O error on {0}: {1}")]
    Io(PathBuf, std::io::Error),

    #[error("database error: {0}")]
    Database(String),
}

impl From<ScanError> for crate::error::AppError {
    fn from(e: ScanError) -> Self {
        match e {
            ScanError::NotADirectory(p) => {
                crate::error::AppError::BadRequest(format!("not a directory: {}", p.display()))
            }
            ScanError::Io(p, err) => {
                crate::error::AppError::Internal(format!("I/O error on {}: {err}", p.display()))
            }
            ScanError::Database(msg) => crate::error::AppError::Internal(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // ── hash_file ──────────────────────────────────────────────────

    #[test]
    fn hash_file_known_content() {
        let dir = std::env::temp_dir().join("shelfrat_test_hash");
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("test.txt");
        std::fs::write(&file_path, b"hello world").unwrap();

        let result = hash_file(&file_path).unwrap();

        // SHA-256 of "hello world"
        assert_eq!(
            result,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );

        // Clean up.
        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn hash_file_empty_file() {
        let dir = std::env::temp_dir().join("shelfrat_test_hash_empty");
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("empty.txt");
        std::fs::write(&file_path, b"").unwrap();

        let result = hash_file(&file_path).unwrap();

        // SHA-256 of empty input.
        assert_eq!(
            result,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn hash_file_large_content() {
        // Test with content larger than the 8192-byte buffer.
        let dir = std::env::temp_dir().join("shelfrat_test_hash_large");
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("large.bin");

        let data = vec![0xABu8; 20_000];
        std::fs::write(&file_path, &data).unwrap();

        let result = hash_file(&file_path);
        assert!(result.is_ok());
        // Just verify it's a valid 64-char hex string.
        let hash = result.unwrap();
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn hash_file_nonexistent_returns_error() {
        let result = hash_file(Path::new("/tmp/shelfrat_nonexistent_file_xyz"));
        assert!(result.is_err());
    }

    // ── ScanError → AppError conversion ────────────────────────────

    #[test]
    fn scan_error_not_a_directory_becomes_bad_request() {
        let err = ScanError::NotADirectory(PathBuf::from("/foo/bar"));
        let app_err: crate::error::AppError = err.into();
        match app_err {
            crate::error::AppError::BadRequest(msg) => {
                assert!(msg.contains("not a directory"));
                assert!(msg.contains("/foo/bar"));
            }
            other => panic!("expected BadRequest, got: {other:?}"),
        }
    }

    #[test]
    fn scan_error_io_becomes_internal() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let err = ScanError::Io(PathBuf::from("/secret"), io_err);
        let app_err: crate::error::AppError = err.into();
        match app_err {
            crate::error::AppError::Internal(msg) => {
                assert!(msg.contains("I/O error"));
                assert!(msg.contains("/secret"));
            }
            other => panic!("expected Internal, got: {other:?}"),
        }
    }

    #[test]
    fn scan_error_database_becomes_internal() {
        let err = ScanError::Database("connection lost".to_string());
        let app_err: crate::error::AppError = err.into();
        match app_err {
            crate::error::AppError::Internal(msg) => {
                assert_eq!(msg, "connection lost");
            }
            other => panic!("expected Internal, got: {other:?}"),
        }
    }
}
