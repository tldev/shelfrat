use std::path::PathBuf;
use std::sync::atomic::Ordering;

use sea_orm::DatabaseConnection;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::metaqueue::MetaQueue;
use crate::repositories::{config_repo, metadata_repo};
use crate::scanner;

/// Run a full library scan from an API request.
pub async fn full_scan(
    db: &DatabaseConnection,
    pool: &SqlitePool,
    library_path: Option<PathBuf>,
    meta_queue: &Option<MetaQueue>,
) -> Result<Value, AppError> {
    let library_path =
        library_path.ok_or_else(|| AppError::BadRequest("no library path configured".into()))?;

    tracing::info!("starting full library scan: {}", library_path.display());

    scanner::SCAN_IN_PROGRESS.store(true, Ordering::SeqCst);

    let lib_path_clone = library_path.clone();
    let files = tokio::task::spawn_blocking(move || scanner::scan_directory(&lib_path_clone))
        .await
        .map_err(|e| {
            scanner::SCAN_IN_PROGRESS.store(false, Ordering::SeqCst);
            AppError::Internal(format!("scan task failed: {e}"))
        })?
        .map_err(|e| {
            scanner::SCAN_IN_PROGRESS.store(false, Ordering::SeqCst);
            AppError::from(e)
        })?;

    let result = metadata_repo::import_scanned_files(pool, &files, true)
        .await
        .map_err(|e| {
            scanner::SCAN_IN_PROGRESS.store(false, Ordering::SeqCst);
            AppError::from(e)
        })?;

    scanner::SCAN_IN_PROGRESS.store(false, Ordering::SeqCst);

    tracing::info!(
        "scan complete: {} scanned, {} imported, {} updated",
        result.total_scanned,
        result.imported,
        result.updated
    );

    let queued = queue_metadata_enrichment(db, meta_queue).await;

    Ok(json!({
        "imported": result.imported,
        "updated": result.updated,
        "skipped": result.skipped,
        "total_scanned": result.total_scanned,
        "metadata_queued": queued,
    }))
}

/// Run a library scan from the job scheduler. Returns Ok(json_string) or Err(error_string).
pub async fn run_library_scan_job(
    db: &DatabaseConnection,
    pool: &SqlitePool,
    library_path: &Option<PathBuf>,
    meta_queue: &Option<MetaQueue>,
) -> Result<String, String> {
    let library_path = resolve_library_path(db, library_path)
        .await
        .ok_or_else(|| "no library path configured".to_string())?;

    if scanner::SCAN_IN_PROGRESS
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("scan already in progress".to_string());
    }

    tracing::info!("job: starting library scan: {}", library_path.display());

    let lib_path_clone = library_path.clone();
    let files =
        match tokio::task::spawn_blocking(move || scanner::scan_directory(&lib_path_clone)).await {
            Ok(Ok(files)) => files,
            Ok(Err(e)) => {
                scanner::SCAN_IN_PROGRESS.store(false, Ordering::SeqCst);
                return Err(format!("scan error: {e}"));
            }
            Err(e) => {
                scanner::SCAN_IN_PROGRESS.store(false, Ordering::SeqCst);
                return Err(format!("scan task panicked: {e}"));
            }
        };

    let result = match metadata_repo::import_scanned_files(pool, &files, true).await {
        Ok(r) => r,
        Err(e) => {
            scanner::SCAN_IN_PROGRESS.store(false, Ordering::SeqCst);
            return Err(format!("import error: {e}"));
        }
    };

    scanner::SCAN_IN_PROGRESS.store(false, Ordering::SeqCst);

    tracing::info!(
        "job: scan complete: {} scanned, {} imported, {} updated",
        result.total_scanned,
        result.imported,
        result.updated
    );

    let queued = queue_metadata_enrichment(db, meta_queue).await;

    let summary = json!({
        "imported": result.imported,
        "updated": result.updated,
        "skipped": result.skipped,
        "total_scanned": result.total_scanned,
        "metadata_queued": queued,
    });

    Ok(summary.to_string())
}

/// Queue books that need metadata enrichment.
async fn queue_metadata_enrichment(
    db: &DatabaseConnection,
    meta_queue: &Option<MetaQueue>,
) -> usize {
    let retry_hours: i64 = config_repo::get(db, "metadata_retry_hours")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(24);

    let provider_count = crate::metaqueue::PROVIDERS.len() as i64;

    let books_needing = metadata_repo::books_needing_metadata(db, retry_hours, provider_count)
        .await
        .unwrap_or_default();

    let queued = books_needing.len();
    if queued > 0 {
        if let Some(ref queue) = meta_queue {
            queue.enqueue_many(&books_needing);
            tracing::info!("queued {queued} books for background metadata processing");
        } else {
            tracing::warn!("no metadata queue available, skipping {queued} books");
        }
    }
    queued
}

/// Resolve library path: env var > app_config > startup value.
async fn resolve_library_path(
    db: &DatabaseConnection,
    startup_path: &Option<PathBuf>,
) -> Option<PathBuf> {
    if let Ok(env_path) = std::env::var("LIBRARY_PATH") {
        if !env_path.is_empty() {
            return Some(PathBuf::from(env_path));
        }
    }

    if let Ok(Some(db_path)) = config_repo::get(db, "library_path").await {
        if !db_path.is_empty() {
            return Some(PathBuf::from(db_path));
        }
    }

    startup_path.clone()
}
