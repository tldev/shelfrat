use std::fmt;

/// Errors that can occur when calling a metadata provider.
#[derive(Debug)]
pub enum ProviderError {
    /// Transient network error — should retry
    Network(String),
    /// 429 Too Many Requests — should backoff + retry
    RateLimited,
    /// Non-recoverable error (bad API key, etc.) — don't retry
    Fatal(String),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(msg) => write!(f, "network error: {msg}"),
            Self::RateLimited => write!(f, "rate limited (429)"),
            Self::Fatal(msg) => write!(f, "fatal error: {msg}"),
        }
    }
}

impl std::error::Error for ProviderError {}

/// Result type for provider lookups. Empty vec = no results found.
pub type ProviderResult = Result<Vec<crate::metadata::ExtractedMetadata>, ProviderError>;

/// Errors that can occur during the enrichment pipeline.
#[derive(Debug)]
pub enum EnrichError {
    Database(sea_orm::DbErr),
    RateLimited,
    Provider(String),
}

impl fmt::Display for EnrichError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(e) => write!(f, "database error: {e}"),
            Self::RateLimited => write!(f, "rate limited"),
            Self::Provider(msg) => write!(f, "provider error: {msg}"),
        }
    }
}

impl From<sea_orm::DbErr> for EnrichError {
    fn from(e: sea_orm::DbErr) -> Self {
        Self::Database(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ProviderError Display ---

    #[test]
    fn provider_error_network_display() {
        let e = ProviderError::Network("connection reset".into());
        assert_eq!(e.to_string(), "network error: connection reset");
    }

    #[test]
    fn provider_error_rate_limited_display() {
        let e = ProviderError::RateLimited;
        assert_eq!(e.to_string(), "rate limited (429)");
    }

    #[test]
    fn provider_error_fatal_display() {
        let e = ProviderError::Fatal("invalid api key".into());
        assert_eq!(e.to_string(), "fatal error: invalid api key");
    }

    #[test]
    fn provider_error_implements_std_error() {
        let e: Box<dyn std::error::Error> = Box::new(ProviderError::RateLimited);
        assert_eq!(e.to_string(), "rate limited (429)");
    }

    // --- EnrichError Display ---

    #[test]
    fn enrich_error_database_display() {
        let e = EnrichError::Database(sea_orm::DbErr::Custom("disk full".into()));
        assert!(e.to_string().contains("database error:"));
        assert!(e.to_string().contains("disk full"));
    }

    #[test]
    fn enrich_error_rate_limited_display() {
        let e = EnrichError::RateLimited;
        assert_eq!(e.to_string(), "rate limited");
    }

    #[test]
    fn enrich_error_provider_display() {
        let e = EnrichError::Provider("timeout".into());
        assert_eq!(e.to_string(), "provider error: timeout");
    }

    // --- From<DbErr> ---

    #[test]
    fn enrich_error_from_db_err() {
        let db_err = sea_orm::DbErr::Custom("test error".into());
        let enrich_err: EnrichError = db_err.into();
        assert!(matches!(enrich_err, EnrichError::Database(_)));
    }

    // --- Debug ---

    #[test]
    fn provider_error_debug_format() {
        let e = ProviderError::Network("test".into());
        let debug = format!("{:?}", e);
        assert!(debug.contains("Network"));
    }

    #[test]
    fn enrich_error_debug_format() {
        let e = EnrichError::RateLimited;
        let debug = format!("{:?}", e);
        assert!(debug.contains("RateLimited"));
    }
}
