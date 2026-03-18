use sea_orm::DatabaseConnection;

use crate::repositories::config_repo;

/// Static mapping from config key to SHELFRAT_* environment variable name.
/// `library_path` is a special case -- it also maps to the legacy `LIBRARY_PATH`.
const ENV_MAPPINGS: &[(&str, &str)] = &[
    ("library_path", "SHELFRAT_LIBRARY_PATH"),
    ("smtp_host", "SHELFRAT_SMTP_HOST"),
    ("smtp_port", "SHELFRAT_SMTP_PORT"),
    ("smtp_user", "SHELFRAT_SMTP_USER"),
    ("smtp_password", "SHELFRAT_SMTP_PASSWORD"),
    ("smtp_from", "SHELFRAT_SMTP_FROM"),
    ("smtp_encryption", "SHELFRAT_SMTP_ENCRYPTION"),
    ("kindle_from_email", "SHELFRAT_KINDLE_FROM_EMAIL"),
    ("app_url", "SHELFRAT_APP_URL"),
    ("oidc_issuer_url", "SHELFRAT_OIDC_ISSUER_URL"),
    ("oidc_client_id", "SHELFRAT_OIDC_CLIENT_ID"),
    ("oidc_client_secret", "SHELFRAT_OIDC_CLIENT_SECRET"),
    ("oidc_auto_register", "SHELFRAT_OIDC_AUTO_REGISTER"),
    ("oidc_admin_claim", "SHELFRAT_OIDC_ADMIN_CLAIM"),
    ("oidc_admin_value", "SHELFRAT_OIDC_ADMIN_VALUE"),
    ("oidc_provider_name", "SHELFRAT_OIDC_PROVIDER_NAME"),
    ("hardcover_api_key", "SHELFRAT_HARDCOVER_API_KEY"),
    ("metadata_providers", "SHELFRAT_METADATA_PROVIDERS"),
    ("metadata_retry_hours", "SHELFRAT_METADATA_RETRY_HOURS"),
    (
        "job_cadence:library_scan",
        "SHELFRAT_JOB_CADENCE_LIBRARY_SCAN",
    ),
];

/// Get a config value. Checks environment variable first, then database.
///
/// This is the single resolution point -- the rest of the codebase should call
/// this and remain ignorant of where the value came from.
pub async fn get(db: &DatabaseConnection, key: &str) -> Option<String> {
    if let Some(val) = get_env(key) {
        return Some(val);
    }

    config_repo::get(db, key)
        .await
        .ok()
        .flatten()
        .filter(|v| !v.is_empty())
}

/// Check env only (no DB). Used internally and by admin_service for the
/// env_locked / write-rejection logic that the rest of the code doesn't need.
fn get_env(key: &str) -> Option<String> {
    if let Some(env_name) = env_var_name(key) {
        if let Ok(val) = std::env::var(env_name) {
            if !val.is_empty() {
                return Some(val);
            }
        }
    }

    // Legacy: LIBRARY_PATH
    if key == "library_path" {
        if let Ok(val) = std::env::var("LIBRARY_PATH") {
            if !val.is_empty() {
                return Some(val);
            }
        }
    }

    None
}

// ── Admin-only helpers (env locking) ──────────────────────────────────

/// Return the SHELFRAT_* env var name for a config key, if one exists.
pub fn env_var_name(key: &str) -> Option<&'static str> {
    ENV_MAPPINGS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
}

/// Return all config keys currently locked by environment variables.
pub fn env_locked_keys() -> Vec<&'static str> {
    ENV_MAPPINGS
        .iter()
        .filter(|(key, _)| get_env(key).is_some())
        .map(|(key, _)| *key)
        .collect()
}

/// Returns true if the given key is set via an environment variable.
pub fn is_env_locked(key: &str) -> bool {
    get_env(key).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_var_name_known_keys() {
        assert_eq!(env_var_name("smtp_host"), Some("SHELFRAT_SMTP_HOST"));
        assert_eq!(
            env_var_name("job_cadence:library_scan"),
            Some("SHELFRAT_JOB_CADENCE_LIBRARY_SCAN")
        );
    }

    #[test]
    fn env_var_name_unknown_key() {
        assert_eq!(env_var_name("unknown_key"), None);
    }

    #[test]
    fn get_env_returns_none_when_unset() {
        assert!(get_env("smtp_host").is_none());
    }
}
