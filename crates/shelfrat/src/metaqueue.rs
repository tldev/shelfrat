use std::path::PathBuf;

use sea_orm::DatabaseConnection;
use sqlx::SqlitePool;
use tokio::sync::mpsc;

use crate::repositories::metadata_repo;
use crate::services::metadata_service;

/// A handle for submitting book IDs to the background metadata queue.
#[derive(Clone)]
pub struct MetaQueue {
    tx: mpsc::UnboundedSender<i64>,
}

impl MetaQueue {
    /// Submit a book ID for background metadata extraction + enrichment.
    pub fn enqueue(&self, book_id: i64) {
        if self.tx.send(book_id).is_err() {
            tracing::warn!("metadata queue closed, cannot enqueue book {book_id}");
        }
    }

    /// Submit multiple book IDs at once.
    pub fn enqueue_many(&self, book_ids: &[i64]) {
        for &id in book_ids {
            self.enqueue(id);
        }
    }
}

/// Start the background metadata processing queue.
/// Returns a `MetaQueue` handle for submitting work.
pub fn start(pool: SqlitePool, db: DatabaseConnection, covers_dir: PathBuf) -> MetaQueue {
    let (tx, rx) = mpsc::unbounded_channel::<i64>();

    tokio::spawn(async move {
        worker(rx, pool, db, covers_dir).await;
    });

    tracing::info!("background metadata queue started");
    MetaQueue { tx }
}

/// Background worker: drains book IDs and processes metadata for each.
async fn worker(
    mut rx: mpsc::UnboundedReceiver<i64>,
    pool: SqlitePool,
    db: DatabaseConnection,
    covers_dir: PathBuf,
) {
    while let Some(book_id) = rx.recv().await {
        // Small delay to batch rapid-fire submissions (e.g. after a scan)
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Drain any additional IDs that arrived during the delay
        let mut batch = vec![book_id];
        while let Ok(id) = rx.try_recv() {
            batch.push(id);
        }

        tracing::info!("metaqueue: processing batch of {} books", batch.len());
        for (i, id) in batch.iter().enumerate() {
            tracing::info!("metaqueue: [{}/{}] processing book {id}", i + 1, batch.len());
            process_book(&pool, &db, *id, &covers_dir).await;
        }
        tracing::info!("metaqueue: batch complete");
    }

    tracing::debug!("metadata queue worker exiting");
}

/// Enrichment providers, tried in order.
pub const PROVIDERS: &[&str] = &["openlibrary", "googlebooks"];

/// Process a single book: extract embedded metadata, then try external providers.
async fn process_book(
    pool: &SqlitePool,
    db: &DatabaseConnection,
    book_id: i64,
    covers_dir: &std::path::Path,
) {
    // Extract embedded metadata first (from the file itself)
    let needs_embedded = metadata_repo::needs_embedded_extraction(db, book_id)
        .await
        .unwrap_or(false);

    if needs_embedded {
        if let Err(e) = metadata_service::extract_and_apply(db, pool, book_id, covers_dir).await {
            tracing::warn!("metaqueue: embedded extraction failed for book {book_id}: {e}");
        }
    }

    // Try external providers until the book is fully enriched
    for &provider in PROVIDERS {
        if !metadata_repo::needs_enrichment(db, book_id).await.unwrap_or(false) {
            break;
        }
        if metadata_repo::provider_attempted(db, book_id, provider).await.unwrap_or(false) {
            continue;
        }

        let result = match provider {
            "openlibrary" => {
                metadata_service::enrich_from_openlibrary(db, pool, book_id, Some(covers_dir))
                    .await
            }
            "googlebooks" => {
                metadata_service::enrich_from_googlebooks(db, pool, book_id, Some(covers_dir))
                    .await
            }
            _ => {
                tracing::warn!("metaqueue: unknown provider {provider}");
                continue;
            }
        };

        if let Err(e) = metadata_repo::record_provider_attempt(db, book_id, provider).await {
            tracing::warn!("metaqueue: failed to record {provider} attempt for book {book_id}: {e}");
        }

        match result {
            Ok(true) => tracing::info!("metaqueue: enriched book {book_id} from {provider}"),
            Ok(false) => tracing::info!("metaqueue: no {provider} match for book {book_id}"),
            Err(e) => tracing::warn!("metaqueue: {provider} failed for book {book_id}: {e}"),
        }
    }
}
