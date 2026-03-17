use sea_orm::*;

use crate::entities::{book, book_metadata};

pub async fn find_by_id(db: &DatabaseConnection, id: i64) -> Result<Option<book::Model>, DbErr> {
    book::Entity::find_by_id(id).one(db).await
}

pub async fn get_metadata(
    db: &DatabaseConnection,
    book_id: i64,
) -> Result<Option<book_metadata::Model>, DbErr> {
    book_metadata::Entity::find()
        .filter(book_metadata::Column::BookId.eq(book_id))
        .one(db)
        .await
}

pub async fn get_authors(db: &DatabaseConnection, book_id: i64) -> Result<Vec<String>, DbErr> {
    #[derive(FromQueryResult)]
    struct AuthorName {
        name: String,
    }

    let rows = AuthorName::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT a.name FROM authors a JOIN book_authors ba ON ba.author_id = a.id WHERE ba.book_id = ? ORDER BY ba.sort_order",
        [book_id.into()],
    ))
    .all(db)
    .await?;

    Ok(rows.into_iter().map(|r| r.name).collect())
}

pub async fn get_tags(db: &DatabaseConnection, book_id: i64) -> Result<Vec<String>, DbErr> {
    #[derive(FromQueryResult)]
    struct TagName {
        name: String,
    }

    let rows = TagName::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT t.name FROM tags t JOIN book_tags bt ON bt.tag_id = t.id WHERE bt.book_id = ?",
        [book_id.into()],
    ))
    .all(db)
    .await?;

    Ok(rows.into_iter().map(|r| r.name).collect())
}

/// Book file info for download/metadata operations.
#[derive(Debug, FromQueryResult)]
#[allow(dead_code)]
pub struct BookFileInfo {
    pub id: i64,
    pub file_path: String,
    pub file_format: String,
}

pub async fn get_file_info(
    db: &DatabaseConnection,
    id: i64,
) -> Result<Option<BookFileInfo>, DbErr> {
    BookFileInfo::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT id, file_path, file_format FROM books WHERE id = ? AND missing = 0",
        [id.into()],
    ))
    .one(db)
    .await
}

pub async fn get_cover_path(
    db: &DatabaseConnection,
    book_id: i64,
) -> Result<Option<String>, DbErr> {
    #[derive(FromQueryResult)]
    struct CoverPath {
        cover_image_path: String,
    }

    Ok(CoverPath::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT cover_image_path FROM book_metadata WHERE book_id = ? AND cover_image_path IS NOT NULL",
        [book_id.into()],
    ))
    .one(db)
    .await?
    .map(|r| r.cover_image_path))
}

/// Row for book list queries with aggregated authors/tags.
#[derive(Debug, FromQueryResult, serde::Serialize)]
pub struct BookListRow {
    pub id: i64,
    pub file_path: String,
    pub file_hash: String,
    pub file_format: String,
    pub file_size_bytes: i64,
    pub added_at: chrono::NaiveDateTime,
    pub last_seen_at: chrono::NaiveDateTime,
    pub missing: bool,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub cover_image_path: Option<String>,
    pub series_name: Option<String>,
    pub series_number: Option<f64>,
    pub authors_agg: String,
    pub tags_agg: String,
}

/// List books with filters, sorting, and pagination.
pub async fn list_filtered(
    db: &DatabaseConnection,
    sort: Option<&str>,
    author: Option<&str>,
    tag: Option<&str>,
    format: Option<&str>,
    limit: u64,
    offset: u64,
) -> Result<(Vec<BookListRow>, u64), DbErr> {
    let order_clause = match sort {
        Some("title") => "ORDER BY bm.title ASC",
        Some("author") => "ORDER BY authors_agg ASC",
        Some("added") | None => "ORDER BY b.added_at DESC",
        _ => "ORDER BY b.added_at DESC",
    };

    let mut conditions = vec!["b.missing = 0".to_string()];
    let mut values: Vec<Value> = Vec::new();

    if let Some(author) = author {
        conditions.push(
            "b.id IN (SELECT ba2.book_id FROM book_authors ba2 JOIN authors a2 ON a2.id = ba2.author_id WHERE a2.name = ?)".to_string()
        );
        values.push(author.into());
    }
    if let Some(tag) = tag {
        conditions.push(
            "b.id IN (SELECT bt2.book_id FROM book_tags bt2 JOIN tags t2 ON t2.id = bt2.tag_id WHERE t2.name = ?)".to_string()
        );
        values.push(tag.into());
    }
    if let Some(format) = format {
        conditions.push("b.file_format = ?".to_string());
        values.push(format.into());
    }

    let where_clause = conditions.join(" AND ");

    let query = format!(
        "SELECT \
            b.id, b.file_path, b.file_hash, b.file_format, b.file_size_bytes, \
            b.added_at, b.last_seen_at, b.missing, \
            bm.title, bm.subtitle, bm.cover_image_path, bm.series_name, bm.series_number, \
            COALESCE(GROUP_CONCAT(DISTINCT a.name), '') as authors_agg, \
            COALESCE(GROUP_CONCAT(DISTINCT t.name), '') as tags_agg \
         FROM books b \
         LEFT JOIN book_metadata bm ON bm.book_id = b.id \
         LEFT JOIN book_authors ba ON ba.book_id = b.id \
         LEFT JOIN authors a ON a.id = ba.author_id \
         LEFT JOIN book_tags bt ON bt.book_id = b.id \
         LEFT JOIN tags t ON t.id = bt.tag_id \
         WHERE {where_clause} \
         GROUP BY b.id \
         {order_clause} \
         LIMIT ? OFFSET ?"
    );

    let mut stmt_values = values.clone();
    stmt_values.push((limit as i64).into());
    stmt_values.push((offset as i64).into());

    let rows = BookListRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        &query,
        stmt_values,
    ))
    .all(db)
    .await?;

    let count_sql = format!(
        "SELECT COUNT(DISTINCT b.id) as count FROM books b \
         LEFT JOIN book_authors ba ON ba.book_id = b.id \
         LEFT JOIN authors a ON a.id = ba.author_id \
         LEFT JOIN book_tags bt ON bt.book_id = b.id \
         LEFT JOIN tags t ON t.id = bt.tag_id \
         WHERE {where_clause}"
    );

    #[derive(FromQueryResult)]
    struct CountResult {
        count: i64,
    }

    let total = CountResult::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        &count_sql,
        values,
    ))
    .one(db)
    .await?
    .map(|r| r.count as u64)
    .unwrap_or(0);

    Ok((rows, total))
}

/// Full-text search via FTS5.
pub async fn search_fts(
    db: &DatabaseConnection,
    fts_query: &str,
    limit: u64,
) -> Result<Vec<BookListRow>, DbErr> {
    BookListRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT \
            b.id, b.file_path, b.file_hash, b.file_format, b.file_size_bytes, \
            b.added_at, b.last_seen_at, b.missing, \
            bm.title, bm.subtitle, bm.cover_image_path, bm.series_name, bm.series_number, \
            COALESCE(GROUP_CONCAT(DISTINCT a.name), '') as authors_agg, \
            COALESCE(GROUP_CONCAT(DISTINCT t.name), '') as tags_agg \
         FROM books b \
         JOIN books_fts fts ON fts.rowid = b.id \
         LEFT JOIN book_metadata bm ON bm.book_id = b.id \
         LEFT JOIN book_authors ba ON ba.book_id = b.id \
         LEFT JOIN authors a ON a.id = ba.author_id \
         LEFT JOIN book_tags bt ON bt.book_id = b.id \
         LEFT JOIN tags t ON t.id = bt.tag_id \
         WHERE b.missing = 0 AND books_fts MATCH ? \
         GROUP BY b.id \
         ORDER BY rank \
         LIMIT ?",
        [fts_query.into(), (limit as i64).into()],
    ))
    .all(db)
    .await
}

/// Count rows for authors with book counts.
#[derive(Debug, FromQueryResult, serde::Serialize)]
pub struct NameCount {
    pub name: String,
    pub book_count: i64,
}

pub async fn list_authors_with_counts(db: &DatabaseConnection) -> Result<Vec<NameCount>, DbErr> {
    NameCount::find_by_statement(Statement::from_string(
        DatabaseBackend::Sqlite,
        "SELECT a.name, COUNT(ba.book_id) as book_count \
         FROM authors a JOIN book_authors ba ON ba.author_id = a.id \
         JOIN books b ON b.id = ba.book_id AND b.missing = 0 \
         GROUP BY a.id ORDER BY a.name ASC",
    ))
    .all(db)
    .await
}

pub async fn list_tags_with_counts(db: &DatabaseConnection) -> Result<Vec<NameCount>, DbErr> {
    NameCount::find_by_statement(Statement::from_string(
        DatabaseBackend::Sqlite,
        "SELECT t.name, COUNT(bt.book_id) as book_count \
         FROM tags t JOIN book_tags bt ON bt.tag_id = t.id \
         JOIN books b ON b.id = bt.book_id AND b.missing = 0 \
         GROUP BY t.id ORDER BY t.name ASC",
    ))
    .all(db)
    .await
}

pub async fn list_formats_with_counts(db: &DatabaseConnection) -> Result<Vec<NameCount>, DbErr> {
    NameCount::find_by_statement(Statement::from_string(
        DatabaseBackend::Sqlite,
        "SELECT file_format as name, COUNT(*) as book_count FROM books WHERE missing = 0 GROUP BY file_format ORDER BY book_count DESC",
    ))
    .all(db)
    .await
}

/// Library statistics for admin.
#[derive(Debug, serde::Serialize)]
pub struct LibraryStats {
    pub total_books: u64,
    pub available_books: u64,
    pub missing_books: u64,
    pub total_authors: u64,
    pub format_breakdown: Vec<FormatBreakdown>,
}

#[derive(Debug, FromQueryResult, serde::Serialize)]
pub struct FormatBreakdown {
    pub format: String,
    pub count: i64,
}

pub async fn library_stats(db: &DatabaseConnection) -> Result<LibraryStats, DbErr> {
    let total_books = book::Entity::find().count(db).await?;
    let available_books = book::Entity::find()
        .filter(book::Column::Missing.eq(false))
        .count(db)
        .await?;
    let missing_books = book::Entity::find()
        .filter(book::Column::Missing.eq(true))
        .count(db)
        .await?;

    use crate::entities::author;
    let total_authors = author::Entity::find().count(db).await?;

    let format_breakdown = FormatBreakdown::find_by_statement(Statement::from_string(
        DatabaseBackend::Sqlite,
        "SELECT file_format as format, COUNT(*) as count FROM books WHERE missing = 0 GROUP BY file_format ORDER BY count DESC",
    ))
    .all(db)
    .await?;

    Ok(LibraryStats {
        total_books,
        available_books,
        missing_books,
        total_authors,
        format_breakdown,
    })
}
