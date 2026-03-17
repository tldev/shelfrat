use sqlx::SqlitePool;

/// Update the FTS5 index for a single book.
/// Deletes any existing entry and re-inserts with current metadata.
pub async fn update_book_fts(db: &SqlitePool, book_id: i64) -> Result<(), sqlx::Error> {
    // Gather current metadata
    let meta = sqlx::query_as::<_, FtsSource>(
        r#"
        SELECT
            bm.title,
            bm.subtitle,
            COALESCE(
                (SELECT GROUP_CONCAT(a.name, ' ') FROM authors a
                 JOIN book_authors ba ON ba.author_id = a.id
                 WHERE ba.book_id = bm.book_id), ''
            ) as authors,
            COALESCE(
                (SELECT GROUP_CONCAT(t.name, ' ') FROM tags t
                 JOIN book_tags bt ON bt.tag_id = t.id
                 WHERE bt.book_id = bm.book_id), ''
            ) as tags,
            bm.series_name,
            COALESCE(bm.isbn_10, '') || ' ' || COALESCE(bm.isbn_13, '') as isbn
        FROM book_metadata bm
        WHERE bm.book_id = ?
        "#,
    )
    .bind(book_id)
    .fetch_optional(db)
    .await?;

    let Some(meta) = meta else {
        return Ok(());
    };

    // Delete existing FTS entry (contentless tables need explicit delete with all column values)
    // For contentless FTS5, use INSERT with the 'delete' command
    sqlx::query(
        "INSERT INTO books_fts(books_fts, rowid, title, subtitle, authors, tags, series_name, isbn) VALUES('delete', ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(meta.title.as_deref().unwrap_or(""))
    .bind(meta.subtitle.as_deref().unwrap_or(""))
    .bind(&*meta.authors)
    .bind(&*meta.tags)
    .bind(meta.series_name.as_deref().unwrap_or(""))
    .bind(&*meta.isbn)
    .execute(db)
    .await
    .ok(); // Ignore error if row doesn't exist yet

    // Insert new FTS entry
    sqlx::query(
        "INSERT INTO books_fts(rowid, title, subtitle, authors, tags, series_name, isbn) VALUES(?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(meta.title.as_deref().unwrap_or(""))
    .bind(meta.subtitle.as_deref().unwrap_or(""))
    .bind(&*meta.authors)
    .bind(&*meta.tags)
    .bind(meta.series_name.as_deref().unwrap_or(""))
    .bind(&*meta.isbn)
    .execute(db)
    .await?;

    Ok(())
}

/// Rebuild the entire FTS index from scratch.
/// Useful after bulk imports or database recovery.
pub async fn rebuild_fts_index(db: &SqlitePool) -> Result<u64, sqlx::Error> {
    // Clear the entire FTS index
    sqlx::query("DELETE FROM books_fts").execute(db).await?;

    // Get all non-missing book IDs
    let book_ids = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM books WHERE missing = 0",
    )
    .fetch_all(db)
    .await?;

    let count = book_ids.len() as u64;

    for book_id in book_ids {
        update_book_fts(db, book_id).await?;
    }

    Ok(count)
}

#[derive(Debug, sqlx::FromRow)]
struct FtsSource {
    title: Option<String>,
    subtitle: Option<String>,
    authors: String,
    tags: String,
    series_name: Option<String>,
    isbn: String,
}
