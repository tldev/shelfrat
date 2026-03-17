use sea_orm::DatabaseConnection;
use serde_json::{json, Value};

use crate::error::AppError;
use crate::repositories::book_repo;

pub async fn list_books(
    db: &DatabaseConnection,
    sort: Option<&str>,
    author: Option<&str>,
    tag: Option<&str>,
    format: Option<&str>,
    limit: u64,
    offset: u64,
) -> Result<Value, AppError> {
    let (rows, total) =
        book_repo::list_filtered(db, sort, author, tag, format, limit, offset).await?;

    let books: Vec<Value> = rows
        .into_iter()
        .map(format_book_list_row)
        .collect();

    Ok(json!({
        "books": books,
        "total": total,
        "limit": limit,
        "offset": offset,
    }))
}

pub async fn get_book(db: &DatabaseConnection, id: i64) -> Result<Value, AppError> {
    let book = book_repo::find_by_id(db, id)
        .await?
        .ok_or(AppError::NotFound)?;

    let metadata = book_repo::get_metadata(db, id).await?;
    let authors = book_repo::get_authors(db, id).await?;
    let tags = book_repo::get_tags(db, id).await?;

    // Extract just the filename — never expose full server paths to clients.
    let filename = std::path::Path::new(&book.file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    let meta_json = metadata.map(|m| json!({
        "title": m.title,
        "subtitle": m.subtitle,
        "description": m.description,
        "publisher": m.publisher,
        "published_date": m.published_date,
        "page_count": m.page_count,
        "language": m.language,
        "isbn_10": m.isbn_10,
        "isbn_13": m.isbn_13,
        "series_name": m.series_name,
        "series_number": m.series_number,
        "has_cover": m.cover_image_path.is_some(),
        "metadata_source": m.metadata_source,
    }));

    Ok(json!({
        "book": {
            "id": book.id,
            "filename": filename,
            "file_hash": book.file_hash,
            "file_format": book.file_format,
            "file_size_bytes": book.file_size_bytes,
            "added_at": book.added_at,
            "last_seen_at": book.last_seen_at,
            "missing": book.missing,
        },
        "metadata": meta_json,
        "authors": authors,
        "tags": tags,
    }))
}

pub async fn search_books(
    db: &DatabaseConnection,
    query: &str,
    limit: u64,
) -> Result<Value, AppError> {
    let fts_query = sanitize_fts_query(query);
    let rows = book_repo::search_fts(db, &fts_query, limit).await?;

    let books: Vec<Value> = rows
        .into_iter()
        .map(format_book_list_row)
        .collect();

    Ok(json!({
        "books": books,
        "query": query,
    }))
}

pub async fn list_authors(db: &DatabaseConnection) -> Result<Value, AppError> {
    let authors = book_repo::list_authors_with_counts(db).await?;
    Ok(json!({ "authors": authors }))
}

pub async fn list_tags(db: &DatabaseConnection) -> Result<Value, AppError> {
    let tags = book_repo::list_tags_with_counts(db).await?;
    Ok(json!({ "tags": tags }))
}

pub async fn list_formats(db: &DatabaseConnection) -> Result<Value, AppError> {
    let formats = book_repo::list_formats_with_counts(db).await?;
    Ok(json!({ "formats": formats }))
}

fn format_book_list_row(r: book_repo::BookListRow) -> Value {
    // Extract just the filename — never expose full server paths to clients.
    let filename = std::path::Path::new(&r.file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    json!({
        "id": r.id,
        "filename": filename,
        "file_format": r.file_format,
        "file_size_bytes": r.file_size_bytes,
        "added_at": r.added_at,
        "title": r.title,
        "subtitle": r.subtitle,
        "has_cover": r.cover_image_path.is_some(),
        "series_name": r.series_name,
        "series_number": r.series_number,
        "authors": r.authors_agg.split(',').filter(|s| !s.is_empty()).collect::<Vec<_>>(),
        "tags": r.tags_agg.split(',').filter(|s| !s.is_empty()).collect::<Vec<_>>(),
    })
}

/// Sanitize user input for FTS5 queries.
fn sanitize_fts_query(input: &str) -> String {
    let cleaned: String = input
        .chars()
        .filter(|c| !matches!(c, '"' | '\'' | '*' | '+' | '-' | '(' | ')' | '{' | '}' | '^' | '~'))
        .collect();

    let terms: Vec<String> = cleaned
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{t}\"*"))
        .collect();

    if terms.is_empty() {
        "\"\"".to_string()
    } else {
        terms.join(" ")
    }
}
