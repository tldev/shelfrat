use crate::metadata::ExtractedMetadata;
use crate::repositories::metadata_repo::MetaLookup;

/// Query data used for scoring search results.
pub struct SearchQuery {
    pub title: Option<String>,
    pub author: Option<String>,
    pub isbn: Option<String>,
}

impl SearchQuery {
    pub fn from_lookup(lookup: &MetaLookup) -> Self {
        Self {
            title: lookup.title.clone(),
            author: lookup.first_author.clone(),
            isbn: lookup.isbn_13.clone().or_else(|| lookup.isbn_10.clone()),
        }
    }
}

/// Score and rank search results, returning them sorted best-first.
pub fn rank_results(query: &SearchQuery, results: &[ExtractedMetadata]) -> Vec<ExtractedMetadata> {
    let mut scored: Vec<(f64, ExtractedMetadata)> = results
        .iter()
        .map(|r| (score(query, r), r.clone()))
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().map(|(_, r)| r).collect()
}

fn score(query: &SearchQuery, result: &ExtractedMetadata) -> f64 {
    let title_score = match (&query.title, &result.title) {
        (Some(qt), Some(rt)) => strsim::jaro_winkler(&qt.to_lowercase(), &rt.to_lowercase()),
        _ => 0.0,
    };

    let author_score = match &query.author {
        Some(qa) => {
            if result.authors.is_empty() {
                0.0
            } else {
                result
                    .authors
                    .iter()
                    .map(|ra| strsim::jaro_winkler(&qa.to_lowercase(), &ra.to_lowercase()))
                    .fold(0.0_f64, f64::max)
            }
        }
        None => 0.0,
    };

    let isbn_score = match &query.isbn {
        Some(qi) => {
            if result.isbn.as_deref() == Some(qi) {
                1.0
            } else {
                0.0
            }
        }
        None => 0.0,
    };

    let total_fields = 7.0;
    let mut filled = 0.0;
    if result.title.is_some() {
        filled += 1.0;
    }
    if result.description.is_some() {
        filled += 1.0;
    }
    if result.publisher.is_some() {
        filled += 1.0;
    }
    if result.published_date.is_some() {
        filled += 1.0;
    }
    if result.language.is_some() {
        filled += 1.0;
    }
    if result.isbn.is_some() {
        filled += 1.0;
    }
    if !result.authors.is_empty() {
        filled += 1.0;
    }
    let completeness_score = filled / total_fields;

    0.35 * title_score + 0.30 * author_score + 0.20 * isbn_score + 0.15 * completeness_score
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_meta(title: &str, authors: Vec<&str>, isbn: Option<&str>) -> ExtractedMetadata {
        ExtractedMetadata {
            title: Some(title.to_string()),
            authors: authors.into_iter().map(String::from).collect(),
            isbn: isbn.map(String::from),
            ..Default::default()
        }
    }

    #[test]
    fn exact_match_ranks_first() {
        let query = SearchQuery {
            title: Some("Dune".to_string()),
            author: Some("Frank Herbert".to_string()),
            isbn: None,
        };

        let results = vec![
            make_meta("Dune Messiah", vec!["Frank Herbert"], None),
            make_meta("Dune", vec!["Frank Herbert"], None),
            make_meta("The Dune Encyclopedia", vec!["Willis McNelly"], None),
        ];

        let ranked = rank_results(&query, &results);
        assert_eq!(ranked[0].title.as_deref(), Some("Dune"));
    }

    #[test]
    fn isbn_match_boosts_score() {
        let query = SearchQuery {
            title: Some("The Road".to_string()),
            author: None,
            isbn: Some("9780307387899".to_string()),
        };

        let results = vec![
            make_meta("The Road", vec!["Jack Kerouac"], None),
            make_meta("The Road", vec!["Cormac McCarthy"], Some("9780307387899")),
        ];

        let ranked = rank_results(&query, &results);
        assert_eq!(ranked[0].isbn.as_deref(), Some("9780307387899"));
    }

    #[test]
    fn empty_results_returns_empty() {
        let query = SearchQuery {
            title: Some("Test".to_string()),
            author: None,
            isbn: None,
        };
        let ranked = rank_results(&query, &[]);
        assert!(ranked.is_empty());
    }

    #[test]
    fn completeness_breaks_ties() {
        let query = SearchQuery {
            title: Some("Test Book".to_string()),
            author: None,
            isbn: None,
        };

        let sparse = make_meta("Test Book", vec![], None);
        let mut rich = make_meta("Test Book", vec!["Author"], None);
        rich.description = Some("A description".to_string());
        rich.publisher = Some("Publisher".to_string());

        let ranked = rank_results(&query, &[sparse, rich]);
        assert!(ranked[0].description.is_some());
    }

    #[test]
    fn author_similarity_matters() {
        let query = SearchQuery {
            title: Some("It".to_string()),
            author: Some("Stephen King".to_string()),
            isbn: None,
        };

        let results = vec![
            make_meta("It", vec!["Random Author"], None),
            make_meta("It", vec!["Stephen King"], None),
        ];

        let ranked = rank_results(&query, &results);
        assert_eq!(ranked[0].authors[0], "Stephen King");
    }

    #[test]
    fn single_result_unchanged() {
        let query = SearchQuery {
            title: Some("Test".to_string()),
            author: None,
            isbn: None,
        };

        let results = vec![make_meta("Test", vec![], None)];
        let ranked = rank_results(&query, &results);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].title.as_deref(), Some("Test"));
    }
}
