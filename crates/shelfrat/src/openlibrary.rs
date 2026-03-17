use serde::Deserialize;

use crate::metadata::ExtractedMetadata;

const OL_SEARCH_URL: &str = "https://openlibrary.org/search.json";
const OL_ISBN_URL: &str = "https://openlibrary.org/isbn";
const OL_COVERS_URL: &str = "https://covers.openlibrary.org/b/isbn";

/// Look up book metadata from Open Library by ISBN.
pub async fn lookup_by_isbn(isbn: &str) -> Option<ExtractedMetadata> {
    let url = format!("{OL_ISBN_URL}/{isbn}.json");

    let resp = reqwest::get(&url).await.ok()?;
    if !resp.status().is_success() {
        return None;
    }

    let book: OlBook = resp.json().await.ok()?;

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

    // Fetch authors from author references
    let authors = resolve_authors(&book.authors.unwrap_or_default()).await;

    // Try to get cover image by ISBN
    let cover_data = fetch_cover_by_isbn(isbn).await;

    Some(ExtractedMetadata {
        title,
        description,
        publisher,
        published_date,
        language,
        isbn: Some(isbn.to_string()),
        authors,
        cover_data,
    })
}

/// Search Open Library by title and optional author.
pub async fn search_by_title(title: &str) -> Option<ExtractedMetadata> {
    let client = reqwest::Client::new();
    let resp = client
        .get(OL_SEARCH_URL)
        .query(&[("title", title), ("limit", "1")])
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let result: OlSearchResult = resp.json().await.ok()?;
    let doc = result.docs.into_iter().next()?;

    let found_title = doc.title.filter(|t| !t.is_empty());
    let authors = doc.author_name.unwrap_or_default();
    let publisher = doc.publisher.and_then(|p| p.into_iter().next());
    let published_date = doc.first_publish_year.map(|y| y.to_string());
    let language = doc.language.and_then(|l| l.into_iter().next());
    let isbn = doc
        .isbn
        .and_then(|isbns| isbns.into_iter().find(|i| i.len() == 13 || i.len() == 10));

    let cover_data = if let Some(ref isbn) = isbn {
        fetch_cover_by_isbn(isbn).await
    } else {
        None
    };

    Some(ExtractedMetadata {
        title: found_title,
        description: None,
        publisher,
        published_date,
        language,
        isbn,
        authors,
        cover_data,
    })
}

/// Fetch a cover image by ISBN from Open Library covers API.
async fn fetch_cover_by_isbn(isbn: &str) -> Option<Vec<u8>> {
    let url = format!("{OL_COVERS_URL}/{isbn}-L.jpg?default=false");
    let resp = reqwest::get(&url).await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    if bytes.len() < 100 {
        // Too small to be a real image
        return None;
    }
    Some(bytes.to_vec())
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
