use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::auth::AuthUser;
use crate::email::{self, SmtpConfig};
use crate::error::AppError;
use crate::repositories::{audit_repo, book_repo, user_repo};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/books/{id}/download", get(download_book))
        .route("/books/{id}/cover", get(book_cover))
        .route("/books/{id}/send", post(send_to_kindle))
}

async fn download_book(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let book = book_repo::get_file_info(&state.db, id)
        .await?
        .ok_or(AppError::NotFound)?;

    let path = std::path::Path::new(&book.file_path);
    if !path.is_file() {
        return Err(AppError::NotFound);
    }

    let file = File::open(path)
        .await
        .map_err(|e| AppError::Internal(format!("cannot open file: {e}")))?;

    let metadata = file
        .metadata()
        .await
        .map_err(|e| AppError::Internal(format!("cannot read file metadata: {e}")))?;

    let content_type = mime_for_format(&book.file_format);
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("book");

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let safe_filename = sanitize_filename(filename);

    Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{safe_filename}\""),
        )
        .header(header::CONTENT_LENGTH, metadata.len())
        .body(body)
        .map_err(|e| AppError::Internal(format!("failed to build response: {e}")))
}

async fn book_cover(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let cover_path = book_repo::get_cover_path(&state.db, id)
        .await?
        .ok_or(AppError::NotFound)?;

    let path = std::path::Path::new(&cover_path);
    if !path.is_file() {
        return Err(AppError::NotFound);
    }

    let file = File::open(path)
        .await
        .map_err(|e| AppError::Internal(format!("cannot open cover: {e}")))?;

    let metadata = file
        .metadata()
        .await
        .map_err(|e| AppError::Internal(format!("cannot read cover metadata: {e}")))?;

    let content_type = path
        .extension()
        .and_then(|e| e.to_str())
        .map(mime_for_image_ext)
        .unwrap_or("application/octet-stream");

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, metadata.len())
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(body)
        .map_err(|e| AppError::Internal(format!("failed to build response: {e}")))
}

#[derive(Debug, Deserialize)]
struct SendRequest {
    email: Option<String>,
}

async fn send_to_kindle(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<SendRequest>,
) -> Result<Json<Value>, AppError> {
    let book = book_repo::get_file_info(&state.db, id)
        .await?
        .ok_or(AppError::NotFound)?;

    let path = std::path::Path::new(&book.file_path);
    if !path.is_file() {
        return Err(AppError::NotFound);
    }

    let to_email = if let Some(ref email) = req.email {
        email.clone()
    } else {
        let u = user_repo::find_by_id(&state.db, user.id)
            .await?
            .ok_or(AppError::NotFound)?;
        u.kindle_email
            .filter(|e| !e.is_empty())
            .ok_or_else(|| {
                AppError::BadRequest(
                    "no kindle email configured — set it in your profile or provide 'email' in request".into(),
                )
            })?
    };

    let smtp_config = SmtpConfig::from_db(&state.pool).await?;

    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("book");
    let content_type = mime_for_format(&book.file_format);

    email::send_book_email(&smtp_config, &to_email, filename, path, content_type).await?;

    audit_repo::log_action(
        &state.db,
        Some(user.id),
        "book_sent",
        Some(&format!("user {} sent book {} to {}", user.username, id, to_email)),
    )
    .await?;

    Ok(Json(json!({
        "message": "book sent",
        "to": to_email,
        "book_id": id,
    })))
}

/// Sanitize a filename for use in Content-Disposition headers.
/// Prevents header injection by stripping control chars and quotes.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_control())
        .map(|c| if matches!(c, '"' | '\\') { '_' } else { c })
        .collect()
}

fn mime_for_image_ext(ext: &str) -> &'static str {
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

fn mime_for_format(format: &str) -> &'static str {
    match format {
        "epub" => "application/epub+zip",
        "pdf" => "application/pdf",
        "mobi" => "application/x-mobipocket-ebook",
        "azw" | "azw3" => "application/vnd.amazon.ebook",
        "fb2" => "application/x-fictionbook+xml",
        "cbz" => "application/x-cbz",
        "cbr" => "application/x-cbr",
        "djvu" => "image/vnd.djvu",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
