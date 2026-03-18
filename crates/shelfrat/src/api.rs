mod admin;
mod auth;
mod books;
mod download;
mod health;
mod jobs;
mod oidc;
mod scan;
mod setup;
mod users;

use crate::state::AppState;
use axum::body::Body;
use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderValue, Method, Request};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::Router;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

pub fn router(state: AppState) -> Router {
    let web_dir = std::env::var("WEB_DIR").unwrap_or_else(|_| "web/build".into());
    let index = format!("{web_dir}/index.html");

    Router::new()
        .nest("/api/v1", api_routes())
        .fallback_service(ServeDir::new(&web_dir).fallback(ServeFile::new(&index)))
        .layer(middleware::from_fn(security_headers))
        .layer(TraceLayer::new_for_http())
        .layer(build_cors_layer())
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024)) // 10 MB
        .with_state(state)
}

/// Build a CORS layer that is restrictive by default.
///
/// - If `CORS_ORIGIN` env var is set, allows that specific origin (for dev with separate frontend).
/// - Otherwise, no cross-origin requests are allowed (the SPA is served from the same origin).
fn build_cors_layer() -> CorsLayer {
    let methods = AllowMethods::list([
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::DELETE,
        Method::OPTIONS,
    ]);
    let headers = AllowHeaders::list([header::CONTENT_TYPE, header::AUTHORIZATION]);

    match std::env::var("CORS_ORIGIN") {
        Ok(origin) if !origin.is_empty() => {
            let origin: HeaderValue = origin
                .parse()
                .expect("CORS_ORIGIN must be a valid header value");
            CorsLayer::new()
                .allow_origin(AllowOrigin::exact(origin))
                .allow_methods(methods)
                .allow_headers(headers)
                .max_age(std::time::Duration::from_secs(3600))
        }
        _ => CorsLayer::new()
            .allow_methods(methods)
            .allow_headers(headers),
    }
}

/// Middleware that sets security-related response headers on every response.
async fn security_headers(req: Request<Body>, next: Next) -> Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    // Modern best practice: disable legacy XSS auditor, rely on CSP
    headers.insert("x-xss-protection", HeaderValue::from_static("0"));
    headers.insert(
        "referrer-policy",
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    headers.insert(
        "content-security-policy",
        HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'"),
    );
    response
}

fn api_routes() -> Router<AppState> {
    Router::new()
        .merge(health::routes())
        .merge(setup::routes())
        .merge(auth::routes())
        .merge(books::routes())
        .merge(download::routes())
        .merge(users::routes())
        .merge(scan::routes())
        .merge(admin::routes())
        .merge(jobs::routes())
        .merge(oidc::routes())
}
