use sea_orm::DatabaseConnection;
use serde::Serialize;

use crate::repositories::{audit_repo, config_repo, metadata_repo};

/// All known provider identifiers.
const ALL_PROVIDERS: &[&str] = &["openlibrary", "googlebooks", "hardcover"];

/// Default enabled providers when none are configured.
const DEFAULT_PROVIDERS: &[&str] = &["openlibrary", "googlebooks"];

#[derive(Debug, Serialize)]
pub struct ProviderInfo {
    pub name: String,
    pub enabled: bool,
    pub order: usize,
    pub requires_key: bool,
    pub key_configured: bool,
}

/// Get the ordered list of enabled provider names from config.
pub async fn get_enabled_providers(db: &DatabaseConnection) -> Vec<String> {
    let raw = config_repo::get(db, "metadata_providers")
        .await
        .ok()
        .flatten();

    match raw {
        Some(json_str) => serde_json::from_str::<Vec<String>>(&json_str)
            .unwrap_or_else(|_| DEFAULT_PROVIDERS.iter().map(|s| s.to_string()).collect()),
        None => DEFAULT_PROVIDERS.iter().map(|s| s.to_string()).collect(),
    }
}

/// Get full provider config for the admin UI.
pub async fn get_provider_config(db: &DatabaseConnection) -> Vec<ProviderInfo> {
    let enabled = get_enabled_providers(db).await;
    let key_set = config_repo::get(db, "hardcover_api_key")
        .await
        .ok()
        .flatten()
        .map(|k| !k.is_empty())
        .unwrap_or(false);

    let mut result: Vec<ProviderInfo> = Vec::new();

    // Add enabled providers in order
    for (i, name) in enabled.iter().enumerate() {
        if ALL_PROVIDERS.contains(&name.as_str()) {
            result.push(ProviderInfo {
                name: name.clone(),
                enabled: true,
                order: i,
                requires_key: name == "hardcover",
                key_configured: name != "hardcover" || key_set,
            });
        }
    }

    // Add disabled providers
    for &name in ALL_PROVIDERS {
        if !enabled.contains(&name.to_string()) {
            result.push(ProviderInfo {
                name: name.to_string(),
                enabled: false,
                order: result.len(),
                requires_key: name == "hardcover",
                key_configured: name != "hardcover" || key_set,
            });
        }
    }

    result
}

/// Update the ordered list of enabled providers.
pub async fn update_provider_order(
    db: &DatabaseConnection,
    admin_id: i64,
    providers: Vec<String>,
) -> Result<(), String> {
    // Validate all names are known
    for name in &providers {
        if !ALL_PROVIDERS.contains(&name.as_str()) {
            return Err(format!("unknown provider: {name}"));
        }
    }

    // Reject hardcover if key not configured
    if providers.contains(&"hardcover".to_string()) {
        let key_set = config_repo::get(db, "hardcover_api_key")
            .await
            .ok()
            .flatten()
            .map(|k| !k.is_empty())
            .unwrap_or(false);
        if !key_set {
            return Err("configure hardcover API key before enabling".to_string());
        }
    }

    let json = serde_json::to_string(&providers).map_err(|e| e.to_string())?;
    config_repo::set(db, "metadata_providers", &json)
        .await
        .map_err(|e| e.to_string())?;

    audit_repo::log_action(
        db,
        Some(admin_id),
        "settings_updated",
        Some(&format!(
            "metadata providers updated: {}",
            providers.join(", ")
        )),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Test and save a Hardcover API key.
pub async fn save_hardcover_key(
    db: &DatabaseConnection,
    admin_id: i64,
    api_key: &str,
) -> Result<(), String> {
    crate::hardcover::test_api_key(api_key).await?;

    config_repo::set(db, "hardcover_api_key", api_key)
        .await
        .map_err(|e| e.to_string())?;

    audit_repo::log_action(
        db,
        Some(admin_id),
        "settings_updated",
        Some("hardcover API key configured"),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Clear provider attempt records so books can be re-queried.
pub async fn reset_provider(
    db: &DatabaseConnection,
    admin_id: i64,
    provider: &str,
) -> Result<u64, String> {
    if !ALL_PROVIDERS.contains(&provider) {
        return Err(format!("unknown provider: {provider}"));
    }

    let cleared = metadata_repo::clear_provider_attempts(db, provider)
        .await
        .map_err(|e| e.to_string())?;

    audit_repo::log_action(
        db,
        Some(admin_id),
        "settings_updated",
        Some(&format!(
            "reset {provider} attempts ({cleared} records cleared)"
        )),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(cleared)
}
