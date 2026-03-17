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
