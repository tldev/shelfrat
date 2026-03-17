use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::auth::AdminUser;
use crate::error::AppError;
use crate::services::scan_service;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/library/scan", post(full_scan))
        .route("/library/reindex", post(rebuild_fts))
}

async fn full_scan(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let library_path = state.resolve_library_path().await;
    let result =
        scan_service::full_scan(&state.db, &state.pool, library_path, &state.meta_queue).await?;
    Ok(Json(result))
}

async fn rebuild_fts(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("rebuilding FTS index");
    let count = crate::fts::rebuild_fts_index(&state.pool)
        .await
        .map_err(|e| AppError::Internal(format!("FTS rebuild failed: {e}")))?;

    tracing::info!("FTS index rebuilt for {count} books");
    Ok(Json(json!({ "indexed": count })))
}
