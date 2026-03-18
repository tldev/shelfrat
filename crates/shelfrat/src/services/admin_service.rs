use sea_orm::DatabaseConnection;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::AppError;
use crate::repositories::{audit_repo, book_repo, config_repo};

/// Keys that are exposed through the settings API.
const CONFIGURABLE_KEYS: &[&str] = &[
    "library_path",
    "smtp_host",
    "smtp_port",
    "smtp_user",
    "smtp_password",
    "smtp_from",
    "smtp_encryption",
    "kindle_from_email",
    "job_cadence:library_scan",
    "metadata_retry_hours",
    "app_url",
    "oidc_issuer_url",
    "oidc_client_id",
    "oidc_client_secret",
    "oidc_auto_register",
    "oidc_admin_claim",
    "oidc_admin_value",
    "oidc_provider_name",
    "hardcover_api_key",
    "metadata_providers",
];

pub async fn get_settings(db: &DatabaseConnection) -> Result<Value, AppError> {
    let rows = config_repo::get_all(db).await?;

    let settings: HashMap<String, String> = rows
        .into_iter()
        .filter(|r| {
            CONFIGURABLE_KEYS.contains(&r.key.as_str())
                && r.key != "smtp_password"
                && r.key != "oidc_client_secret"
                && r.key != "hardcover_api_key"
        })
        .map(|r| (r.key, r.value))
        .collect();

    Ok(json!({ "settings": settings }))
}

pub async fn update_settings(
    db: &DatabaseConnection,
    admin_id: i64,
    body: &HashMap<String, String>,
) -> Result<Value, AppError> {
    let mut updated = Vec::new();

    for (key, value) in body {
        if !CONFIGURABLE_KEYS.contains(&key.as_str()) {
            return Err(AppError::BadRequest("unknown setting key".into()));
        }
        if value.is_empty() {
            continue;
        }
        config_repo::set(db, key, value).await?;
        updated.push(key.clone());
    }

    audit_repo::log_action(
        db,
        Some(admin_id),
        "settings_updated",
        Some(&format!("keys updated: {}", updated.join(", "))),
    )
    .await?;

    Ok(json!({
        "message": "settings updated",
        "updated": updated,
    }))
}

pub async fn library_info(
    db: &DatabaseConnection,
    library_path: Option<PathBuf>,
) -> Result<Value, AppError> {
    let stats = book_repo::library_stats(db).await?;

    Ok(json!({
        "library_path": library_path.map(|p| p.to_string_lossy().to_string()),
        "total_books": stats.total_books,
        "available_books": stats.available_books,
        "missing_books": stats.missing_books,
        "total_authors": stats.total_authors,
        "format_breakdown": stats.format_breakdown,
    }))
}

pub async fn query_audit_log(
    db: &DatabaseConnection,
    action: Option<&str>,
    user_id: Option<i64>,
    limit: u64,
    offset: u64,
) -> Result<Value, AppError> {
    let (entries, total) =
        audit_repo::query_with_filters(db, action, user_id, limit, offset).await?;

    Ok(json!({
        "entries": entries,
        "total": total,
        "limit": limit,
        "offset": offset,
    }))
}

pub async fn update_job_cadence(
    db: &DatabaseConnection,
    admin_id: i64,
    job_name: &str,
    seconds: u64,
) -> Result<Value, AppError> {
    let key = format!("job_cadence:{job_name}");
    config_repo::set(db, &key, &seconds.to_string()).await?;

    let detail = format!(
        "job cadence updated: {job_name} = {seconds}s{}",
        if seconds == 0 { " (disabled)" } else { "" }
    );
    audit_repo::log_action(db, Some(admin_id), "job_cadence_updated", Some(&detail)).await?;

    Ok(json!({
        "message": "cadence updated",
        "job": job_name,
        "cadence_seconds": seconds,
        "enabled": seconds > 0,
    }))
}
