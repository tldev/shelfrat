use serde::Deserialize;

use crate::metadata::ExtractedMetadata;
use crate::provider_error::{ProviderError, ProviderResult};

const OL_SEARCH_URL: &str = "https://openlibrary.org/search.json";
const OL_ISBN_URL: &str = "https://openlibrary.org/isbn";
const OL_COVERS_URL: &str = "https://covers.openlibrary.org/b/isbn";

/// Look up book metadata from Open Library by ISBN.
pub async fn lookup_by_isbn(isbn: &str) -> ProviderResult {
    let url = format!("{OL_ISBN_URL}/{isbn}.json");

    let resp = reqwest::get(&url)
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(ProviderError::RateLimited);
    }
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(vec![]);
    }
    if !resp.status().is_success() {
        return Err(ProviderError::Network(format!("HTTP {}", resp.status())));
    }

    let book: OlBook = resp
        .json()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    let title = book.title.clone().filter(|t| !t.is_empty());
    let description = book.description_text();
    let publisher = book.publishers.and_then(|p| p.into_iter().next());
    let published_date = book.publish_date;
    let language = book.languages.and_then(|langs| {
        langs
            .into_iter()
            .next()
            .map(|l| l.key.trim_start_matches("/languages/").to_string())
    });

    let authors = resolve_authors(&book.authors.unwrap_or_default()).await;
    let cover_data = fetch_cover_by_isbn(isbn).await;

    Ok(vec![ExtractedMetadata {
        title,
        description,
        publisher,
        published_date,
        language,
        isbn: Some(isbn.to_string()),
        authors,
        cover_data,
        provider_id: None,
    }])
}

/// Search Open Library by title, returning up to 5 results without covers.
pub async fn search_by_title(title: &str) -> ProviderResult {
    let client = reqwest::Client::new();
    let resp = client
        .get(OL_SEARCH_URL)
        .query(&[("title", title), ("limit", "5")])
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(ProviderError::RateLimited);
    }
    if !resp.status().is_success() {
        return Err(ProviderError::Network(format!("HTTP {}", resp.status())));
    }

    let result: OlSearchResult = resp
        .json()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    Ok(search_docs_to_metadata(result.docs))
}

/// Search Open Library by title and author, returning up to 5 results without covers.
pub async fn search_by_title_and_author(title: &str, author: &str) -> ProviderResult {
    let client = reqwest::Client::new();
    let resp = client
        .get(OL_SEARCH_URL)
        .query(&[("title", title), ("author", author), ("limit", "5")])
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(ProviderError::RateLimited);
    }
    if !resp.status().is_success() {
        return Err(ProviderError::Network(format!("HTTP {}", resp.status())));
    }

    let result: OlSearchResult = resp
        .json()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    Ok(search_docs_to_metadata(result.docs))
}

/// Fetch a cover image by ISBN from Open Library covers API.
pub async fn fetch_cover_by_isbn(isbn: &str) -> Option<Vec<u8>> {
    let url = format!("{OL_COVERS_URL}/{isbn}-L.jpg?default=false");
    let resp = reqwest::get(&url).await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    if bytes.len() < 100 {
        return None;
    }
    Some(bytes.to_vec())
}

fn search_docs_to_metadata(docs: Vec<OlSearchDoc>) -> Vec<ExtractedMetadata> {
    docs.into_iter()
        .map(|doc| {
            let title = doc.title.filter(|t| !t.is_empty());
            let authors = doc.author_name.unwrap_or_default();
            let publisher = doc.publisher.and_then(|p| p.into_iter().next());
            let published_date = doc.first_publish_year.map(|y| y.to_string());
            let language = doc.language.and_then(|l| l.into_iter().next());
            let isbn = doc
                .isbn
                .and_then(|isbns| isbns.into_iter().find(|i| i.len() == 13 || i.len() == 10));

            ExtractedMetadata {
                title,
                description: None,
                publisher,
                published_date,
                language,
                isbn,
                authors,
                cover_data: None,
                provider_id: None,
            }
        })
        .collect()
}

/// Resolve author names from Open Library author references.
async fn resolve_authors(author_refs: &[OlAuthorRef]) -> Vec<String> {
    let mut authors = Vec::new();
    for author_ref in author_refs.iter().take(5) {
        let key = author_ref.key().unwrap_or_default();
        if key.is_empty() {
            continue;
        }
        let url = format!("https://openlibrary.org{key}.json");
        if let Ok(resp) = reqwest::get(&url).await {
            if let Ok(author) = resp.json::<OlAuthor>().await {
                if let Some(name) = author.name {
                    if !name.is_empty() {
                        authors.push(name);
                    }
                }
            }
        }
    }
    authors
}

// --- Deserialization types ---

#[derive(Debug, Deserialize)]
struct OlBook {
    title: Option<String>,
    description: Option<OlDescription>,
    publishers: Option<Vec<String>>,
    publish_date: Option<String>,
    languages: Option<Vec<OlRef>>,
    authors: Option<Vec<OlAuthorRef>>,
}

impl OlBook {
    fn description_text(&self) -> Option<String> {
        self.description.as_ref().map(|d| match d {
            OlDescription::Text(s) => s.clone(),
            OlDescription::Typed { value, .. } => value.clone(),
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OlDescription {
    Text(String),
    Typed {
        #[serde(rename = "type")]
        _type: Option<String>,
        value: String,
    },
}

#[derive(Debug, Deserialize)]
struct OlRef {
    key: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OlAuthorRef {
    Keyed { key: String },
    AuthorKeyed { author: OlRef },
}

impl OlAuthorRef {
    fn key(&self) -> Option<&str> {
        match self {
            OlAuthorRef::Keyed { key } => Some(key),
            OlAuthorRef::AuthorKeyed { author } => Some(&author.key),
        }
    }
}

#[derive(Debug, Deserialize)]
struct OlAuthor {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OlSearchResult {
    docs: Vec<OlSearchDoc>,
}

#[derive(Debug, Deserialize)]
struct OlSearchDoc {
    title: Option<String>,
    author_name: Option<Vec<String>>,
    publisher: Option<Vec<String>>,
    first_publish_year: Option<i32>,
    language: Option<Vec<String>>,
    isbn: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_doc(
        title: Option<&str>,
        authors: Option<Vec<&str>>,
        isbn: Option<Vec<&str>>,
    ) -> OlSearchDoc {
        OlSearchDoc {
            title: title.map(String::from),
            author_name: authors.map(|a| a.into_iter().map(String::from).collect()),
            publisher: None,
            first_publish_year: None,
            language: None,
            isbn: isbn.map(|i| i.into_iter().map(String::from).collect()),
        }
    }

    // --- search_docs_to_metadata ---

    #[test]
    fn search_docs_empty_input() {
        let result = search_docs_to_metadata(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn search_docs_single_complete() {
        let docs = vec![OlSearchDoc {
            title: Some("Dune".to_string()),
            author_name: Some(vec!["Frank Herbert".to_string()]),
            publisher: Some(vec!["Chilton Books".to_string()]),
            first_publish_year: Some(1965),
            language: Some(vec!["eng".to_string()]),
            isbn: Some(vec!["9780441172719".to_string()]),
        }];
        let result = search_docs_to_metadata(docs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title.as_deref(), Some("Dune"));
        assert_eq!(result[0].authors, vec!["Frank Herbert"]);
        assert_eq!(result[0].publisher.as_deref(), Some("Chilton Books"));
        assert_eq!(result[0].published_date.as_deref(), Some("1965"));
        assert_eq!(result[0].language.as_deref(), Some("eng"));
        assert_eq!(result[0].isbn.as_deref(), Some("9780441172719"));
        assert!(result[0].cover_data.is_none());
        assert!(result[0].description.is_none());
    }

    #[test]
    fn search_docs_empty_title_filtered() {
        let docs = vec![make_doc(Some(""), None, None)];
        let result = search_docs_to_metadata(docs);
        assert!(result[0].title.is_none());
    }

    #[test]
    fn search_docs_missing_fields_are_none() {
        let docs = vec![make_doc(Some("Title"), None, None)];
        let result = search_docs_to_metadata(docs);
        assert!(result[0].authors.is_empty());
        assert!(result[0].publisher.is_none());
        assert!(result[0].published_date.is_none());
        assert!(result[0].language.is_none());
        assert!(result[0].isbn.is_none());
    }

    #[test]
    fn search_docs_multiple_results() {
        let docs = vec![
            make_doc(Some("Book A"), Some(vec!["Author 1"]), None),
            make_doc(Some("Book B"), Some(vec!["Author 2"]), None),
            make_doc(Some("Book C"), None, None),
        ];
        let result = search_docs_to_metadata(docs);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].title.as_deref(), Some("Book A"));
        assert_eq!(result[1].title.as_deref(), Some("Book B"));
        assert_eq!(result[2].title.as_deref(), Some("Book C"));
    }

    #[test]
    fn search_docs_takes_first_valid_isbn() {
        // The function takes the first ISBN with length 10 or 13
        let docs = vec![make_doc(
            Some("Book"),
            None,
            Some(vec!["0123456789", "9780123456789"]),
        )];
        let result = search_docs_to_metadata(docs);
        assert_eq!(result[0].isbn.as_deref(), Some("0123456789"));
    }

    #[test]
    fn search_docs_isbn13_first_selected() {
        let docs = vec![make_doc(
            Some("Book"),
            None,
            Some(vec!["9780123456789", "0123456789"]),
        )];
        let result = search_docs_to_metadata(docs);
        assert_eq!(result[0].isbn.as_deref(), Some("9780123456789"));
    }

    #[test]
    fn search_docs_falls_back_to_isbn10() {
        let docs = vec![make_doc(Some("Book"), None, Some(vec!["0123456789"]))];
        let result = search_docs_to_metadata(docs);
        assert_eq!(result[0].isbn.as_deref(), Some("0123456789"));
    }

    #[test]
    fn search_docs_skips_invalid_isbn_lengths() {
        let docs = vec![make_doc(Some("Book"), None, Some(vec!["12345"]))];
        let result = search_docs_to_metadata(docs);
        assert!(result[0].isbn.is_none());
    }

    #[test]
    fn search_docs_multiple_authors() {
        let docs = vec![make_doc(
            Some("Book"),
            Some(vec!["Author A", "Author B", "Author C"]),
            None,
        )];
        let result = search_docs_to_metadata(docs);
        assert_eq!(result[0].authors, vec!["Author A", "Author B", "Author C"]);
    }

    #[test]
    fn search_docs_first_publisher_selected() {
        let docs = vec![OlSearchDoc {
            title: Some("Book".to_string()),
            author_name: None,
            publisher: Some(vec!["Pub A".to_string(), "Pub B".to_string()]),
            first_publish_year: None,
            language: None,
            isbn: None,
        }];
        let result = search_docs_to_metadata(docs);
        assert_eq!(result[0].publisher.as_deref(), Some("Pub A"));
    }

    // --- OlBook description_text ---

    #[test]
    fn ol_book_description_plain_text() {
        let book = OlBook {
            title: None,
            description: Some(OlDescription::Text("A great book".to_string())),
            publishers: None,
            publish_date: None,
            languages: None,
            authors: None,
        };
        assert_eq!(book.description_text(), Some("A great book".to_string()));
    }

    #[test]
    fn ol_book_description_typed() {
        let book = OlBook {
            title: None,
            description: Some(OlDescription::Typed {
                _type: Some("/type/text".to_string()),
                value: "Typed description".to_string(),
            }),
            publishers: None,
            publish_date: None,
            languages: None,
            authors: None,
        };
        assert_eq!(
            book.description_text(),
            Some("Typed description".to_string())
        );
    }

    #[test]
    fn ol_book_description_none() {
        let book = OlBook {
            title: None,
            description: None,
            publishers: None,
            publish_date: None,
            languages: None,
            authors: None,
        };
        assert_eq!(book.description_text(), None);
    }

    // --- OlAuthorRef::key ---

    #[test]
    fn author_ref_keyed() {
        let r = OlAuthorRef::Keyed {
            key: "/authors/OL1A".to_string(),
        };
        assert_eq!(r.key(), Some("/authors/OL1A"));
    }

    #[test]
    fn author_ref_author_keyed() {
        let r = OlAuthorRef::AuthorKeyed {
            author: OlRef {
                key: "/authors/OL2A".to_string(),
            },
        };
        assert_eq!(r.key(), Some("/authors/OL2A"));
    }
}
