use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::auth::AdminUser;
use crate::error::AppError;
use crate::services::{admin_service, provider_service};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/audit-log", get(query_audit_log))
        .route("/admin/settings", get(get_settings).put(update_settings))
        .route("/admin/library-info", get(library_info))
        .route("/admin/providers", get(get_providers).put(update_providers))
        .route("/admin/providers/test-hardcover", post(test_hardcover_key))
        .route("/admin/providers/{name}/reset", post(reset_provider))
}

#[derive(Debug, Deserialize)]
struct AuditLogParams {
    action: Option<String>,
    user_id: Option<i64>,
    limit: Option<u64>,
    offset: Option<u64>,
}

async fn query_audit_log(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(params): Query<AuditLogParams>,
) -> Result<Json<Value>, AppError> {
    let limit = params.limit.unwrap_or(50).min(500);
    let offset = params.offset.unwrap_or(0);
    let result = admin_service::query_audit_log(
        &state.db,
        params.action.as_deref(),
        params.user_id,
        limit,
        offset,
    )
    .await?;
    Ok(Json(result))
}

async fn get_settings(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let result = admin_service::get_settings(&state.db).await?;
    Ok(Json(result))
}

async fn update_settings(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let result = admin_service::update_settings(&state.db, admin.id, &body).await?;
    Ok(Json(result))
}

async fn library_info(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let library_path = state.resolve_library_path().await;
    let result = admin_service::library_info(&state.db, library_path).await?;
    Ok(Json(result))
}

async fn get_providers(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let providers = provider_service::get_provider_config(&state.db).await;
    Ok(Json(json!({ "providers": providers })))
}

#[derive(Debug, Deserialize)]
struct UpdateProvidersBody {
    providers: Vec<String>,
}

async fn update_providers(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<UpdateProvidersBody>,
) -> Result<Json<Value>, AppError> {
    provider_service::update_provider_order(&state.db, admin.id, body.providers)
        .await
        .map_err(AppError::BadRequest)?;
    Ok(Json(json!({ "message": "providers updated" })))
}

#[derive(Debug, Deserialize)]
struct TestHardcoverBody {
    api_key: String,
}

async fn test_hardcover_key(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<TestHardcoverBody>,
) -> Result<Json<Value>, AppError> {
    provider_service::save_hardcover_key(&state.db, admin.id, &body.api_key)
        .await
        .map_err(AppError::BadRequest)?;
    Ok(Json(
        json!({ "message": "hardcover API key validated and saved" }),
    ))
}

async fn reset_provider(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, AppError> {
    let cleared = provider_service::reset_provider(&state.db, admin.id, &name)
        .await
        .map_err(AppError::BadRequest)?;
    Ok(Json(json!({
        "message": format!("{name} reset"),
        "cleared": cleared,
    })))
}
