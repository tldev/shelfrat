use serde_json::Value;

use crate::metadata::ExtractedMetadata;
use crate::provider_error::{ProviderError, ProviderResult};

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
async fn graphql(api_key: &str, query: &str, variables: Value) -> Result<Value, ProviderError> {
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
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(ProviderError::RateLimited);
    }
    if !resp.status().is_success() {
        return Err(ProviderError::Network(format!("HTTP {}", resp.status())));
    }

    let json: Value = resp
        .json()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    json.get("data")
        .cloned()
        .ok_or_else(|| ProviderError::Network("no data in response".to_string()))
}

/// Look up book metadata from Hardcover by ISBN.
///
/// Queries `editions` as the top-level entity to stay within the API's
/// max query depth of 3.
pub async fn lookup_by_isbn(api_key: &str, isbn: &str) -> ProviderResult {
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

    let editions = data.get("editions").and_then(|e| e.as_array());
    let Some(edition) = editions.and_then(|a| a.first()) else {
        return Ok(vec![]);
    };

    let book = match edition.get("book") {
        Some(b) => b,
        None => return Ok(vec![]),
    };

    let title = str_field(book, "title");
    let description = str_field(book, "description");
    let authors = extract_contributors(book.get("cached_contributors"));

    let isbn_val = str_field(edition, "isbn_13")
        .or_else(|| str_field(edition, "isbn_10"))
        .or_else(|| Some(isbn.to_string()));

    let publisher = edition.get("publisher").and_then(|p| str_field(p, "name"));

    let cover_url = edition.get("image").and_then(|img| str_field(img, "url"));
    let cover_data = fetch_cover_opt(cover_url.as_deref()).await;

    Ok(vec![ExtractedMetadata {
        title,
        description,
        publisher,
        published_date: str_field(edition, "release_date"),
        language: None,
        isbn: isbn_val,
        authors,
        cover_data,
        provider_id: None,
    }])
}

/// Search Hardcover by title, returning lightweight results from Typesense.
///
/// Uses the `search` endpoint (Typesense-backed) since `_ilike` is disabled.
/// Returns up to 5 results with title/authors for ranking. The caller should
/// use `fetch_book_detail` for the winning result to get full metadata + cover.
pub async fn search_by_title(api_key: &str, title: &str) -> ProviderResult {
    search_internal(api_key, title).await
}

/// Search Hardcover by title and author (concatenated free text).
pub async fn search_by_title_and_author(
    api_key: &str,
    title: &str,
    author: &str,
) -> ProviderResult {
    let query_text = format!("{title} {author}");
    search_internal(api_key, &query_text).await
}

async fn search_internal(api_key: &str, query_text: &str) -> ProviderResult {
    let search_query = r#"
        query SearchBooks($query: String!) {
            search(query: $query, query_type: "Book", per_page: 5, page: 1) {
                results
            }
        }
    "#;

    let data = graphql(
        api_key,
        search_query,
        serde_json::json!({ "query": query_text }),
    )
    .await?;

    let hits = data
        .get("search")
        .and_then(|s| s.get("results"))
        .and_then(|r| r.get("hits"))
        .and_then(|h| h.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(parse_typesense_hits(&hits))
}

/// Fetch full book details from Hardcover by book ID.
/// Called after ranking picks the winning search result.
pub async fn fetch_book_detail(api_key: &str, book_id: i64) -> Option<ExtractedMetadata> {
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

    let detail = graphql(api_key, detail_query, serde_json::json!({ "id": book_id }))
        .await
        .ok()?;
    let book = detail.get("books_by_pk")?;

    let title = str_field(book, "title");
    let description = str_field(book, "description");
    let authors = extract_contributors(book.get("cached_contributors"));

    let edition = book
        .get("editions")
        .and_then(|e| e.as_array())
        .and_then(|a| a.first());

    let isbn = edition.and_then(|e| str_field(e, "isbn_13").or_else(|| str_field(e, "isbn_10")));
    let published_date = edition.and_then(|e| str_field(e, "release_date"));

    let cover_url = book.get("cached_image").and_then(|ci| {
        ci.get("url")
            .and_then(|v| v.as_str())
            .or_else(|| ci.as_str())
            .map(String::from)
    });
    let cover_data = fetch_cover_opt(cover_url.as_deref()).await;

    Some(ExtractedMetadata {
        title,
        description,
        publisher: None,
        published_date,
        language: None,
        isbn,
        authors,
        cover_data,
        provider_id: None,
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

/// Parse Typesense search hits into lightweight ExtractedMetadata for ranking.
fn parse_typesense_hits(hits: &[Value]) -> Vec<ExtractedMetadata> {
    hits.iter()
        .filter_map(|hit| {
            let doc = hit.get("document")?;

            let book_id = doc
                .get("id")
                .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()));

            let title = doc.get("title").and_then(|v| v.as_str()).map(String::from);

            let authors = doc
                .get("author_names")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();

            Some(ExtractedMetadata {
                title,
                authors,
                provider_id: book_id.map(|id| id.to_string()),
                ..Default::default()
            })
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    // --- str_field ---

    #[test]
    fn str_field_valid() {
        let obj = serde_json::json!({"title": "Dune"});
        assert_eq!(str_field(&obj, "title"), Some("Dune".to_string()));
    }

    #[test]
    fn str_field_empty_string_returns_none() {
        let obj = serde_json::json!({"title": ""});
        assert_eq!(str_field(&obj, "title"), None);
    }

    #[test]
    fn str_field_missing_key_returns_none() {
        let obj = serde_json::json!({"other": "value"});
        assert_eq!(str_field(&obj, "title"), None);
    }

    #[test]
    fn str_field_non_string_returns_none() {
        let obj = serde_json::json!({"count": 42});
        assert_eq!(str_field(&obj, "count"), None);
    }

    #[test]
    fn str_field_null_returns_none() {
        let obj = serde_json::json!({"title": null});
        assert_eq!(str_field(&obj, "title"), None);
    }

    // --- extract_contributors ---

    #[test]
    fn contributors_author_name_format() {
        let val = serde_json::json!([
            {"author": {"name": "Frank Herbert"}},
            {"author": {"name": "Brian Herbert"}}
        ]);
        let result = extract_contributors(Some(&val));
        assert_eq!(result, vec!["Frank Herbert", "Brian Herbert"]);
    }

    #[test]
    fn contributors_name_format() {
        let val = serde_json::json!([
            {"name": "Stephen King"},
            {"name": "Peter Straub"}
        ]);
        let result = extract_contributors(Some(&val));
        assert_eq!(result, vec!["Stephen King", "Peter Straub"]);
    }

    #[test]
    fn contributors_bare_string_format() {
        let val = serde_json::json!(["Author One", "Author Two"]);
        let result = extract_contributors(Some(&val));
        assert_eq!(result, vec!["Author One", "Author Two"]);
    }

    #[test]
    fn contributors_mixed_formats() {
        let val = serde_json::json!([
            {"author": {"name": "From Author Key"}},
            {"name": "From Name Key"},
            "Bare String"
        ]);
        let result = extract_contributors(Some(&val));
        assert_eq!(
            result,
            vec!["From Author Key", "From Name Key", "Bare String"]
        );
    }

    #[test]
    fn contributors_empty_array() {
        let val = serde_json::json!([]);
        let result = extract_contributors(Some(&val));
        assert!(result.is_empty());
    }

    #[test]
    fn contributors_none() {
        let result = extract_contributors(None);
        assert!(result.is_empty());
    }

    #[test]
    fn contributors_not_array() {
        let val = serde_json::json!("not an array");
        let result = extract_contributors(Some(&val));
        assert!(result.is_empty());
    }

    #[test]
    fn contributors_filters_empty_names() {
        let val = serde_json::json!([
            {"name": ""},
            {"name": "Valid Author"},
            ""
        ]);
        let result = extract_contributors(Some(&val));
        assert_eq!(result, vec!["Valid Author"]);
    }

    // --- parse_typesense_hits ---

    #[test]
    fn typesense_hit_with_all_fields() {
        let hits = vec![serde_json::json!({
            "document": {
                "id": 12345,
                "title": "Dune",
                "author_names": ["Frank Herbert"]
            }
        })];
        let results = parse_typesense_hits(&hits);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title.as_deref(), Some("Dune"));
        assert_eq!(results[0].authors, vec!["Frank Herbert"]);
        assert_eq!(results[0].provider_id.as_deref(), Some("12345"));
    }

    #[test]
    fn typesense_hit_id_as_string() {
        let hits = vec![serde_json::json!({
            "document": {
                "id": "67890",
                "title": "Test"
            }
        })];
        let results = parse_typesense_hits(&hits);
        assert_eq!(results[0].provider_id.as_deref(), Some("67890"));
    }

    #[test]
    fn typesense_hit_no_document_skipped() {
        let hits = vec![serde_json::json!({"highlight": {}})];
        let results = parse_typesense_hits(&hits);
        assert!(results.is_empty());
    }

    #[test]
    fn typesense_hit_no_authors() {
        let hits = vec![serde_json::json!({
            "document": {
                "id": 1,
                "title": "Orphan Book"
            }
        })];
        let results = parse_typesense_hits(&hits);
        assert!(results[0].authors.is_empty());
    }

    #[test]
    fn typesense_multiple_hits() {
        let hits = vec![
            serde_json::json!({"document": {"id": 1, "title": "A", "author_names": ["X"]}}),
            serde_json::json!({"document": {"id": 2, "title": "B", "author_names": ["Y"]}}),
            serde_json::json!({"document": {"id": 3, "title": "C"}}),
        ];
        let results = parse_typesense_hits(&hits);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].title.as_deref(), Some("A"));
        assert_eq!(results[1].title.as_deref(), Some("B"));
        assert_eq!(results[2].title.as_deref(), Some("C"));
    }

    #[test]
    fn typesense_empty_hits() {
        let results = parse_typesense_hits(&[]);
        assert!(results.is_empty());
    }

    #[test]
    fn typesense_hit_fields_default_to_none() {
        let hits = vec![serde_json::json!({
            "document": {"id": 1, "title": "T"}
        })];
        let results = parse_typesense_hits(&hits);
        assert!(results[0].description.is_none());
        assert!(results[0].publisher.is_none());
        assert!(results[0].isbn.is_none());
        assert!(results[0].cover_data.is_none());
    }

    // --- normalize_key ---

    #[test]
    fn normalize_strips_bearer_prefix() {
        assert_eq!(normalize_key("Bearer abc123"), "abc123");
    }

    #[test]
    fn normalize_strips_lowercase_bearer() {
        assert_eq!(normalize_key("bearer abc123"), "abc123");
    }

    #[test]
    fn normalize_leaves_plain_key_unchanged() {
        assert_eq!(normalize_key("abc123"), "abc123");
    }
}
