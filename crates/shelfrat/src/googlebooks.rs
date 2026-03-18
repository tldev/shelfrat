use serde::Deserialize;

use crate::metadata::ExtractedMetadata;
use crate::provider_error::{ProviderError, ProviderResult};

const GB_VOLUMES_URL: &str = "https://www.googleapis.com/books/v1/volumes";

/// Look up book metadata from Google Books by ISBN.
pub async fn lookup_by_isbn(isbn: &str) -> ProviderResult {
    let client = reqwest::Client::new();
    let resp = client
        .get(GB_VOLUMES_URL)
        .query(&[
            ("q", &format!("isbn:{isbn}")),
            ("maxResults", &"1".to_string()),
        ])
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(ProviderError::RateLimited);
    }
    if !resp.status().is_success() {
        return Err(ProviderError::Network(format!("HTTP {}", resp.status())));
    }

    let result: GbSearchResult = resp
        .json()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    let Some(items) = result.items else {
        return Ok(vec![]);
    };
    let Some(item) = items.into_iter().next() else {
        return Ok(vec![]);
    };
    let Some(vol) = item.volume_info else {
        return Ok(vec![]);
    };

    match volume_to_metadata(vol, Some(isbn.to_string()), true).await {
        Some(meta) => Ok(vec![meta]),
        None => Ok(vec![]),
    }
}

/// Search Google Books by title, returning up to 5 results without covers.
pub async fn search_by_title(title: &str) -> ProviderResult {
    let client = reqwest::Client::new();
    let resp = client
        .get(GB_VOLUMES_URL)
        .query(&[
            ("q", &format!("intitle:{title}")),
            ("maxResults", &"5".to_string()),
        ])
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(ProviderError::RateLimited);
    }
    if !resp.status().is_success() {
        return Err(ProviderError::Network(format!("HTTP {}", resp.status())));
    }

    let result: GbSearchResult = resp
        .json()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    let items = result.items.unwrap_or_default();
    let mut results = Vec::new();
    for item in items {
        if let Some(vol) = item.volume_info {
            if let Some(meta) = volume_to_metadata(vol, None, false).await {
                results.push(meta);
            }
        }
    }

    Ok(results)
}

/// Search Google Books by title and author, returning up to 5 results without covers.
pub async fn search_by_title_and_author(title: &str, author: &str) -> ProviderResult {
    let client = reqwest::Client::new();
    let resp = client
        .get(GB_VOLUMES_URL)
        .query(&[
            ("q", &format!("intitle:{title} inauthor:{author}")),
            ("maxResults", &"5".to_string()),
        ])
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(ProviderError::RateLimited);
    }
    if !resp.status().is_success() {
        return Err(ProviderError::Network(format!("HTTP {}", resp.status())));
    }

    let result: GbSearchResult = resp
        .json()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    let items = result.items.unwrap_or_default();
    let mut results = Vec::new();
    for item in items {
        if let Some(vol) = item.volume_info {
            if let Some(meta) = volume_to_metadata(vol, None, false).await {
                results.push(meta);
            }
        }
    }

    Ok(results)
}

/// Fetch a cover image by ISBN from Google Books.
pub async fn fetch_cover_by_isbn(isbn: &str) -> Option<Vec<u8>> {
    let client = reqwest::Client::new();
    let resp = client
        .get(GB_VOLUMES_URL)
        .query(&[
            ("q", &format!("isbn:{isbn}")),
            ("maxResults", &"1".to_string()),
        ])
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let result: GbSearchResult = resp.json().await.ok()?;
    let item = result.items?.into_iter().next()?;
    let vol = item.volume_info?;

    if let Some(ref links) = vol.image_links {
        let url = links
            .extra_large
            .as_ref()
            .or(links.large.as_ref())
            .or(links.medium.as_ref())
            .or(links.small.as_ref())
            .or(links.thumbnail.as_ref());
        if let Some(url) = url {
            return fetch_cover(url).await;
        }
    }

    None
}

/// Convert a Google Books VolumeInfo to our ExtractedMetadata.
async fn volume_to_metadata(
    vol: GbVolumeInfo,
    search_isbn: Option<String>,
    with_cover: bool,
) -> Option<ExtractedMetadata> {
    let title = vol.title.filter(|t| !t.is_empty());
    let description = vol.description.filter(|d| !d.is_empty());
    let publisher = vol.publisher.filter(|p| !p.is_empty());
    let published_date = vol.published_date.filter(|d| !d.is_empty());
    let language = vol.language.filter(|l| !l.is_empty());
    let authors = vol.authors.unwrap_or_default();

    let isbn = vol
        .industry_identifiers
        .as_ref()
        .and_then(|ids| {
            ids.iter()
                .find(|id| id.identifier_type == "ISBN_13")
                .or_else(|| ids.iter().find(|id| id.identifier_type == "ISBN_10"))
                .map(|id| id.identifier.clone())
        })
        .or(search_isbn);

    let cover_data = if with_cover {
        if let Some(ref links) = vol.image_links {
            let url = links
                .extra_large
                .as_ref()
                .or(links.large.as_ref())
                .or(links.medium.as_ref())
                .or(links.small.as_ref())
                .or(links.thumbnail.as_ref());
            if let Some(url) = url {
                fetch_cover(url).await
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    Some(ExtractedMetadata {
        title,
        description,
        publisher,
        published_date,
        language,
        isbn,
        authors,
        cover_data,
        provider_id: None,
    })
}

/// Fetch a cover image from a Google Books image URL.
async fn fetch_cover(url: &str) -> Option<Vec<u8>> {
    let url = url.replace("http://", "https://");
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

// --- Deserialization types ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GbSearchResult {
    items: Option<Vec<GbItem>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GbItem {
    volume_info: Option<GbVolumeInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GbVolumeInfo {
    title: Option<String>,
    authors: Option<Vec<String>>,
    publisher: Option<String>,
    published_date: Option<String>,
    description: Option<String>,
    industry_identifiers: Option<Vec<GbIdentifier>>,
    image_links: Option<GbImageLinks>,
    language: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GbIdentifier {
    #[serde(rename = "type")]
    identifier_type: String,
    identifier: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GbImageLinks {
    thumbnail: Option<String>,
    small: Option<String>,
    medium: Option<String>,
    large: Option<String>,
    extra_large: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_volume(
        title: Option<&str>,
        authors: Option<Vec<&str>>,
        isbn13: Option<&str>,
    ) -> GbVolumeInfo {
        let identifiers = isbn13.map(|isbn| {
            vec![GbIdentifier {
                identifier_type: "ISBN_13".to_string(),
                identifier: isbn.to_string(),
            }]
        });
        GbVolumeInfo {
            title: title.map(String::from),
            authors: authors.map(|a| a.into_iter().map(String::from).collect()),
            publisher: None,
            published_date: None,
            description: None,
            industry_identifiers: identifiers,
            image_links: None,
            language: None,
        }
    }

    // --- volume_to_metadata (with_cover = false) ---

    #[tokio::test]
    async fn volume_basic_fields() {
        let vol = GbVolumeInfo {
            title: Some("The Road".to_string()),
            authors: Some(vec!["Cormac McCarthy".to_string()]),
            publisher: Some("Knopf".to_string()),
            published_date: Some("2006-09-26".to_string()),
            description: Some("A post-apocalyptic tale".to_string()),
            industry_identifiers: None,
            image_links: None,
            language: Some("en".to_string()),
        };
        let meta = volume_to_metadata(vol, None, false).await.unwrap();
        assert_eq!(meta.title.as_deref(), Some("The Road"));
        assert_eq!(meta.authors, vec!["Cormac McCarthy"]);
        assert_eq!(meta.publisher.as_deref(), Some("Knopf"));
        assert_eq!(meta.published_date.as_deref(), Some("2006-09-26"));
        assert_eq!(meta.description.as_deref(), Some("A post-apocalyptic tale"));
        assert_eq!(meta.language.as_deref(), Some("en"));
        assert!(meta.cover_data.is_none());
    }

    #[tokio::test]
    async fn volume_empty_strings_filtered() {
        let vol = GbVolumeInfo {
            title: Some("".to_string()),
            authors: None,
            publisher: Some("".to_string()),
            published_date: Some("".to_string()),
            description: Some("".to_string()),
            industry_identifiers: None,
            image_links: None,
            language: Some("".to_string()),
        };
        let meta = volume_to_metadata(vol, None, false).await.unwrap();
        assert!(meta.title.is_none());
        assert!(meta.publisher.is_none());
        assert!(meta.published_date.is_none());
        assert!(meta.description.is_none());
        assert!(meta.language.is_none());
    }

    #[tokio::test]
    async fn volume_isbn13_preferred() {
        let vol = GbVolumeInfo {
            title: Some("Book".to_string()),
            authors: None,
            publisher: None,
            published_date: None,
            description: None,
            industry_identifiers: Some(vec![
                GbIdentifier {
                    identifier_type: "ISBN_10".to_string(),
                    identifier: "0123456789".to_string(),
                },
                GbIdentifier {
                    identifier_type: "ISBN_13".to_string(),
                    identifier: "9780123456789".to_string(),
                },
            ]),
            image_links: None,
            language: None,
        };
        let meta = volume_to_metadata(vol, None, false).await.unwrap();
        assert_eq!(meta.isbn.as_deref(), Some("9780123456789"));
    }

    #[tokio::test]
    async fn volume_isbn10_fallback() {
        let vol = GbVolumeInfo {
            title: Some("Book".to_string()),
            authors: None,
            publisher: None,
            published_date: None,
            description: None,
            industry_identifiers: Some(vec![GbIdentifier {
                identifier_type: "ISBN_10".to_string(),
                identifier: "0123456789".to_string(),
            }]),
            image_links: None,
            language: None,
        };
        let meta = volume_to_metadata(vol, None, false).await.unwrap();
        assert_eq!(meta.isbn.as_deref(), Some("0123456789"));
    }

    #[tokio::test]
    async fn volume_search_isbn_fallback() {
        let vol = make_volume(Some("Book"), None, None);
        let meta = volume_to_metadata(vol, Some("9999999999999".to_string()), false)
            .await
            .unwrap();
        assert_eq!(meta.isbn.as_deref(), Some("9999999999999"));
    }

    #[tokio::test]
    async fn volume_no_isbn_anywhere() {
        let vol = make_volume(Some("Book"), None, None);
        let meta = volume_to_metadata(vol, None, false).await.unwrap();
        assert!(meta.isbn.is_none());
    }

    #[tokio::test]
    async fn volume_no_cover_when_flag_false() {
        let vol = GbVolumeInfo {
            title: Some("Book".to_string()),
            authors: None,
            publisher: None,
            published_date: None,
            description: None,
            industry_identifiers: None,
            image_links: Some(GbImageLinks {
                thumbnail: Some("https://example.com/thumb.jpg".to_string()),
                small: None,
                medium: None,
                large: None,
                extra_large: None,
            }),
            language: None,
        };
        let meta = volume_to_metadata(vol, None, false).await.unwrap();
        assert!(meta.cover_data.is_none());
    }

    #[tokio::test]
    async fn volume_missing_authors_empty_vec() {
        let vol = make_volume(Some("Book"), None, None);
        let meta = volume_to_metadata(vol, None, false).await.unwrap();
        assert!(meta.authors.is_empty());
    }

    #[tokio::test]
    async fn volume_multiple_authors() {
        let vol = make_volume(Some("Book"), Some(vec!["A", "B", "C"]), None);
        let meta = volume_to_metadata(vol, None, false).await.unwrap();
        assert_eq!(meta.authors, vec!["A", "B", "C"]);
    }

    #[tokio::test]
    async fn volume_provider_id_always_none() {
        let vol = make_volume(Some("Book"), None, None);
        let meta = volume_to_metadata(vol, None, false).await.unwrap();
        assert!(meta.provider_id.is_none());
    }
}
