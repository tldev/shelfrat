use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

use crate::auth::AdminUser;
use crate::error::AppError;
use crate::services::admin_service;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/audit-log", get(query_audit_log))
        .route("/admin/settings", get(get_settings).put(update_settings))
        .route("/admin/library-info", get(library_info))
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
