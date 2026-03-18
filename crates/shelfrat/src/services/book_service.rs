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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::book_repo::BookListRow;

    // ── sanitize_fts_query ─────────────────────────────────────────

    #[test]
    fn fts_empty_input() {
        assert_eq!(sanitize_fts_query(""), "\"\"");
    }

    #[test]
    fn fts_single_word() {
        assert_eq!(sanitize_fts_query("word"), "\"word\"*");
    }

    #[test]
    fn fts_multiple_words() {
        assert_eq!(sanitize_fts_query("word1 word2"), "\"word1\"* \"word2\"*");
    }

    #[test]
    fn fts_strips_special_chars() {
        // All special chars: " ' * + - ( ) { } ^ ~
        assert_eq!(sanitize_fts_query("he\"l'l*o"), "\"hello\"*");
        assert_eq!(sanitize_fts_query("a+b-c(d)e{f}g^h~i"), "\"abcdefghi\"*");
    }

    #[test]
    fn fts_only_special_chars() {
        assert_eq!(sanitize_fts_query("\"'*+-(){}^~"), "\"\"");
    }

    #[test]
    fn fts_whitespace_only() {
        assert_eq!(sanitize_fts_query("   "), "\"\"");
    }

    #[test]
    fn fts_mixed_special_and_words() {
        assert_eq!(
            sanitize_fts_query("hello+ world-"),
            "\"hello\"* \"world\"*"
        );
    }

    // ── format_book_list_row ───────────────────────────────────────

    fn make_row(
        file_path: &str,
        authors_agg: &str,
        tags_agg: &str,
    ) -> BookListRow {
        BookListRow {
            id: 1,
            file_path: file_path.to_string(),
            file_hash: "abc123".to_string(),
            file_format: "epub".to_string(),
            file_size_bytes: 12345,
            added_at: chrono::NaiveDateTime::default(),
            last_seen_at: chrono::NaiveDateTime::default(),
            missing: false,
            title: Some("Test Book".to_string()),
            subtitle: None,
            cover_image_path: None,
            series_name: None,
            series_number: None,
            authors_agg: authors_agg.to_string(),
            tags_agg: tags_agg.to_string(),
        }
    }

    #[test]
    fn format_row_extracts_filename() {
        let row = make_row("/library/books/my_book.epub", "Author One", "fiction");
        let val = format_book_list_row(row);
        assert_eq!(val["filename"], "my_book.epub");
    }

    #[test]
    fn format_row_splits_authors() {
        let row = make_row("/a.epub", "Alice,Bob,Charlie", "");
        let val = format_book_list_row(row);
        let authors = val["authors"].as_array().unwrap();
        assert_eq!(authors.len(), 3);
        assert_eq!(authors[0], "Alice");
        assert_eq!(authors[1], "Bob");
        assert_eq!(authors[2], "Charlie");
    }

    #[test]
    fn format_row_splits_tags() {
        let row = make_row("/a.epub", "", "sci-fi,fantasy");
        let val = format_book_list_row(row);
        let tags = val["tags"].as_array().unwrap();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0], "sci-fi");
        assert_eq!(tags[1], "fantasy");
    }

    #[test]
    fn format_row_empty_authors_and_tags() {
        let row = make_row("/a.epub", "", "");
        let val = format_book_list_row(row);
        assert_eq!(val["authors"].as_array().unwrap().len(), 0);
        assert_eq!(val["tags"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn format_row_has_cover_false_when_none() {
        let row = make_row("/a.epub", "", "");
        let val = format_book_list_row(row);
        assert_eq!(val["has_cover"], false);
    }

    #[test]
    fn format_row_has_cover_true_when_some() {
        let mut row = make_row("/a.epub", "", "");
        row.cover_image_path = Some("/covers/1.jpg".to_string());
        let val = format_book_list_row(row);
        assert_eq!(val["has_cover"], true);
    }

    #[test]
    fn format_row_bare_filename() {
        // Path with no directory component.
        let row = make_row("just_a_file.pdf", "", "");
        let val = format_book_list_row(row);
        assert_eq!(val["filename"], "just_a_file.pdf");
    }
}
