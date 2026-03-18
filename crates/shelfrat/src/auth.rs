use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use jsonwebtoken::{decode, DecodingKey, Validation};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::repositories::config_repo;
use crate::state::AppState;

/// JWT claims embedded in every token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: i64,
    pub username: String,
    pub role: String,
    pub exp: i64,
    pub iat: i64,
}

/// Authenticated user extracted from a valid JWT in the Authorization header.
/// Use as an Axum extractor on any handler that requires login.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: i64,
    pub username: String,
    pub role: String,
}

/// Like `AuthUser`, but rejects non-admin users with 403.
#[derive(Debug, Clone)]
pub struct AdminUser(pub AuthUser);

impl std::ops::Deref for AdminUser {
    type Target = AuthUser;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Error returned when auth extraction fails.
#[derive(Debug)]
pub enum AuthError {
    MissingToken,
    InvalidToken(String),
    Forbidden,
    Internal(String),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            AuthError::MissingToken => (StatusCode::UNAUTHORIZED, "missing authorization token"),
            AuthError::InvalidToken(e) => {
                tracing::debug!("auth: invalid token: {e}");
                (StatusCode::UNAUTHORIZED, "invalid or expired token")
            }
            AuthError::Forbidden => (StatusCode::FORBIDDEN, "admin access required"),
            AuthError::Internal(e) => {
                tracing::warn!("auth: internal error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
            }
        };
        let body = serde_json::json!({ "error": msg });
        (status, axum::Json(body)).into_response()
    }
}

/// Retrieve (or create) the JWT secret stored in app_config.
pub async fn get_or_create_jwt_secret(db: &DatabaseConnection) -> Result<String, AuthError> {
    config_repo::get_or_create_jwt_secret(db)
        .await
        .map_err(|e| AuthError::Internal(e.to_string()))
}

/// Extract the Bearer token from the Authorization header.
fn extract_bearer_token(parts: &Parts) -> Result<&str, AuthError> {
    let header = parts
        .headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(AuthError::MissingToken)?;

    header
        .strip_prefix("Bearer ")
        .ok_or(AuthError::MissingToken)
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AuthError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let db = state.db.clone();
        let token = extract_bearer_token(parts).map(|s| s.to_owned());

        async move {
            let token = token?;
            let secret = get_or_create_jwt_secret(&db).await?;

            let token_data = decode::<Claims>(
                &token,
                &DecodingKey::from_secret(secret.as_bytes()),
                &Validation::default(),
            )
            .map_err(|e| AuthError::InvalidToken(e.to_string()))?;

            Ok(AuthUser {
                id: token_data.claims.sub,
                username: token_data.claims.username,
                role: token_data.claims.role,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Method;
    use axum::response::IntoResponse;

    // ── AuthError::into_response ───────────────────────────────────

    #[test]
    fn auth_error_missing_token_returns_401() {
        let resp = AuthError::MissingToken.into_response();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn auth_error_invalid_token_returns_401() {
        let resp = AuthError::InvalidToken("expired".to_string()).into_response();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn auth_error_forbidden_returns_403() {
        let resp = AuthError::Forbidden.into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn auth_error_internal_returns_500() {
        let resp = AuthError::Internal("db down".to_string()).into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ── extract_bearer_token ───────────────────────────────────────

    fn make_parts(auth_header: Option<&str>) -> Parts {
        let mut builder = axum::http::Request::builder()
            .method(Method::GET)
            .uri("/test");
        if let Some(val) = auth_header {
            builder = builder.header("authorization", val);
        }
        let req = builder.body(()).unwrap();
        req.into_parts().0
    }

    #[test]
    fn extract_bearer_token_valid() {
        let parts = make_parts(Some("Bearer my_token_123"));
        let token = extract_bearer_token(&parts).unwrap();
        assert_eq!(token, "my_token_123");
    }

    #[test]
    fn extract_bearer_token_missing_header() {
        let parts = make_parts(None);
        let result = extract_bearer_token(&parts);
        assert!(matches!(result, Err(AuthError::MissingToken)));
    }

    #[test]
    fn extract_bearer_token_wrong_scheme() {
        let parts = make_parts(Some("Basic abc123"));
        let result = extract_bearer_token(&parts);
        assert!(matches!(result, Err(AuthError::MissingToken)));
    }

    #[test]
    fn extract_bearer_token_empty_bearer() {
        let parts = make_parts(Some("Bearer "));
        let token = extract_bearer_token(&parts).unwrap();
        assert_eq!(token, "");
    }

    #[test]
    fn extract_bearer_token_no_space_after_bearer() {
        let parts = make_parts(Some("Bearertoken"));
        let result = extract_bearer_token(&parts);
        assert!(matches!(result, Err(AuthError::MissingToken)));
    }
}

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AuthError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let db = state.db.clone();
        let token = extract_bearer_token(parts).map(|s| s.to_owned());

        async move {
            let token = token?;
            let secret = get_or_create_jwt_secret(&db).await?;

            let token_data = decode::<Claims>(
                &token,
                &DecodingKey::from_secret(secret.as_bytes()),
                &Validation::default(),
            )
            .map_err(|e| AuthError::InvalidToken(e.to_string()))?;

            let user = AuthUser {
                id: token_data.claims.sub,
                username: token_data.claims.username,
                role: token_data.claims.role,
            };

            if user.role != "admin" {
                return Err(AuthError::Forbidden);
            }

            Ok(AdminUser(user))
        }
    }
}
