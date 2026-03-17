use axum::extract::{Query, State};
use axum::response::Redirect;
use axum::routing::get;
use axum::{Json, Router};
use jsonwebtoken::{decode, decode_header, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth::Claims;
use crate::error::AppError;
use crate::repositories::{audit_repo, config_repo, user_repo};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/oidc/status", get(oidc_status))
        .route("/auth/oidc/authorize", get(oidc_authorize))
        .route("/auth/oidc/callback", get(oidc_callback))
}

// --- Types ---

#[derive(Debug, Deserialize)]
struct OidcDiscovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
}

#[derive(Debug, Deserialize)]
struct JwkSet {
    keys: Vec<JwkKey>,
}

#[derive(Debug, Deserialize)]
struct JwkKey {
    #[allow(dead_code)]
    kty: String,
    kid: Option<String>,
    n: Option<String>,
    e: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    id_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IdTokenClaims {
    #[allow(dead_code)]
    iss: String,
    sub: String,
    nonce: Option<String>,
    email: Option<String>,
    name: Option<String>,
    preferred_username: Option<String>,
    /// Catch-all for extra claims (groups, roles, etc.)
    #[serde(flatten)]
    extra: std::collections::HashMap<String, serde_json::Value>,
}

/// Signed state JWT — carries nonce for CSRF protection, stateless.
#[derive(Debug, Serialize, Deserialize)]
struct OidcState {
    nonce: String,
    exp: i64,
}

#[derive(Debug, Default)]
struct OidcConfig {
    issuer_url: String,
    client_id: String,
    client_secret: String,
    auto_register: bool,
    admin_claim: String,
    admin_value: String,
}

#[derive(Debug, Deserialize)]
struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

// --- Helpers ---

async fn get_oidc_config(db: &sea_orm::DatabaseConnection) -> Result<Option<OidcConfig>, AppError> {
    let rows = config_repo::get_by_prefix(db, "oidc_").await?;

    let mut config = OidcConfig {
        auto_register: true, // default
        ..Default::default()
    };

    for row in &rows {
        match row.key.as_str() {
            "oidc_issuer_url" => config.issuer_url = row.value.clone(),
            "oidc_client_id" => config.client_id = row.value.clone(),
            "oidc_client_secret" => config.client_secret = row.value.clone(),
            "oidc_auto_register" => config.auto_register = row.value != "false",
            "oidc_admin_claim" => config.admin_claim = row.value.clone(),
            "oidc_admin_value" => config.admin_value = row.value.clone(),
            _ => {}
        }
    }

    // Default claim name
    if config.admin_claim.is_empty() {
        config.admin_claim = "groups".to_string();
    }

    if config.issuer_url.is_empty() || config.client_id.is_empty() {
        return Ok(None);
    }

    Ok(Some(config))
}

async fn get_app_url(db: &sea_orm::DatabaseConnection) -> Result<String, AppError> {
    config_repo::get(db, "app_url")
        .await?
        .ok_or(AppError::BadRequest("app_url not configured".into()))
}

/// Build a reqwest client with a 10-second timeout for OIDC external requests.
fn oidc_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("failed to build HTTP client")
}

async fn discover_oidc(issuer_url: &str) -> Result<OidcDiscovery, AppError> {
    let url = format!(
        "{}/.well-known/openid-configuration",
        issuer_url.trim_end_matches('/')
    );
    oidc_http_client()
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("OIDC discovery failed: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("OIDC discovery parse failed: {e}")))
}

/// Resolve the user's role from OIDC claims.
/// Returns "admin" if the configured claim contains the configured value, otherwise "member".
/// Returns None if role mapping is not configured (admin_value is empty).
fn resolve_oidc_role(config: &OidcConfig, claims: &IdTokenClaims) -> Option<String> {
    if config.admin_value.is_empty() {
        return None;
    }

    let claim_value = claims.extra.get(&config.admin_claim)?;

    let is_admin = match claim_value {
        // Claim is an array (e.g. groups: ["users", "shelfrat-admin"])
        serde_json::Value::Array(arr) => arr.iter().any(|v| {
            v.as_str()
                .is_some_and(|s| s == config.admin_value)
        }),
        // Claim is a plain string
        serde_json::Value::String(s) => s == &config.admin_value,
        _ => false,
    };

    Some(if is_admin { "admin" } else { "member" }.to_string())
}

// --- Endpoints ---

async fn oidc_status(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let config = get_oidc_config(&state.db).await?;
    Ok(Json(json!({ "enabled": config.is_some() })))
}

async fn oidc_authorize(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let config = get_oidc_config(&state.db)
        .await?
        .ok_or(AppError::BadRequest("OIDC not configured".into()))?;
    let app_url = get_app_url(&state.db).await?;
    let discovery = discover_oidc(&config.issuer_url).await?;

    let jwt_secret = config_repo::get_or_create_jwt_secret(&state.db)
        .await
        .map_err(|_| AppError::Internal("failed to get jwt secret".into()))?;

    let nonce = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();

    let oidc_state = OidcState {
        nonce: nonce.clone(),
        exp: (now + chrono::Duration::minutes(10)).timestamp(),
    };

    let state_jwt = encode(
        &Header::default(),
        &oidc_state,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("state JWT creation failed: {e}")))?;

    let redirect_uri = format!(
        "{}/api/v1/auth/oidc/callback",
        app_url.trim_end_matches('/')
    );

    let mut auth_url = reqwest::Url::parse(&discovery.authorization_endpoint)
        .map_err(|e| AppError::Internal(format!("invalid authorization endpoint: {e}")))?;

    auth_url
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &config.client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("scope", if config.admin_value.is_empty() {
            "openid email profile"
        } else {
            "openid email profile groups"
        })
        .append_pair("state", &state_jwt)
        .append_pair("nonce", &nonce);

    Ok(Json(json!({ "url": auth_url.to_string() })))
}

async fn oidc_callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> Redirect {
    match handle_oidc_callback(&state, params).await {
        Ok(redirect) => redirect,
        Err(e) => {
            tracing::error!("OIDC callback error: {e}");
            Redirect::to("/login?error=oidc_failed")
        }
    }
}

async fn handle_oidc_callback(
    state: &AppState,
    params: CallbackParams,
) -> Result<Redirect, AppError> {
    if let Some(error) = params.error {
        let desc = params.error_description.unwrap_or_default();
        tracing::warn!("OIDC provider error: {error}: {desc}");
        return Ok(Redirect::to("/login?error=oidc_failed"));
    }

    let code = params
        .code
        .ok_or(AppError::BadRequest("missing code".into()))?;
    let state_jwt = params
        .state
        .ok_or(AppError::BadRequest("missing state".into()))?;

    let config = get_oidc_config(&state.db)
        .await?
        .ok_or(AppError::BadRequest("OIDC not configured".into()))?;
    let app_url = get_app_url(&state.db).await?;
    let discovery = discover_oidc(&config.issuer_url).await?;

    let jwt_secret = config_repo::get_or_create_jwt_secret(&state.db)
        .await
        .map_err(|_| AppError::Internal("failed to get jwt secret".into()))?;

    // Verify state JWT
    let oidc_state = decode::<OidcState>(
        &state_jwt,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| {
        tracing::warn!("OIDC state validation failed: {e}");
        AppError::BadRequest("invalid or expired OIDC state".into())
    })?
    .claims;

    // Exchange code for tokens
    let redirect_uri = format!(
        "{}/api/v1/auth/oidc/callback",
        app_url.trim_end_matches('/')
    );

    let client = oidc_http_client();
    let token_res: TokenResponse = client
        .post(&discovery.token_endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
        ])
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("token exchange failed: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("token response parse failed: {e}")))?;

    let id_token_str = token_res
        .id_token
        .ok_or(AppError::Internal("no id_token in response".into()))?;

    // Validate ID token
    let header = decode_header(&id_token_str)
        .map_err(|e| AppError::Internal(format!("ID token header decode failed: {e}")))?;

    let jwks: JwkSet = client
        .get(&discovery.jwks_uri)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("JWKS fetch failed: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("JWKS parse failed: {e}")))?;

    let key = if let Some(ref kid) = header.kid {
        jwks.keys.iter().find(|k| k.kid.as_deref() == Some(kid))
    } else {
        jwks.keys.first()
    }
    .ok_or(AppError::Internal("no matching key in JWKS".into()))?;

    let (n, e) = match (key.n.as_ref(), key.e.as_ref()) {
        (Some(n), Some(e)) => (n, e),
        _ => {
            return Err(AppError::Internal(
                "JWKS key missing RSA components".into(),
            ))
        }
    };

    let decoding_key = DecodingKey::from_rsa_components(n, e)
        .map_err(|e| AppError::Internal(format!("RSA key construction failed: {e}")))?;

    let mut validation = Validation::new(header.alg);
    validation.set_issuer(&[&discovery.issuer]);
    validation.set_audience(&[&config.client_id]);

    let id_token = decode::<IdTokenClaims>(&id_token_str, &decoding_key, &validation)
        .map_err(|e| {
            tracing::warn!("ID token validation failed: {e}");
            AppError::BadRequest("ID token validation failed".into())
        })?
        .claims;

    tracing::debug!(
        "OIDC claims: sub={}, email={:?}, name={:?}, preferred_username={:?}",
        id_token.sub, id_token.email, id_token.name, id_token.preferred_username
    );
    tracing::debug!(
        "OIDC extra claims: {:?}",
        id_token.extra.keys().collect::<Vec<_>>()
    );
    if !config.admin_value.is_empty() {
        tracing::debug!(
            "OIDC role mapping: claim={}, admin_value={}, claim_data={:?}",
            config.admin_claim, config.admin_value,
            id_token.extra.get(&config.admin_claim)
        );
    }

    // Verify nonce
    if id_token.nonce.as_deref() != Some(&oidc_state.nonce) {
        return Err(AppError::BadRequest("nonce mismatch".into()));
    }

    // Resolve role from OIDC claims (if role mapping is configured)
    let oidc_role = resolve_oidc_role(&config, &id_token);
    tracing::debug!("OIDC resolved role: {:?}", oidc_role);

    // Find or create user
    let user = user_repo::find_by_oidc(&state.db, &id_token.sub, &discovery.issuer).await?;

    let user = match user {
        Some(u) => {
            // Sync role on every login if role mapping is configured
            if let Some(ref role) = oidc_role {
                if *role != u.role {
                    user_repo::update_role(&state.db, u.id, role).await?;
                    tracing::info!(
                        "OIDC role sync: {} changed from {} to {}",
                        u.username, u.role, role
                    );
                }
            }
            // Re-fetch to get updated role
            user_repo::find_by_id(&state.db, u.id)
                .await?
                .ok_or(AppError::Internal("user not found after update".into()))?
        }
        None => {
            if !config.auto_register {
                return Ok(Redirect::to("/login?error=oidc_no_account"));
            }

            let username = id_token
                .preferred_username
                .or_else(|| {
                    id_token
                        .email
                        .as_ref()
                        .map(|e| e.split('@').next().unwrap_or("user").to_string())
                })
                .unwrap_or_else(|| {
                    format!("oidc_{}", &id_token.sub[..8.min(id_token.sub.len())])
                });

            let email = id_token.email.unwrap_or_default();
            let display_name = id_token.name;
            let role = oidc_role.as_deref().unwrap_or("member");

            let final_username = user_repo::ensure_unique_username(&state.db, &username).await?;

            user_repo::create_oidc_user(
                &state.db,
                &final_username,
                display_name.as_deref(),
                &email,
                role,
                &id_token.sub,
                &discovery.issuer,
            )
            .await?;

            audit_repo::log_action(
                &state.db,
                None,
                "oidc_register",
                Some(&format!("user {final_username} registered via OIDC as {role}")),
            )
            .await?;

            user_repo::find_by_oidc(&state.db, &id_token.sub, &discovery.issuer)
                .await?
                .ok_or(AppError::Internal("user not found after OIDC registration".into()))?
        }
    };

    // Issue app JWT
    let now = chrono::Utc::now();
    let claims = Claims {
        sub: user.id,
        username: user.username.clone(),
        role: user.role.clone(),
        iat: now.timestamp(),
        exp: (now + chrono::Duration::hours(24)).timestamp(),
    };

    let app_token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("JWT creation failed: {e}")))?;

    // Audit log
    audit_repo::log_action(
        &state.db,
        Some(user.id),
        "login",
        Some(&format!("user {} logged in via OIDC", user.username)),
    )
    .await?;

    // Redirect to frontend with token in URL fragment (not sent in referer/logs)
    let redirect_url = format!("/login#oidc_token={app_token}");
    Ok(Redirect::to(&redirect_url))
}
