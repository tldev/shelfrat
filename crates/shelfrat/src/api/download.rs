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
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("book");

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
        u.kindle_email.filter(|e| !e.is_empty()).ok_or_else(|| {
            AppError::BadRequest(
                "no kindle email configured — set it in your profile or provide 'email' in request"
                    .into(),
            )
        })?
    };

    let smtp_config = SmtpConfig::from_db(&state.pool).await?;

    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("book");
    let content_type = mime_for_format(&book.file_format);

    email::send_book_email(&smtp_config, &to_email, filename, path, content_type).await?;

    audit_repo::log_action(
        &state.db,
        Some(user.id),
        "book_sent",
        Some(&format!(
            "user {} sent book {} to {}",
            user.username, id, to_email
        )),
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── sanitize_filename ──────────────────────────────────────────

    #[test]
    fn sanitize_filename_normal() {
        assert_eq!(sanitize_filename("book.epub"), "book.epub");
    }

    #[test]
    fn sanitize_filename_strips_control_chars() {
        assert_eq!(sanitize_filename("bo\x00ok\x1F.epub"), "book.epub");
    }

    #[test]
    fn sanitize_filename_replaces_quotes_and_backslash() {
        assert_eq!(sanitize_filename("my\"book\\.epub"), "my_book_.epub");
    }

    #[test]
    fn sanitize_filename_mixed() {
        assert_eq!(
            sanitize_filename("a\x01\"b\\c\x7f"),
            "a_b_c" // \x7f is a control char (DEL)
        );
    }

    #[test]
    fn sanitize_filename_empty() {
        assert_eq!(sanitize_filename(""), "");
    }

    #[test]
    fn sanitize_filename_all_normal() {
        let name = "My Book (2024) - Author.epub";
        assert_eq!(sanitize_filename(name), name);
    }

    // ── mime_for_format ────────────────────────────────────────────

    #[test]
    fn mime_epub() {
        assert_eq!(mime_for_format("epub"), "application/epub+zip");
    }

    #[test]
    fn mime_pdf() {
        assert_eq!(mime_for_format("pdf"), "application/pdf");
    }

    #[test]
    fn mime_mobi() {
        assert_eq!(mime_for_format("mobi"), "application/x-mobipocket-ebook");
    }

    #[test]
    fn mime_azw() {
        assert_eq!(mime_for_format("azw"), "application/vnd.amazon.ebook");
    }

    #[test]
    fn mime_azw3() {
        assert_eq!(mime_for_format("azw3"), "application/vnd.amazon.ebook");
    }

    #[test]
    fn mime_fb2() {
        assert_eq!(mime_for_format("fb2"), "application/x-fictionbook+xml");
    }

    #[test]
    fn mime_cbz() {
        assert_eq!(mime_for_format("cbz"), "application/x-cbz");
    }

    #[test]
    fn mime_cbr() {
        assert_eq!(mime_for_format("cbr"), "application/x-cbr");
    }

    #[test]
    fn mime_djvu() {
        assert_eq!(mime_for_format("djvu"), "image/vnd.djvu");
    }

    #[test]
    fn mime_txt() {
        assert_eq!(mime_for_format("txt"), "text/plain; charset=utf-8");
    }

    #[test]
    fn mime_unknown() {
        assert_eq!(mime_for_format("xyz"), "application/octet-stream");
        assert_eq!(mime_for_format(""), "application/octet-stream");
    }

    // ── mime_for_image_ext ─────────────────────────────────────────

    #[test]
    fn image_jpg() {
        assert_eq!(mime_for_image_ext("jpg"), "image/jpeg");
    }

    #[test]
    fn image_jpeg() {
        assert_eq!(mime_for_image_ext("jpeg"), "image/jpeg");
    }

    #[test]
    fn image_png() {
        assert_eq!(mime_for_image_ext("png"), "image/png");
    }

    #[test]
    fn image_gif() {
        assert_eq!(mime_for_image_ext("gif"), "image/gif");
    }

    #[test]
    fn image_webp() {
        assert_eq!(mime_for_image_ext("webp"), "image/webp");
    }

    #[test]
    fn image_unknown() {
        assert_eq!(mime_for_image_ext("bmp"), "application/octet-stream");
        assert_eq!(mime_for_image_ext(""), "application/octet-stream");
    }
}
