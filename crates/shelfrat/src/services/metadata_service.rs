use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use sea_orm::DatabaseConnection;

use crate::metadata::ExtractedMetadata;
use crate::provider_error::{EnrichError, ProviderError, ProviderResult};
use crate::ranking::{self, SearchQuery};
use crate::rate_limiter::RateLimiter;
use crate::repositories::metadata_repo::MetadataColumn;
use crate::repositories::{book_repo, metadata_repo};

/// A lazily-evaluated cascade step. The future is created upfront but only
/// polled (and thus the HTTP call only fires) when `run_cascade` reaches it.
type CascadeStep = Pin<Box<dyn Future<Output = ProviderResult> + Send>>;

/// Apply extracted metadata to the database for a given book.
/// Only updates fields that are currently NULL.
pub async fn apply_extracted_metadata(
    db: &DatabaseConnection,
    book_id: i64,
    meta: &ExtractedMetadata,
    covers_dir: Option<&Path>,
) -> Result<(), sea_orm::DbErr> {
    if let Some(ref title) = meta.title {
        metadata_repo::update_if_null(db, book_id, MetadataColumn::Title, title).await?;
    }
    if let Some(ref description) = meta.description {
        metadata_repo::update_if_null(db, book_id, MetadataColumn::Description, description)
            .await?;
    }
    if let Some(ref publisher) = meta.publisher {
        metadata_repo::update_if_null(db, book_id, MetadataColumn::Publisher, publisher).await?;
    }
    if let Some(ref published_date) = meta.published_date {
        metadata_repo::update_if_null(db, book_id, MetadataColumn::PublishedDate, published_date)
            .await?;
    }
    if let Some(ref language) = meta.language {
        metadata_repo::update_if_null(db, book_id, MetadataColumn::Language, language).await?;
    }

    if let Some(ref isbn) = meta.isbn {
        let clean: String = isbn
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == 'X')
            .collect();
        if clean.len() == 13 {
            metadata_repo::update_if_null(db, book_id, MetadataColumn::Isbn13, &clean).await?;
        } else if clean.len() == 10 {
            metadata_repo::update_if_null(db, book_id, MetadataColumn::Isbn10, &clean).await?;
        }
    }

    metadata_repo::set_source(db, book_id, "embedded", false).await?;

    // Overwrite filename-based title with embedded title
    if let Some(ref title) = meta.title {
        metadata_repo::set_title(db, book_id, title, "embedded").await?;
    }

    // Insert authors
    for (i, author_name) in meta.authors.iter().enumerate() {
        metadata_repo::upsert_author(db, book_id, author_name, i as i32).await?;
    }

    // Save cover image
    if let (Some(ref cover_data), Some(covers_dir)) = (&meta.cover_data, covers_dir) {
        if let Ok(cover_path) = metadata_repo::save_cover(book_id, cover_data, covers_dir) {
            metadata_repo::set_cover_path(db, book_id, &cover_path.to_string_lossy()).await?;
        }
    }

    Ok(())
}

/// Extract embedded metadata from a book file and apply it.
pub async fn extract_and_apply(
    db: &DatabaseConnection,
    pool: &sqlx::SqlitePool,
    book_id: i64,
    covers_dir: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let book = book_repo::get_file_info(db, book_id)
        .await?
        .ok_or("book not found or missing")?;

    let path = std::path::PathBuf::from(&book.file_path);
    let format = book.file_format.clone();

    let extracted =
        tokio::task::spawn_blocking(move || crate::metadata::extract(&path, &format)).await?;

    if let Some(meta) = extracted {
        apply_extracted_metadata(db, book_id, &meta, Some(covers_dir)).await?;
        crate::fts::update_book_fts(pool, book_id).await?;
        tracing::info!("extracted embedded metadata for book {book_id}");
    }

    Ok(())
}

/// Enrich a book from OpenLibrary.
pub async fn enrich_from_openlibrary(
    db: &DatabaseConnection,
    pool: &sqlx::SqlitePool,
    book_id: i64,
    covers_dir: Option<&Path>,
    limiter: &mut RateLimiter,
) -> Result<bool, EnrichError> {
    let lookup = metadata_repo::get_meta_lookup(db, book_id).await?;
    let Some(lookup) = lookup else {
        return Ok(false);
    };

    let query = SearchQuery::from_lookup(&lookup);
    let isbn = lookup.isbn_13.or(lookup.isbn_10);

    let mut steps: Vec<CascadeStep> = Vec::new();
    if let Some(isbn) = isbn.clone() {
        steps.push(Box::pin(async move {
            crate::openlibrary::lookup_by_isbn(&isbn).await
        }));
    }
    if let (Some(title), Some(author)) = (lookup.title.clone(), lookup.first_author.clone()) {
        steps.push(Box::pin(async move {
            crate::openlibrary::search_by_title_and_author(&title, &author).await
        }));
    }
    if let Some(title) = lookup.title {
        steps.push(Box::pin(async move {
            crate::openlibrary::search_by_title(&title).await
        }));
    }

    let results = run_cascade(limiter, steps).await?;
    if results.is_empty() {
        return Ok(false);
    }

    let ranked = ranking::rank_results(&query, &results);
    let Some(mut winner) = ranked.into_iter().next() else {
        return Ok(false);
    };

    if winner.cover_data.is_none() {
        if let Some(ref isbn) = winner.isbn {
            winner.cover_data = crate::openlibrary::fetch_cover_by_isbn(isbn).await;
        }
    }

    apply_extracted_metadata(db, book_id, &winner, covers_dir).await?;
    crate::fts::update_book_fts(pool, book_id)
        .await
        .map_err(|e| EnrichError::Database(sea_orm::DbErr::Custom(e.to_string())))?;
    metadata_repo::set_source(db, book_id, "openlibrary", true).await?;

    Ok(true)
}

/// Enrich a book from Hardcover.
pub async fn enrich_from_hardcover(
    db: &DatabaseConnection,
    pool: &sqlx::SqlitePool,
    book_id: i64,
    covers_dir: Option<&Path>,
    api_key: &str,
    limiter: &mut RateLimiter,
) -> Result<bool, EnrichError> {
    let lookup = metadata_repo::get_meta_lookup(db, book_id).await?;
    let Some(lookup) = lookup else {
        return Ok(false);
    };

    let query = SearchQuery::from_lookup(&lookup);
    let isbn = lookup.isbn_13.or(lookup.isbn_10);

    let mut steps: Vec<CascadeStep> = Vec::new();
    if let Some(isbn) = isbn.clone() {
        let key = api_key.to_string();
        steps.push(Box::pin(async move {
            crate::hardcover::lookup_by_isbn(&key, &isbn).await
        }));
    }
    if let (Some(title), Some(author)) = (lookup.title.clone(), lookup.first_author.clone()) {
        let key = api_key.to_string();
        steps.push(Box::pin(async move {
            crate::hardcover::search_by_title_and_author(&key, &title, &author).await
        }));
    }
    if let Some(title) = lookup.title {
        let key = api_key.to_string();
        steps.push(Box::pin(async move {
            crate::hardcover::search_by_title(&key, &title).await
        }));
    }

    let results = run_cascade(limiter, steps).await?;
    if results.is_empty() {
        return Ok(false);
    }

    let ranked = ranking::rank_results(&query, &results);
    let Some(winner) = ranked.into_iter().next() else {
        return Ok(false);
    };

    // For Hardcover search results, fetch full details for the winner via books_by_pk
    let final_meta = if let Some(ref id_str) = winner.provider_id {
        if let Ok(hc_book_id) = id_str.parse::<i64>() {
            crate::hardcover::fetch_book_detail(api_key, hc_book_id).await
        } else {
            Some(winner)
        }
    } else {
        Some(winner)
    };

    let Some(extracted) = final_meta else {
        return Ok(false);
    };

    apply_extracted_metadata(db, book_id, &extracted, covers_dir).await?;
    crate::fts::update_book_fts(pool, book_id)
        .await
        .map_err(|e| EnrichError::Database(sea_orm::DbErr::Custom(e.to_string())))?;
    metadata_repo::set_source(db, book_id, "hardcover", true).await?;

    Ok(true)
}

/// Enrich a book from Google Books.
pub async fn enrich_from_googlebooks(
    db: &DatabaseConnection,
    pool: &sqlx::SqlitePool,
    book_id: i64,
    covers_dir: Option<&Path>,
    limiter: &mut RateLimiter,
) -> Result<bool, EnrichError> {
    let lookup = metadata_repo::get_meta_lookup(db, book_id).await?;
    let Some(lookup) = lookup else {
        return Ok(false);
    };

    let query = SearchQuery::from_lookup(&lookup);
    let isbn = lookup.isbn_13.or(lookup.isbn_10);

    let mut steps: Vec<CascadeStep> = Vec::new();
    if let Some(isbn) = isbn.clone() {
        steps.push(Box::pin(async move {
            crate::googlebooks::lookup_by_isbn(&isbn).await
        }));
    }
    if let (Some(title), Some(author)) = (lookup.title.clone(), lookup.first_author.clone()) {
        steps.push(Box::pin(async move {
            crate::googlebooks::search_by_title_and_author(&title, &author).await
        }));
    }
    if let Some(title) = lookup.title {
        steps.push(Box::pin(async move {
            crate::googlebooks::search_by_title(&title).await
        }));
    }

    let results = run_cascade(limiter, steps).await?;
    if results.is_empty() {
        return Ok(false);
    }

    let ranked = ranking::rank_results(&query, &results);
    let Some(mut winner) = ranked.into_iter().next() else {
        return Ok(false);
    };

    if winner.cover_data.is_none() {
        if let Some(ref isbn) = winner.isbn {
            winner.cover_data = crate::googlebooks::fetch_cover_by_isbn(isbn).await;
        }
    }

    apply_extracted_metadata(db, book_id, &winner, covers_dir).await?;
    crate::fts::update_book_fts(pool, book_id)
        .await
        .map_err(|e| EnrichError::Database(sea_orm::DbErr::Custom(e.to_string())))?;
    metadata_repo::set_source(db, book_id, "googlebooks", true).await?;

    Ok(true)
}

/// Execute cascade steps in order, rate-limited. First non-empty result wins; errors abort.
async fn run_cascade(
    limiter: &mut RateLimiter,
    steps: Vec<CascadeStep>,
) -> Result<Vec<ExtractedMetadata>, EnrichError> {
    for step in steps {
        limiter.wait().await;
        match step.await {
            Ok(v) if !v.is_empty() => return Ok(v),
            Err(e) => return Err(to_enrich_err(e)),
            _ => {}
        }
    }
    Ok(vec![])
}

fn to_enrich_err(e: ProviderError) -> EnrichError {
    match e {
        ProviderError::RateLimited => EnrichError::RateLimited,
        ProviderError::Network(msg) | ProviderError::Fatal(msg) => EnrichError::Provider(msg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_meta(title: &str) -> ExtractedMetadata {
        ExtractedMetadata {
            title: Some(title.to_string()),
            ..Default::default()
        }
    }

    fn step_ok(metas: Vec<ExtractedMetadata>) -> CascadeStep {
        Box::pin(async move { Ok(metas) })
    }

    fn step_empty() -> CascadeStep {
        Box::pin(async { Ok(vec![]) })
    }

    fn step_rate_limited() -> CascadeStep {
        Box::pin(async { Err(ProviderError::RateLimited) })
    }

    fn step_network_err(msg: &str) -> CascadeStep {
        let msg = msg.to_string();
        Box::pin(async move { Err(ProviderError::Network(msg)) })
    }

    fn step_fatal_err(msg: &str) -> CascadeStep {
        let msg = msg.to_string();
        Box::pin(async move { Err(ProviderError::Fatal(msg)) })
    }

    fn fast_limiter() -> RateLimiter {
        RateLimiter::fixed(Duration::from_millis(0))
    }

    // --- run_cascade ---

    #[tokio::test]
    async fn cascade_first_step_succeeds() {
        let mut limiter = fast_limiter();
        let steps = vec![step_ok(vec![make_meta("Book A")])];
        let result = run_cascade(&mut limiter, steps).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title.as_deref(), Some("Book A"));
    }

    #[tokio::test]
    async fn cascade_skips_empty_takes_second() {
        let mut limiter = fast_limiter();
        let steps = vec![
            step_empty(),
            step_ok(vec![make_meta("From Step 2")]),
            step_ok(vec![make_meta("Should Not Reach")]),
        ];
        let result = run_cascade(&mut limiter, steps).await.unwrap();
        assert_eq!(result[0].title.as_deref(), Some("From Step 2"));
    }

    #[tokio::test]
    async fn cascade_skips_empty_takes_third() {
        let mut limiter = fast_limiter();
        let steps = vec![
            step_empty(),
            step_empty(),
            step_ok(vec![make_meta("From Step 3")]),
        ];
        let result = run_cascade(&mut limiter, steps).await.unwrap();
        assert_eq!(result[0].title.as_deref(), Some("From Step 3"));
    }

    #[tokio::test]
    async fn cascade_all_empty_returns_empty() {
        let mut limiter = fast_limiter();
        let steps = vec![step_empty(), step_empty(), step_empty()];
        let result = run_cascade(&mut limiter, steps).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn cascade_no_steps_returns_empty() {
        let mut limiter = fast_limiter();
        let result = run_cascade(&mut limiter, vec![]).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn cascade_error_at_first_step_aborts() {
        let mut limiter = fast_limiter();
        let steps = vec![
            step_network_err("timeout"),
            step_ok(vec![make_meta("Unreachable")]),
        ];
        let result = run_cascade(&mut limiter, steps).await;
        assert!(matches!(result, Err(EnrichError::Provider(msg)) if msg == "timeout"));
    }

    #[tokio::test]
    async fn cascade_error_at_second_step_aborts() {
        let mut limiter = fast_limiter();
        let steps = vec![step_empty(), step_network_err("connection refused")];
        let result = run_cascade(&mut limiter, steps).await;
        assert!(matches!(result, Err(EnrichError::Provider(msg)) if msg == "connection refused"));
    }

    #[tokio::test]
    async fn cascade_rate_limited_propagates() {
        let mut limiter = fast_limiter();
        let steps = vec![step_rate_limited()];
        let result = run_cascade(&mut limiter, steps).await;
        assert!(matches!(result, Err(EnrichError::RateLimited)));
    }

    #[tokio::test]
    async fn cascade_fatal_error_propagates_as_provider() {
        let mut limiter = fast_limiter();
        let steps = vec![step_fatal_err("bad api key")];
        let result = run_cascade(&mut limiter, steps).await;
        assert!(matches!(result, Err(EnrichError::Provider(msg)) if msg == "bad api key"));
    }

    #[tokio::test]
    async fn cascade_returns_multiple_results() {
        let mut limiter = fast_limiter();
        let steps = vec![step_ok(vec![
            make_meta("A"),
            make_meta("B"),
            make_meta("C"),
        ])];
        let result = run_cascade(&mut limiter, steps).await.unwrap();
        assert_eq!(result.len(), 3);
    }

    // --- to_enrich_err ---

    #[test]
    fn to_enrich_err_rate_limited() {
        let err = to_enrich_err(ProviderError::RateLimited);
        assert!(matches!(err, EnrichError::RateLimited));
    }

    #[test]
    fn to_enrich_err_network() {
        let err = to_enrich_err(ProviderError::Network("oops".into()));
        assert!(matches!(err, EnrichError::Provider(msg) if msg == "oops"));
    }

    #[test]
    fn to_enrich_err_fatal() {
        let err = to_enrich_err(ProviderError::Fatal("bad key".into()));
        assert!(matches!(err, EnrichError::Provider(msg) if msg == "bad key"));
    }
}
