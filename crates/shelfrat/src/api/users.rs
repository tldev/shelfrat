use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use crate::auth::{AdminUser, AuthUser};
use crate::error::AppError;
use crate::services::user_service;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/users", get(list_users))
        .route("/users/invite", post(create_invite))
        .route("/users/register/{token}", post(register_with_invite))
        .route(
            "/users/{id}",
            get(get_user).put(update_user).delete(revoke_user),
        )
}

async fn list_users(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let result = user_service::list_users(&state.db).await?;
    Ok(Json(result))
}

async fn get_user(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, AppError> {
    if auth_user.role != "admin" && auth_user.id != id {
        return Err(AppError::Forbidden);
    }
    let result = user_service::get_user(&state.db, id).await?;
    Ok(Json(result))
}

async fn create_invite(
    admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let result = user_service::create_invite(&state.db, admin.id, &admin.username).await?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    username: String,
    email: String,
    password: String,
}

async fn register_with_invite(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<Value>, AppError> {
    let result = user_service::register_with_invite(
        &state.db,
        &token,
        &req.username,
        &req.email,
        &req.password,
    )
    .await?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct UpdateUserRequest {
    display_name: Option<String>,
    email: Option<String>,
    kindle_email: Option<String>,
    current_password: Option<String>,
    new_password: Option<String>,
    role: Option<String>,
}

async fn update_user(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateUserRequest>,
) -> Result<Json<Value>, AppError> {
    if auth_user.role != "admin" && auth_user.id != id {
        return Err(AppError::Forbidden);
    }
    let result = user_service::update_user(
        &state.db,
        id,
        auth_user.id,
        &auth_user.role,
        &auth_user.username,
        req.display_name.as_deref(),
        req.email.as_deref(),
        req.kindle_email.as_deref(),
        req.role.as_deref(),
        req.current_password.as_deref(),
        req.new_password.as_deref(),
    )
    .await?;
    Ok(Json(result))
}

async fn revoke_user(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, AppError> {
    let result = user_service::revoke_user(&state.db, id, admin.id, &admin.username).await?;
    Ok(Json(result))
}
