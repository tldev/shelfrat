use serde_json::Value;

use crate::metadata::ExtractedMetadata;

const HC_GRAPHQL_URL: &str = "https://api.hardcover.app/v1/graphql";

/// Normalize the API key — Hardcover's settings page shows the full header value
/// (`Bearer eyJhb...`), so strip the prefix if the user pasted it.
fn normalize_key(api_key: &str) -> &str {
    api_key
        .strip_prefix("Bearer ")
        .or_else(|| api_key.strip_prefix("bearer "))
        .unwrap_or(api_key)
}

/// Execute a GraphQL request and return the `data` field.
async fn graphql(api_key: &str, query: &str, variables: Value) -> Option<Value> {
    let body = serde_json::json!({ "query": query, "variables": variables });

    let resp = reqwest::Client::new()
        .post(HC_GRAPHQL_URL)
        .header(
            "Authorization",
            format!("Bearer {}", normalize_key(api_key)),
        )
        .json(&body)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let json: Value = resp.json().await.ok()?;
    json.get("data").cloned()
}

/// Look up book metadata from Hardcover by ISBN.
///
/// Queries `editions` as the top-level entity to stay within the API's
/// max query depth of 3.
pub async fn lookup_by_isbn(api_key: &str, isbn: &str) -> Option<ExtractedMetadata> {
    // Max depth paths: editions->publisher->name (3), editions->image->url (3),
    // editions->book->title (3), editions->book->cached_contributors (3)
    let query = r#"
        query BookByIsbn($isbn: String!) {
            editions(
                where: {_or: [{isbn_13: {_eq: $isbn}}, {isbn_10: {_eq: $isbn}}]}
                limit: 1
            ) {
                isbn_13
                isbn_10
                release_date
                image { url }
                publisher { name }
                book {
                    title
                    description
                    cached_contributors
                }
            }
        }
    "#;

    let data = graphql(api_key, query, serde_json::json!({ "isbn": isbn })).await?;
    let edition = data.get("editions")?.as_array()?.first()?;

    let book = edition.get("book")?;
    let title = str_field(book, "title");
    let description = str_field(book, "description");
    let authors = extract_contributors(book.get("cached_contributors"));

    let isbn_val = str_field(edition, "isbn_13")
        .or_else(|| str_field(edition, "isbn_10"))
        .or_else(|| Some(isbn.to_string()));

    let publisher = edition.get("publisher").and_then(|p| str_field(p, "name"));

    let cover_url = edition.get("image").and_then(|img| str_field(img, "url"));
    let cover_data = fetch_cover_opt(cover_url.as_deref()).await;

    Some(ExtractedMetadata {
        title,
        description,
        publisher,
        published_date: str_field(edition, "release_date"),
        language: None, // would require depth 4 (editions->language->language)
        isbn: isbn_val,
        authors,
        cover_data,
    })
}

/// Search Hardcover by title.
///
/// Uses the `search` endpoint (Typesense-backed) since `_ilike` is disabled,
/// then fetches full book details via `books_by_pk`.
pub async fn search_by_title(api_key: &str, title: &str) -> Option<ExtractedMetadata> {
    // Step 1: search for the book ID
    let search_query = r#"
        query SearchBooks($query: String!) {
            search(query: $query, query_type: "Book", per_page: 1, page: 1) {
                results
            }
        }
    "#;

    let data = graphql(api_key, search_query, serde_json::json!({ "query": title })).await?;

    let results = data.get("search")?.get("results")?;
    let hit = results.get("hits")?.as_array()?.first()?;
    let doc = hit.get("document")?;

    // Typesense IDs are strings; parse to int for books_by_pk
    let book_id: i64 = doc
        .get("id")
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))?;

    // Step 2: fetch book details — max depth 3 (books_by_pk->editions->release_date)
    let detail_query = r#"
        query GetBook($id: Int!) {
            books_by_pk(id: $id) {
                title
                description
                cached_contributors
                cached_image
                editions(limit: 1) {
                    isbn_13
                    isbn_10
                    release_date
                }
            }
        }
    "#;

    let detail = graphql(api_key, detail_query, serde_json::json!({ "id": book_id })).await?;
    let book = detail.get("books_by_pk")?;

    let found_title = str_field(book, "title");
    let description = str_field(book, "description");
    let authors = extract_contributors(book.get("cached_contributors"));

    let edition = book
        .get("editions")
        .and_then(|e| e.as_array())
        .and_then(|a| a.first());

    let isbn = edition.and_then(|e| str_field(e, "isbn_13").or_else(|| str_field(e, "isbn_10")));
    let published_date = edition.and_then(|e| str_field(e, "release_date"));

    // cached_image is a JSONB field — may be {"url": "..."} or a bare string
    let cover_url = book.get("cached_image").and_then(|ci| {
        ci.get("url")
            .and_then(|v| v.as_str())
            .or_else(|| ci.as_str())
            .map(String::from)
    });
    let cover_data = fetch_cover_opt(cover_url.as_deref()).await;

    Some(ExtractedMetadata {
        title: found_title,
        description,
        publisher: None, // would require depth 4 from books_by_pk
        published_date,
        language: None,
        isbn,
        authors,
        cover_data,
    })
}

/// Test that an API key is valid by running a lightweight query.
pub async fn test_api_key(api_key: &str) -> Result<(), String> {
    let body = serde_json::json!({
        "query": "query { me { id } }"
    });

    let resp = reqwest::Client::new()
        .post(HC_GRAPHQL_URL)
        .header(
            "Authorization",
            format!("Bearer {}", normalize_key(api_key)),
        )
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("API returned status {}", resp.status()));
    }

    let text = resp.text().await.map_err(|e| format!("read failed: {e}"))?;
    if text.contains("\"errors\"") && !text.contains("\"data\"") {
        return Err("API key is invalid or unauthorized".to_string());
    }

    Ok(())
}

// --- Helpers ---

/// Extract a non-empty string from a JSON object field.
fn str_field(obj: &Value, key: &str) -> Option<String> {
    obj.get(key)?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// Extract author names from the `cached_contributors` JSONB field.
/// Handles multiple shapes: `[{"author":{"name":"..."}}, ...]`,
/// `[{"name":"..."}, ...]`, or `["string", ...]`.
fn extract_contributors(value: Option<&Value>) -> Vec<String> {
    let Some(arr) = value.and_then(|v| v.as_array()) else {
        return vec![];
    };

    arr.iter()
        .filter_map(|entry| {
            entry
                .get("author")
                .and_then(|a| a.get("name"))
                .and_then(|n| n.as_str())
                .or_else(|| entry.get("name").and_then(|n| n.as_str()))
                .or_else(|| entry.as_str())
                .filter(|s| !s.is_empty())
        })
        .map(String::from)
        .collect()
}

/// Fetch a cover image, returning None if url is None or fetch fails.
async fn fetch_cover_opt(url: Option<&str>) -> Option<Vec<u8>> {
    let url = url?;
    let resp = reqwest::get(url).await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    if bytes.len() < 100 {
        return None;
    }
    Some(bytes.to_vec())
}
