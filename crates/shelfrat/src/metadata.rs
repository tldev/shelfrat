use std::path::Path;

/// Metadata extracted from an ebook file's embedded data.
#[derive(Debug, Clone, Default)]
pub struct ExtractedMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub publisher: Option<String>,
    pub published_date: Option<String>,
    pub language: Option<String>,
    pub isbn: Option<String>,
    pub authors: Vec<String>,
    pub cover_data: Option<Vec<u8>>,
    /// Provider-specific identifier for follow-up queries (not persisted to DB).
    pub provider_id: Option<String>,
}

/// Extract metadata from an ebook file based on its format.
pub fn extract(path: &Path, format: &str) -> Option<ExtractedMetadata> {
    match format {
        "epub" => extract_epub(path),
        // Future: "pdf" => extract_pdf(path),
        _ => None,
    }
}

/// Helper to extract a metadata value as a non-empty String.
fn mdata_str(
    doc: &epub::doc::EpubDoc<std::io::BufReader<std::fs::File>>,
    key: &str,
) -> Option<String> {
    doc.mdata(key)
        .map(|item| item.value.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn extract_epub(path: &Path) -> Option<ExtractedMetadata> {
    let doc = epub::doc::EpubDoc::new(path).ok()?;

    let title = mdata_str(&doc, "title");
    let description = mdata_str(&doc, "description");
    let publisher = mdata_str(&doc, "publisher");
    let published_date = mdata_str(&doc, "date");
    let language = mdata_str(&doc, "language");

    // Try to find ISBN from identifiers
    let isbn = mdata_str(&doc, "identifier").filter(|id| looks_like_isbn(id));

    // Collect authors from "creator" metadata entries
    let authors: Vec<String> = doc
        .metadata
        .iter()
        .filter(|m| m.property == "creator")
        .map(|m| m.value.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Try to get cover image
    let cover_data = {
        let mut doc = epub::doc::EpubDoc::new(path).ok()?;
        doc.get_cover().map(|(data, _mime)| data)
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

/// Heuristic: does this string look like an ISBN-10 or ISBN-13?
fn looks_like_isbn(s: &str) -> bool {
    let digits: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == 'X')
        .collect();
    digits.len() == 10 || digits.len() == 13
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── looks_like_isbn ────────────────────────────────────────────

    #[test]
    fn isbn10_plain_digits() {
        assert!(looks_like_isbn("0123456789"));
    }

    #[test]
    fn isbn10_with_x() {
        assert!(looks_like_isbn("012345678X"));
    }

    #[test]
    fn isbn13_plain_digits() {
        assert!(looks_like_isbn("9780123456789"));
    }

    #[test]
    fn isbn13_with_dashes() {
        // Dashes are filtered out, leaving 13 digits.
        assert!(looks_like_isbn("978-0-12-345678-9"));
    }

    #[test]
    fn isbn_too_short() {
        assert!(!looks_like_isbn("12345"));
        assert!(!looks_like_isbn("123456789")); // 9 digits
    }

    #[test]
    fn isbn_too_long() {
        assert!(!looks_like_isbn("97801234567890")); // 14 digits
    }

    #[test]
    fn isbn_non_digits() {
        assert!(!looks_like_isbn("abcdefghij"));
    }

    #[test]
    fn isbn_empty() {
        assert!(!looks_like_isbn(""));
    }

    #[test]
    fn isbn_11_digits_invalid() {
        assert!(!looks_like_isbn("01234567890")); // 11 digits
    }

    #[test]
    fn isbn_12_digits_invalid() {
        assert!(!looks_like_isbn("012345678901")); // 12 digits
    }

    // ── extract with non-epub format ───────────────────────────────

    #[test]
    fn extract_pdf_returns_none() {
        let result = extract(std::path::Path::new("/fake/book.pdf"), "pdf");
        assert!(result.is_none());
    }

    #[test]
    fn extract_mobi_returns_none() {
        let result = extract(std::path::Path::new("/fake/book.mobi"), "mobi");
        assert!(result.is_none());
    }

    #[test]
    fn extract_unknown_format_returns_none() {
        let result = extract(std::path::Path::new("/fake/book.xyz"), "xyz");
        assert!(result.is_none());
    }
}
