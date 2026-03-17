use std::path::Path;

use sea_orm::DatabaseConnection;

use crate::metadata::ExtractedMetadata;
use crate::repositories::metadata_repo::MetadataColumn;
use crate::repositories::{book_repo, metadata_repo};

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
        metadata_repo::update_if_null(db, book_id, MetadataColumn::Description, description).await?;
    }
    if let Some(ref publisher) = meta.publisher {
        metadata_repo::update_if_null(db, book_id, MetadataColumn::Publisher, publisher).await?;
    }
    if let Some(ref published_date) = meta.published_date {
        metadata_repo::update_if_null(db, book_id, MetadataColumn::PublishedDate, published_date).await?;
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

    // Update FTS index — uses raw SqlitePool since FTS is SQLite-specific
    // We use the pool extracted from the connection if available
    // For now, this is handled by the caller after apply_extracted_metadata
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
) -> Result<bool, sea_orm::DbErr> {
    let meta = metadata_repo::get_meta_lookup(db, book_id).await?;
    let Some(meta) = meta else {
        return Ok(false);
    };

    let isbn = meta.isbn_13.or(meta.isbn_10);
    let result = if let Some(ref isbn) = isbn {
        crate::openlibrary::lookup_by_isbn(isbn).await
    } else if let Some(ref title) = meta.title {
        crate::openlibrary::search_by_title(title).await
    } else {
        None
    };

    let Some(extracted) = result else {
        return Ok(false);
    };

    apply_extracted_metadata(db, book_id, &extracted, covers_dir).await?;
    crate::fts::update_book_fts(pool, book_id)
        .await
        .map_err(|e| sea_orm::DbErr::Custom(e.to_string()))?;
    metadata_repo::set_source(db, book_id, "openlibrary", true).await?;

    Ok(true)
}

/// Enrich a book from Google Books.
pub async fn enrich_from_googlebooks(
    db: &DatabaseConnection,
    pool: &sqlx::SqlitePool,
    book_id: i64,
    covers_dir: Option<&Path>,
) -> Result<bool, sea_orm::DbErr> {
    let meta = metadata_repo::get_meta_lookup(db, book_id).await?;
    let Some(meta) = meta else {
        return Ok(false);
    };

    let isbn = meta.isbn_13.or(meta.isbn_10);
    let result = if let Some(ref isbn) = isbn {
        crate::googlebooks::lookup_by_isbn(isbn).await
    } else if let Some(ref title) = meta.title {
        crate::googlebooks::search_by_title(title).await
    } else {
        None
    };

    let Some(extracted) = result else {
        return Ok(false);
    };

    apply_extracted_metadata(db, book_id, &extracted, covers_dir).await?;
    crate::fts::update_book_fts(pool, book_id)
        .await
        .map_err(|e| sea_orm::DbErr::Custom(e.to_string()))?;
    metadata_repo::set_source(db, book_id, "googlebooks", true).await?;

    Ok(true)
}
