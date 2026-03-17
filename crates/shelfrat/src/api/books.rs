use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::services::book_service;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/books", get(list_books))
        .route("/books/{id}", get(get_book))
        .route("/books/search", get(search_books))
        .route("/authors", get(list_authors))
        .route("/tags", get(list_tags))
        .route("/formats", get(list_formats))
}

#[derive(Debug, Deserialize)]
struct ListParams {
    sort: Option<String>,
    limit: Option<u64>,
    offset: Option<u64>,
    author: Option<String>,
    tag: Option<String>,
    format: Option<String>,
}

async fn list_books(
    _user: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Value>, AppError> {
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);
    let result = book_service::list_books(
        &state.db,
        params.sort.as_deref(),
        params.author.as_deref(),
        params.tag.as_deref(),
        params.format.as_deref(),
        limit,
        offset,
    )
    .await?;
    Ok(Json(result))
}

async fn get_book(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, AppError> {
    let result = book_service::get_book(&state.db, id).await?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    q: String,
    limit: Option<u64>,
}

async fn search_books(
    _user: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Value>, AppError> {
    let limit = params.limit.unwrap_or(20).min(100);
    let result = book_service::search_books(&state.db, &params.q, limit).await?;
    Ok(Json(result))
}

async fn list_authors(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let result = book_service::list_authors(&state.db).await?;
    Ok(Json(result))
}

async fn list_tags(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let result = book_service::list_tags(&state.db).await?;
    Ok(Json(result))
}

async fn list_formats(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let result = book_service::list_formats(&state.db).await?;
    Ok(Json(result))
}
