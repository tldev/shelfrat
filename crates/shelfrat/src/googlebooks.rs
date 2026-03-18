use serde::Deserialize;

use crate::metadata::ExtractedMetadata;

const GB_VOLUMES_URL: &str = "https://www.googleapis.com/books/v1/volumes";

/// Look up book metadata from Google Books by ISBN.
pub async fn lookup_by_isbn(isbn: &str) -> Option<ExtractedMetadata> {
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

    volume_to_metadata(vol, Some(isbn.to_string())).await
}

/// Search Google Books by title.
pub async fn search_by_title(title: &str) -> Option<ExtractedMetadata> {
    let client = reqwest::Client::new();
    let resp = client
        .get(GB_VOLUMES_URL)
        .query(&[
            ("q", &format!("intitle:{title}")),
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

    volume_to_metadata(vol, None).await
}

/// Convert a Google Books VolumeInfo to our ExtractedMetadata.
async fn volume_to_metadata(
    vol: GbVolumeInfo,
    search_isbn: Option<String>,
) -> Option<ExtractedMetadata> {
    let title = vol.title.filter(|t| !t.is_empty());
    let description = vol.description.filter(|d| !d.is_empty());
    let publisher = vol.publisher.filter(|p| !p.is_empty());
    let published_date = vol.published_date.filter(|d| !d.is_empty());
    let language = vol.language.filter(|l| !l.is_empty());
    let authors = vol.authors.unwrap_or_default();

    // Extract ISBN from industry identifiers, preferring ISBN_13
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

    // Try to fetch cover image from Google Books thumbnail URL
    let cover_data = if let Some(ref links) = vol.image_links {
        // Prefer largest available: extraLarge > large > medium > small > thumbnail
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
    })
}

/// Fetch a cover image from a Google Books image URL.
async fn fetch_cover(url: &str) -> Option<Vec<u8>> {
    // Google Books URLs sometimes use http; upgrade to https
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
