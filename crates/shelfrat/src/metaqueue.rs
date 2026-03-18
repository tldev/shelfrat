use std::collections::HashMap;
use std::path::PathBuf;

use sea_orm::DatabaseConnection;
use sqlx::SqlitePool;
use tokio::sync::mpsc;

use crate::provider_error::EnrichError;
use crate::rate_limiter::RateLimiters;
use crate::repositories::{config_repo, metadata_repo};
use crate::services::{metadata_service, provider_service};

const MAX_RATE_LIMITS_PER_BATCH: u32 = 5;

/// Tracks per-provider rate limit hits within a batch and disables providers
/// that exceed the threshold.
struct BatchRateLimitTracker {
    counts: HashMap<String, u32>,
    max_hits: u32,
}

impl BatchRateLimitTracker {
    fn new(max_hits: u32) -> Self {
        Self {
            counts: HashMap::new(),
            max_hits,
        }
    }

    /// Returns true if this provider has been rate-limited too many times.
    fn is_disabled(&self, provider: &str) -> bool {
        self.counts.get(provider).copied().unwrap_or(0) >= self.max_hits
    }

    /// Record a rate limit hit. Returns the new count.
    fn record_rate_limit(&mut self, provider: &str) -> u32 {
        let count = self.counts.entry(provider.to_string()).or_insert(0);
        *count += 1;
        *count
    }
}

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
    let mut limiters = RateLimiters::new();

    while let Some(book_id) = rx.recv().await {
        // Small delay to batch rapid-fire submissions (e.g. after a scan)
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Drain any additional IDs that arrived during the delay
        let mut batch = vec![book_id];
        while let Ok(id) = rx.try_recv() {
            batch.push(id);
        }

        tracing::info!("metaqueue: processing batch of {} books", batch.len());
        let mut rl_tracker = BatchRateLimitTracker::new(MAX_RATE_LIMITS_PER_BATCH);
        for (i, id) in batch.iter().enumerate() {
            tracing::info!(
                "metaqueue: [{}/{}] processing book {id}",
                i + 1,
                batch.len()
            );
            process_book(&pool, &db, *id, &covers_dir, &mut limiters, &mut rl_tracker).await;
        }
        tracing::info!("metaqueue: batch complete");
    }

    tracing::debug!("metadata queue worker exiting");
}

/// Process a single book: extract embedded metadata, then try external providers.
async fn process_book(
    pool: &SqlitePool,
    db: &DatabaseConnection,
    book_id: i64,
    covers_dir: &std::path::Path,
    limiters: &mut RateLimiters,
    rl_tracker: &mut BatchRateLimitTracker,
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
    let providers = provider_service::get_enabled_providers(db).await;
    for provider in &providers {
        if !metadata_repo::needs_enrichment(db, book_id)
            .await
            .unwrap_or(false)
        {
            break;
        }
        if rl_tracker.is_disabled(provider) {
            tracing::debug!("metaqueue: skipping {provider} for book {book_id} (rate limited too many times this batch)");
            continue;
        }
        if metadata_repo::provider_attempted(db, book_id, provider)
            .await
            .unwrap_or(false)
        {
            continue;
        }

        let limiter = limiters.get_mut(provider);

        let result = match provider.as_str() {
            "openlibrary" => {
                metadata_service::enrich_from_openlibrary(
                    db,
                    pool,
                    book_id,
                    Some(covers_dir),
                    limiter,
                )
                .await
            }
            "googlebooks" => {
                metadata_service::enrich_from_googlebooks(
                    db,
                    pool,
                    book_id,
                    Some(covers_dir),
                    limiter,
                )
                .await
            }
            "hardcover" => {
                let api_key = config_repo::get(db, "hardcover_api_key")
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                if api_key.is_empty() {
                    tracing::warn!("metaqueue: hardcover enabled but no API key configured");
                    continue;
                }
                metadata_service::enrich_from_hardcover(
                    db,
                    pool,
                    book_id,
                    Some(covers_dir),
                    &api_key,
                    limiter,
                )
                .await
            }
            _ => {
                tracing::warn!("metaqueue: unknown provider {provider}");
                continue;
            }
        };

        match &result {
            Ok(true) => {
                tracing::info!("metaqueue: enriched book {book_id} from {provider}");
                limiter.on_success();
            }
            Ok(false) => {
                tracing::info!("metaqueue: no {provider} match for book {book_id}");
                limiter.on_success();
            }
            Err(EnrichError::RateLimited) => {
                let count = rl_tracker.record_rate_limit(provider);
                limiter.on_rate_limited();
                if rl_tracker.is_disabled(provider) {
                    tracing::warn!("metaqueue: {provider} rate limited {count} times, disabling for rest of batch");
                } else {
                    tracing::warn!("metaqueue: {provider} rate limited for book {book_id} ({count}/{MAX_RATE_LIMITS_PER_BATCH})");
                }
            }
            Err(e) => {
                tracing::warn!("metaqueue: {provider} failed for book {book_id}: {e}");
            }
        }

        // Only record the attempt if it wasn't a transient error
        if result.is_ok() {
            if let Err(e) = metadata_repo::record_provider_attempt(db, book_id, provider).await {
                tracing::warn!(
                    "metaqueue: failed to record {provider} attempt for book {book_id}: {e}"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_not_disabled_initially() {
        let tracker = BatchRateLimitTracker::new(5);
        assert!(!tracker.is_disabled("googlebooks"));
    }

    #[test]
    fn tracker_not_disabled_below_threshold() {
        let mut tracker = BatchRateLimitTracker::new(5);
        for _ in 0..4 {
            tracker.record_rate_limit("googlebooks");
        }
        assert!(!tracker.is_disabled("googlebooks"));
    }

    #[test]
    fn tracker_disabled_at_threshold() {
        let mut tracker = BatchRateLimitTracker::new(5);
        for _ in 0..5 {
            tracker.record_rate_limit("googlebooks");
        }
        assert!(tracker.is_disabled("googlebooks"));
    }

    #[test]
    fn tracker_disabled_above_threshold() {
        let mut tracker = BatchRateLimitTracker::new(5);
        for _ in 0..7 {
            tracker.record_rate_limit("googlebooks");
        }
        assert!(tracker.is_disabled("googlebooks"));
    }

    #[test]
    fn tracker_providers_are_independent() {
        let mut tracker = BatchRateLimitTracker::new(5);
        for _ in 0..5 {
            tracker.record_rate_limit("googlebooks");
        }
        assert!(tracker.is_disabled("googlebooks"));
        assert!(!tracker.is_disabled("openlibrary"));
    }

    #[test]
    fn tracker_record_returns_count() {
        let mut tracker = BatchRateLimitTracker::new(5);
        assert_eq!(tracker.record_rate_limit("googlebooks"), 1);
        assert_eq!(tracker.record_rate_limit("googlebooks"), 2);
        assert_eq!(tracker.record_rate_limit("googlebooks"), 3);
    }
}
