use std::path::Path;

use sea_orm::*;

use crate::entities::book_metadata;

#[allow(dead_code)]
pub async fn get_book_metadata(
    db: &DatabaseConnection,
    book_id: i64,
) -> Result<Option<book_metadata::Model>, DbErr> {
    book_metadata::Entity::find()
        .filter(book_metadata::Column::BookId.eq(book_id))
        .one(db)
        .await
}

/// Lookup info for metadata enrichment (title, isbn_10, isbn_13).
#[derive(Debug, FromQueryResult)]
pub struct MetaLookup {
    pub title: Option<String>,
    pub isbn_10: Option<String>,
    pub isbn_13: Option<String>,
}

pub async fn get_meta_lookup(
    db: &DatabaseConnection,
    book_id: i64,
) -> Result<Option<MetaLookup>, DbErr> {
    MetaLookup::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT bm.title, bm.isbn_10, bm.isbn_13 FROM book_metadata bm WHERE bm.book_id = ?",
        [book_id.into()],
    ))
    .one(db)
    .await
}

/// Allowed columns for metadata updates. Prevents SQL injection by
/// ensuring only known column names are interpolated into queries.
pub enum MetadataColumn {
    Title,
    Description,
    Publisher,
    PublishedDate,
    Language,
    Isbn10,
    Isbn13,
}

impl MetadataColumn {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Description => "description",
            Self::Publisher => "publisher",
            Self::PublishedDate => "published_date",
            Self::Language => "language",
            Self::Isbn10 => "isbn_10",
            Self::Isbn13 => "isbn_13",
        }
    }
}

/// Update a metadata field only if currently NULL.
pub async fn update_if_null(
    db: &DatabaseConnection,
    book_id: i64,
    field: MetadataColumn,
    value: &str,
) -> Result<(), DbErr> {
    let column = field.as_str();
    let sql = format!(
        "UPDATE book_metadata SET {column} = ? WHERE book_id = ? AND {column} IS NULL"
    );
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        &sql,
        [value.into(), book_id.into()],
    ))
    .await?;
    Ok(())
}

/// Set metadata source and fetched timestamp.
pub async fn set_source(
    db: &DatabaseConnection,
    book_id: i64,
    source: &str,
    only_if_lower: bool,
) -> Result<(), DbErr> {
    let condition = if only_if_lower {
        "AND (metadata_source IS NULL OR metadata_source = 'embedded')"
    } else {
        "AND metadata_source IS NULL"
    };
    let sql = format!(
        "UPDATE book_metadata SET metadata_source = ?, metadata_fetched_at = datetime('now') WHERE book_id = ? {condition}"
    );
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        &sql,
        [source.into(), book_id.into()],
    ))
    .await?;
    Ok(())
}

/// Overwrite title (used when embedded metadata provides a real title).
pub async fn set_title(
    db: &DatabaseConnection,
    book_id: i64,
    title: &str,
    source: &str,
) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "UPDATE book_metadata SET title = ? WHERE book_id = ? AND metadata_source = ?",
        [title.into(), book_id.into(), source.into()],
    ))
    .await?;
    Ok(())
}

/// Set cover image path.
pub async fn set_cover_path(
    db: &DatabaseConnection,
    book_id: i64,
    path: &str,
) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "UPDATE book_metadata SET cover_image_path = ? WHERE book_id = ?",
        [path.into(), book_id.into()],
    ))
    .await?;
    Ok(())
}

/// Upsert an author and link to a book.
pub async fn upsert_author(
    db: &DatabaseConnection,
    book_id: i64,
    author_name: &str,
    sort_order: i32,
) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "INSERT OR IGNORE INTO authors (name) VALUES (?)",
        [author_name.into()],
    ))
    .await?;

    #[derive(FromQueryResult)]
    struct IdRow {
        id: i64,
    }
    let author_id = IdRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT id FROM authors WHERE name = ?",
        [author_name.into()],
    ))
    .one(db)
    .await?
    .ok_or_else(|| DbErr::Custom("author not found after insert".into()))?
    .id;

    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "INSERT OR IGNORE INTO book_authors (book_id, author_id, sort_order) VALUES (?, ?, ?)",
        [book_id.into(), author_id.into(), sort_order.into()],
    ))
    .await?;

    Ok(())
}

/// Record a provider attempt for a book.
pub async fn record_provider_attempt(
    db: &DatabaseConnection,
    book_id: i64,
    provider: &str,
) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "INSERT OR IGNORE INTO metadata_provider_attempts (book_id, provider) VALUES (?, ?)",
        [book_id.into(), provider.into()],
    ))
    .await?;
    Ok(())
}

/// Check if a provider was already attempted for a book.
pub async fn provider_attempted(
    db: &DatabaseConnection,
    book_id: i64,
    provider: &str,
) -> Result<bool, DbErr> {
    #[derive(FromQueryResult)]
    struct Exists {
        exists: bool,
    }

    Ok(Exists::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT EXISTS(SELECT 1 FROM metadata_provider_attempts WHERE book_id = ? AND provider = ?) as \"exists\"",
        [book_id.into(), provider.into()],
    ))
    .one(db)
    .await?
    .map(|r| r.exists)
    .unwrap_or(false))
}

/// Check if a book still needs enrichment (missing description, cover, or authors).
pub async fn needs_enrichment(db: &DatabaseConnection, book_id: i64) -> Result<bool, DbErr> {
    #[derive(FromQueryResult)]
    struct NeedsIt {
        needs: bool,
    }

    Ok(NeedsIt::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT (bm.description IS NULL OR bm.cover_image_path IS NULL \
         OR NOT EXISTS (SELECT 1 FROM book_authors ba WHERE ba.book_id = bm.book_id)) as needs \
         FROM book_metadata bm WHERE bm.book_id = ?",
        [book_id.into()],
    ))
    .one(db)
    .await?
    .map(|r| r.needs)
    .unwrap_or(false))
}

/// Check if a book's metadata_source is NULL (needs embedded extraction).
pub async fn needs_embedded_extraction(
    db: &DatabaseConnection,
    book_id: i64,
) -> Result<bool, DbErr> {
    #[derive(FromQueryResult)]
    struct NeedsIt {
        needs: bool,
    }

    Ok(NeedsIt::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT metadata_source IS NULL as needs FROM book_metadata WHERE book_id = ?",
        [book_id.into()],
    ))
    .one(db)
    .await?
    .map(|r| r.needs)
    .unwrap_or(false))
}

/// Get book IDs that need metadata enrichment, respecting retry window.
pub async fn books_needing_metadata(
    db: &DatabaseConnection,
    retry_hours: i64,
    provider_count: i64,
) -> Result<Vec<i64>, DbErr> {
    #[derive(FromQueryResult)]
    struct IdRow {
        id: i64,
    }

    let rows = IdRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT b.id FROM books b \
         JOIN book_metadata bm ON bm.book_id = b.id \
         WHERE b.missing = 0 \
           AND (bm.metadata_source IS NULL \
                OR bm.description IS NULL \
                OR NOT EXISTS (SELECT 1 FROM book_authors ba WHERE ba.book_id = b.id)) \
           AND (SELECT COUNT(*) FROM metadata_provider_attempts mpa \
                WHERE mpa.book_id = b.id \
                  AND mpa.attempted_at > datetime('now', '-' || ? || ' hours')) < ?",
        [retry_hours.into(), provider_count.into()],
    ))
    .all(db)
    .await?;

    Ok(rows.into_iter().map(|r| r.id).collect())
}

/// Save a cover image to disk and return the full path.
pub fn save_cover(
    book_id: i64,
    data: &[u8],
    covers_dir: &Path,
) -> Result<std::path::PathBuf, std::io::Error> {
    std::fs::create_dir_all(covers_dir)?;

    let ext = if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "png"
    } else if data.starts_with(&[0xFF, 0xD8]) {
        "jpg"
    } else if data.starts_with(b"GIF") {
        "gif"
    } else if data.starts_with(b"RIFF") && data.len() > 12 && &data[8..12] == b"WEBP" {
        "webp"
    } else {
        "jpg"
    };

    let filename = format!("{book_id}.{ext}");
    let full_path = covers_dir.join(&filename);
    std::fs::write(&full_path, data)?;

    Ok(full_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── MetadataColumn::as_str ─────────────────────────────────────

    #[test]
    fn metadata_column_title() {
        assert_eq!(MetadataColumn::Title.as_str(), "title");
    }

    #[test]
    fn metadata_column_description() {
        assert_eq!(MetadataColumn::Description.as_str(), "description");
    }

    #[test]
    fn metadata_column_publisher() {
        assert_eq!(MetadataColumn::Publisher.as_str(), "publisher");
    }

    #[test]
    fn metadata_column_published_date() {
        assert_eq!(MetadataColumn::PublishedDate.as_str(), "published_date");
    }

    #[test]
    fn metadata_column_language() {
        assert_eq!(MetadataColumn::Language.as_str(), "language");
    }

    #[test]
    fn metadata_column_isbn10() {
        assert_eq!(MetadataColumn::Isbn10.as_str(), "isbn_10");
    }

    #[test]
    fn metadata_column_isbn13() {
        assert_eq!(MetadataColumn::Isbn13.as_str(), "isbn_13");
    }

    // ── save_cover ─────────────────────────────────────────────────

    /// Create a unique temp directory for each test to avoid race conditions.
    fn covers_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join("shelfrat_test_covers")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn save_cover_png_detected() {
        let dir = covers_dir("png");
        // PNG magic bytes: 0x89 0x50 0x4E 0x47
        let data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0xFF, 0xFF];
        let path = save_cover(1, &data, &dir).unwrap();
        assert!(path.to_string_lossy().ends_with("1.png"));
        assert!(path.is_file());
        assert_eq!(std::fs::read(&path).unwrap(), data);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_cover_jpg_detected() {
        let dir = covers_dir("jpg");
        // JPEG magic bytes: 0xFF 0xD8
        let data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let path = save_cover(2, &data, &dir).unwrap();
        assert!(path.to_string_lossy().ends_with("2.jpg"));
        assert!(path.is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_cover_gif_detected() {
        let dir = covers_dir("gif");
        // GIF magic bytes: "GIF"
        let data = b"GIF89a\x01\x00\x01\x00".to_vec();
        let path = save_cover(3, &data, &dir).unwrap();
        assert!(path.to_string_lossy().ends_with("3.gif"));
        assert!(path.is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_cover_webp_detected() {
        let dir = covers_dir("webp");
        // WEBP: starts with "RIFF", then 4 bytes, then "WEBP"
        let mut data = b"RIFF".to_vec();
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // file size
        data.extend_from_slice(b"WEBP");
        data.extend_from_slice(&[0x00; 10]); // some payload
        let path = save_cover(4, &data, &dir).unwrap();
        assert!(path.to_string_lossy().ends_with("4.webp"));
        assert!(path.is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_cover_unknown_falls_back_to_jpg() {
        let dir = covers_dir("unknown");
        let data = vec![0x00, 0x01, 0x02, 0x03];
        let path = save_cover(5, &data, &dir).unwrap();
        assert!(path.to_string_lossy().ends_with("5.jpg"));
        assert!(path.is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_cover_creates_directory() {
        let dir = covers_dir("nested_create");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!dir.exists());

        let data = vec![0xFF, 0xD8, 0xFF];
        let path = save_cover(6, &data, &dir).unwrap();
        assert!(dir.exists());
        assert!(path.is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_cover_file_content_matches() {
        let dir = covers_dir("content");
        let data = vec![0x89, 0x50, 0x4E, 0x47, 0xAA, 0xBB, 0xCC];
        let path = save_cover(7, &data, &dir).unwrap();
        let read_back = std::fs::read(&path).unwrap();
        assert_eq!(data, read_back);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Import scanned files into the database.
/// This remains using raw SqlitePool because it needs transaction support and bulk operations.
pub async fn import_scanned_files(
    pool: &sqlx::SqlitePool,
    files: &[crate::scanner::ScannedFile],
    mark_missing: bool,
) -> Result<crate::scanner::ImportResult, crate::scanner::ScanError> {
    let mut imported = 0u64;
    let skipped = 0u64;
    let mut updated = 0u64;

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| crate::scanner::ScanError::Database(e.to_string()))?;

    for file in files {
        let file_path_str = file.path.to_string_lossy().to_string();

        let existing_by_path = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM books WHERE file_path = ?",
        )
        .bind(&file_path_str)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| crate::scanner::ScanError::Database(e.to_string()))?;

        if let Some(book_id) = existing_by_path {
            sqlx::query(
                "UPDATE books SET last_seen_at = datetime('now'), missing = 0, file_hash = ?, file_size_bytes = ? WHERE id = ?",
            )
            .bind(&file.hash)
            .bind(file.size_bytes as i64)
            .bind(book_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| crate::scanner::ScanError::Database(e.to_string()))?;
            updated += 1;
            continue;
        }

        let existing_by_hash = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM books WHERE file_hash = ?",
        )
        .bind(&file.hash)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| crate::scanner::ScanError::Database(e.to_string()))?;

        if let Some(book_id) = existing_by_hash {
            sqlx::query(
                "UPDATE books SET file_path = ?, file_size_bytes = ?, last_seen_at = datetime('now'), missing = 0 WHERE id = ?",
            )
            .bind(&file_path_str)
            .bind(file.size_bytes as i64)
            .bind(book_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| crate::scanner::ScanError::Database(e.to_string()))?;
            updated += 1;
            continue;
        }

        let book_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO books (file_path, file_hash, file_format, file_size_bytes) VALUES (?, ?, ?, ?) RETURNING id",
        )
        .bind(&file_path_str)
        .bind(&file.hash)
        .bind(&file.format)
        .bind(file.size_bytes as i64)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| crate::scanner::ScanError::Database(e.to_string()))?;

        let title = file
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown");
        sqlx::query("INSERT INTO book_metadata (book_id, title) VALUES (?, ?)")
            .bind(book_id)
            .bind(title)
            .execute(&mut *tx)
            .await
            .map_err(|e| crate::scanner::ScanError::Database(e.to_string()))?;

        imported += 1;
    }

    if mark_missing && !files.is_empty() {
        let seen_paths: Vec<String> = files
            .iter()
            .map(|f| f.path.to_string_lossy().to_string())
            .collect();
        let placeholders = seen_paths.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "UPDATE books SET missing = 1 WHERE missing = 0 AND file_path NOT IN ({placeholders})"
        );
        let mut q = sqlx::query(&query);
        for p in &seen_paths {
            q = q.bind(p);
        }
        q.execute(&mut *tx)
            .await
            .map_err(|e| crate::scanner::ScanError::Database(e.to_string()))?;
    }

    tx.commit()
        .await
        .map_err(|e| crate::scanner::ScanError::Database(e.to_string()))?;

    if imported > 0 {
        if let Err(e) = crate::fts::rebuild_fts_index(pool).await {
            tracing::warn!("FTS rebuild after scan failed: {e}");
        }
    }

    Ok(crate::scanner::ImportResult {
        imported,
        updated,
        skipped,
        total_scanned: files.len() as u64,
    })
}
