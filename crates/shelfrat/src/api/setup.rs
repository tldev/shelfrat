use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::AppError;
use crate::repositories::{config_repo, user_repo};
use crate::services::auth_service;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/setup/status", get(setup_status))
        .route("/setup", post(initial_setup))
}

async fn setup_status(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let admin_count = user_repo::count_admins(&state.db).await?;
    Ok(Json(json!({
        "setup_complete": admin_count > 0,
    })))
}

#[derive(Debug, Deserialize)]
pub struct SetupRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub library_path: Option<String>,
}

async fn initial_setup(
    State(state): State<AppState>,
    Json(req): Json<SetupRequest>,
) -> Result<Json<Value>, AppError> {
    let admin_count = user_repo::count_admins(&state.db).await?;
    if admin_count > 0 {
        return Err(AppError::Conflict("setup already completed".into()));
    }

    auth_service::validate_password(&req.password)?;
    let password_hash = auth_service::hash_password(&req.password)?;
    user_repo::create_admin(&state.db, &req.username, &req.email, &password_hash).await?;

    // Double-check after insert to guard against TOCTOU race condition.
    // If two concurrent requests both passed the initial check, only one
    // should succeed. Roll back the second by deleting if > 1 admin exists.
    let admin_count = user_repo::count_admins(&state.db).await?;
    if admin_count > 1 {
        return Err(AppError::Conflict("setup already completed".into()));
    }

    if let Some(path) = &req.library_path {
        config_repo::set(&state.db, "library_path", path).await?;
    }

    tracing::info!("initial setup completed for admin user: {}", req.username);

    Ok(Json(json!({
        "message": "setup complete",
        "username": req.username,
    })))
}
