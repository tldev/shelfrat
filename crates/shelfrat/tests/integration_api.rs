use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use std::str::FromStr;
use tower::ServiceExt; // for oneshot()

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

async fn setup_app() -> (axum::Router, sqlx::SqlitePool, sea_orm::DatabaseConnection) {
    let opts = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let db = sea_orm::SqlxSqliteConnector::from_sqlx_sqlite_pool(pool.clone());

    let state = shelfrat::state::AppState::new(db.clone(), pool.clone(), None, None, None);
    let app = shelfrat::api::router(state);
    (app, pool, db)
}

async fn create_admin_and_login(app: &axum::Router) -> String {
    // Run setup
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/setup")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "username": "admin",
                        "email": "admin@test.com",
                        "password": "testpassword123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Login
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "username": "admin",
                        "password": "testpassword123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["token"].as_str().unwrap().to_string()
}

/// Create a second (non-admin) user via the invite flow and return their JWT.
async fn create_member_and_login(app: &axum::Router, admin_token: &str) -> (String, i64) {
    // Admin creates invite
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/users/invite")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {admin_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let invite_json: Value = serde_json::from_slice(&body).unwrap();
    let invite_token = invite_json["invite_token"].as_str().unwrap();

    // Register with invite
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/users/register/{invite_token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "username": "member1",
                        "email": "member1@test.com",
                        "password": "memberpass123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Login as member
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "username": "member1",
                        "password": "memberpass123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let token = json["token"].as_str().unwrap().to_string();
    let user_id = json["user"]["id"].as_i64().unwrap();
    (token, user_id)
}

async fn body_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

// ===========================================================================
// Health
// ===========================================================================

#[tokio::test]
async fn health_returns_200_with_healthy_status() {
    let (app, _pool, _db) = setup_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["status"], "healthy");
    assert_eq!(json["database"], "connected");
}

// ===========================================================================
// Setup
// ===========================================================================

#[tokio::test]
async fn setup_status_initially_false() {
    let (app, _pool, _db) = setup_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/setup/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["setup_complete"], false);
}

#[tokio::test]
async fn setup_creates_admin_returns_200() {
    let (app, _pool, _db) = setup_app().await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/setup")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "username": "admin",
                        "email": "admin@test.com",
                        "password": "testpassword123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["username"], "admin");
}

#[tokio::test]
async fn setup_again_returns_409_conflict() {
    let (app, _pool, _db) = setup_app().await;

    // First setup
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/setup")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "username": "admin",
                        "email": "admin@test.com",
                        "password": "testpassword123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Second setup
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/setup")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "username": "admin2",
                        "email": "admin2@test.com",
                        "password": "testpassword456"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn setup_status_true_after_setup() {
    let (app, _pool, _db) = setup_app().await;

    // Do setup
    let _response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/setup")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "username": "admin",
                        "email": "admin@test.com",
                        "password": "testpassword123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Check status
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/setup/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["setup_complete"], true);
}

// ===========================================================================
// Auth
// ===========================================================================

#[tokio::test]
async fn auth_login_correct_credentials_returns_token_and_user() {
    let (app, _pool, _db) = setup_app().await;

    // Setup first
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/setup")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "username": "admin",
                        "email": "admin@test.com",
                        "password": "testpassword123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Login
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "username": "admin",
                        "password": "testpassword123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert!(json["token"].is_string());
    assert_eq!(json["user"]["username"], "admin");
    assert_eq!(json["user"]["email"], "admin@test.com");
    assert_eq!(json["user"]["role"], "admin");
}

#[tokio::test]
async fn auth_login_wrong_password_returns_401() {
    let (app, _pool, _db) = setup_app().await;

    // Setup
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/setup")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "username": "admin",
                        "email": "admin@test.com",
                        "password": "testpassword123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Login with wrong password
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "username": "admin",
                        "password": "wrongpassword"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_login_nonexistent_user_returns_401() {
    let (app, _pool, _db) = setup_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "username": "nobody",
                        "password": "whatever123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_me_with_valid_token_returns_user_info() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["username"], "admin");
    assert_eq!(json["email"], "admin@test.com");
    assert_eq!(json["role"], "admin");
}

#[tokio::test]
async fn auth_me_without_token_returns_401() {
    let (app, _pool, _db) = setup_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_me_with_invalid_token_returns_401() {
    let (app, _pool, _db) = setup_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .header("Authorization", "Bearer not.a.valid.token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ===========================================================================
// Books
// ===========================================================================

#[tokio::test]
async fn books_list_without_auth_returns_401() {
    let (app, _pool, _db) = setup_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn books_list_with_auth_returns_empty_list() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    // The list should be present (empty array or object with books array)
    assert!(json["books"].is_array() || json["total"].as_i64() == Some(0));
}

#[tokio::test]
async fn books_get_nonexistent_returns_404() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/99999")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn books_search_returns_results() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/search?q=test")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert!(json["books"].is_array());
}

#[tokio::test]
async fn authors_returns_empty_list() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/authors")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert!(json["authors"].is_array());
}

#[tokio::test]
async fn tags_returns_empty_list() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/tags")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert!(json["tags"].is_array());
}

#[tokio::test]
async fn formats_returns_empty_list() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/formats")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert!(json["formats"].is_array());
}

// ===========================================================================
// Users
// ===========================================================================

#[tokio::test]
async fn users_list_as_admin_returns_users() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/users")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let users = json["users"].as_array().unwrap();
    assert!(!users.is_empty());
    assert_eq!(users[0]["username"], "admin");
}

#[tokio::test]
async fn users_list_as_non_admin_returns_403() {
    let (app, _pool, _db) = setup_app().await;
    let admin_token = create_admin_and_login(&app).await;
    let (member_token, _member_id) = create_member_and_login(&app, &admin_token).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/users")
                .header("Authorization", format!("Bearer {member_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn users_invite_as_admin_creates_token() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/users/invite")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert!(json["invite_token"].is_string());
    assert!(json["invite_url"].is_string());
}

#[tokio::test]
async fn users_register_with_valid_invite() {
    let (app, _pool, _db) = setup_app().await;
    let admin_token = create_admin_and_login(&app).await;

    // Create invite
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/users/invite")
                .header("Authorization", format!("Bearer {admin_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json = body_json(response).await;
    let invite_token = json["invite_token"].as_str().unwrap();

    // Register
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/users/register/{invite_token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "username": "newuser",
                        "email": "newuser@test.com",
                        "password": "newpassword123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["username"], "newuser");
}

#[tokio::test]
async fn users_register_with_bad_token_returns_404() {
    let (app, _pool, _db) = setup_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/users/register/nonexistent-token-123")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "username": "newuser",
                        "email": "newuser@test.com",
                        "password": "newpassword123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn users_get_as_admin_returns_user() {
    let (app, _pool, _db) = setup_app().await;
    let admin_token = create_admin_and_login(&app).await;
    let (_, member_id) = create_member_and_login(&app, &admin_token).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/users/{member_id}"))
                .header("Authorization", format!("Bearer {admin_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["username"], "member1");
}

#[tokio::test]
async fn users_get_self_returns_own_info() {
    let (app, _pool, _db) = setup_app().await;
    let admin_token = create_admin_and_login(&app).await;
    let (member_token, member_id) = create_member_and_login(&app, &admin_token).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/users/{member_id}"))
                .header("Authorization", format!("Bearer {member_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["username"], "member1");
}

#[tokio::test]
async fn users_get_other_as_non_admin_returns_403() {
    let (app, _pool, _db) = setup_app().await;
    let admin_token = create_admin_and_login(&app).await;
    let (member_token, _member_id) = create_member_and_login(&app, &admin_token).await;

    // Get the admin user's info using the admin token to find the admin id
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .header("Authorization", format!("Bearer {admin_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json = body_json(response).await;
    let admin_id = json["id"].as_i64().unwrap();

    // Member tries to view admin's profile
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/users/{admin_id}"))
                .header("Authorization", format!("Bearer {member_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn users_update_own_fields() {
    let (app, _pool, _db) = setup_app().await;
    let admin_token = create_admin_and_login(&app).await;
    let (member_token, member_id) = create_member_and_login(&app, &admin_token).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/users/{member_id}"))
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {member_token}"))
                .body(Body::from(
                    json!({
                        "display_name": "Test Member",
                        "kindle_email": "member@kindle.com"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["display_name"], "Test Member");
    assert_eq!(json["kindle_email"], "member@kindle.com");
}

#[tokio::test]
async fn users_role_change_requires_admin() {
    let (app, _pool, _db) = setup_app().await;
    let admin_token = create_admin_and_login(&app).await;
    let (member_token, member_id) = create_member_and_login(&app, &admin_token).await;

    // Member tries to change own role -> should be forbidden
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/users/{member_id}"))
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {member_token}"))
                .body(Body::from(
                    json!({
                        "role": "admin"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn users_delete_as_admin_revokes_user() {
    let (app, _pool, _db) = setup_app().await;
    let admin_token = create_admin_and_login(&app).await;
    let (_, member_id) = create_member_and_login(&app, &admin_token).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/users/{member_id}"))
                .header("Authorization", format!("Bearer {admin_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["message"], "user revoked");
}

#[tokio::test]
async fn users_delete_self_returns_400() {
    let (app, _pool, _db) = setup_app().await;
    let admin_token = create_admin_and_login(&app).await;

    // Get admin's own ID
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .header("Authorization", format!("Bearer {admin_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json = body_json(response).await;
    let admin_id = json["id"].as_i64().unwrap();

    // Try to delete self
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/users/{admin_id}"))
                .header("Authorization", format!("Bearer {admin_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ===========================================================================
// Admin - Settings
// ===========================================================================

#[tokio::test]
async fn admin_settings_get_as_admin_returns_settings() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/settings")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert!(json["settings"].is_object());
}

#[tokio::test]
async fn admin_settings_without_auth_returns_401() {
    let (app, _pool, _db) = setup_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_settings_as_non_admin_returns_403() {
    let (app, _pool, _db) = setup_app().await;
    let admin_token = create_admin_and_login(&app).await;
    let (member_token, _) = create_member_and_login(&app, &admin_token).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/settings")
                .header("Authorization", format!("Bearer {member_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_settings_put_updates_settings() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/admin/settings")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::from(
                    json!({
                        "library_path": "/tmp/books"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["message"], "settings updated");
    let updated = json["updated"].as_array().unwrap();
    assert!(updated.iter().any(|v| v == "library_path"));
}

#[tokio::test]
async fn admin_settings_put_unknown_key_returns_400() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/admin/settings")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::from(
                    json!({
                        "not_a_real_key": "value"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ===========================================================================
// Admin - Audit Log
// ===========================================================================

#[tokio::test]
async fn admin_audit_log_returns_entries() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    // The login itself creates an audit entry
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/audit-log")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert!(json["entries"].is_array());
    // Should have at least a login entry
    let entries = json["entries"].as_array().unwrap();
    assert!(!entries.is_empty());
}

#[tokio::test]
async fn admin_audit_log_without_auth_returns_401() {
    let (app, _pool, _db) = setup_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/audit-log")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ===========================================================================
// Admin - Library Info
// ===========================================================================

#[tokio::test]
async fn admin_library_info_returns_stats() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/library-info")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["total_books"], 0);
}

// ===========================================================================
// Jobs
// ===========================================================================

#[tokio::test]
async fn admin_jobs_list_returns_known_jobs() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/jobs")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let jobs = json["jobs"].as_array().unwrap();
    assert!(!jobs.is_empty());
    assert!(jobs.iter().any(|j| j["name"] == "library_scan"));
}

#[tokio::test]
async fn admin_jobs_trigger_without_scheduler_returns_500() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    // job_handle is None in test setup, so triggering should return 500
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/jobs/library_scan/run")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn admin_jobs_trigger_unknown_returns_400() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/jobs/nonexistent_job/run")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn admin_jobs_runs_returns_history() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/jobs/library_scan/runs")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert!(json["runs"].is_array());
    assert_eq!(json["total"], 0);
}

#[tokio::test]
async fn admin_jobs_runs_unknown_returns_400() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/jobs/nonexistent_job/runs")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn admin_jobs_update_cadence() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/admin/jobs/library_scan/cadence")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::from(json!({"seconds": 600}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["cadence_seconds"], 600);
    assert_eq!(json["job"], "library_scan");
    assert_eq!(json["enabled"], true);
}

#[tokio::test]
async fn admin_jobs_update_cadence_disable() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/admin/jobs/library_scan/cadence")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::from(json!({"seconds": 0}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["cadence_seconds"], 0);
    assert_eq!(json["enabled"], false);
}

#[tokio::test]
async fn admin_jobs_without_auth_returns_401() {
    let (app, _pool, _db) = setup_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/jobs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_jobs_as_non_admin_returns_403() {
    let (app, _pool, _db) = setup_app().await;
    let admin_token = create_admin_and_login(&app).await;
    let (member_token, _) = create_member_and_login(&app, &admin_token).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/jobs")
                .header("Authorization", format!("Bearer {member_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// ===========================================================================
// Download endpoints
// ===========================================================================

#[tokio::test]
async fn download_nonexistent_book_returns_404() {
    let (app, _pool, _db) = setup_app().await;
    let token = create_admin_and_login(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/99999/download")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn download_without_auth_returns_401() {
    let (app, _pool, _db) = setup_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/1/download")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn cover_nonexistent_book_returns_404() {
    let (app, _pool, _db) = setup_app().await;

    // Covers don't require auth (checked from source: no AuthUser extractor)
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/99999/cover")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ===========================================================================
// OIDC
// ===========================================================================

#[tokio::test]
async fn oidc_status_returns_disabled_when_not_configured() {
    let (app, _pool, _db) = setup_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/oidc/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["enabled"], false);
}

// ===========================================================================
// Cross-cutting: security headers
// ===========================================================================

#[tokio::test]
async fn responses_include_security_headers() {
    let (app, _pool, _db) = setup_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    assert_eq!(response.headers().get("x-frame-options").unwrap(), "DENY");
    assert_eq!(response.headers().get("x-xss-protection").unwrap(), "0");
    assert!(response.headers().get("referrer-policy").is_some());
    assert!(response.headers().get("permissions-policy").is_some());
    assert!(response.headers().get("content-security-policy").is_some());
}

// ===========================================================================
// Users - Admin role change on another user
// ===========================================================================

#[tokio::test]
async fn admin_can_change_member_role() {
    let (app, _pool, _db) = setup_app().await;
    let admin_token = create_admin_and_login(&app).await;
    let (_, member_id) = create_member_and_login(&app, &admin_token).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/users/{member_id}"))
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {admin_token}"))
                .body(Body::from(
                    json!({
                        "role": "admin"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["role"], "admin");
}

#[tokio::test]
async fn admin_cannot_change_own_role() {
    let (app, _pool, _db) = setup_app().await;
    let admin_token = create_admin_and_login(&app).await;

    // Get admin's own ID
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .header("Authorization", format!("Bearer {admin_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json = body_json(response).await;
    let admin_id = json["id"].as_i64().unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/users/{admin_id}"))
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {admin_token}"))
                .body(Body::from(
                    json!({
                        "role": "member"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ===========================================================================
// Users - delete as non-admin returns 403
// ===========================================================================

#[tokio::test]
async fn users_delete_as_non_admin_returns_403() {
    let (app, _pool, _db) = setup_app().await;
    let admin_token = create_admin_and_login(&app).await;
    let (member_token, _) = create_member_and_login(&app, &admin_token).await;

    // Get admin ID
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .header("Authorization", format!("Bearer {admin_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json = body_json(response).await;
    let admin_id = json["id"].as_i64().unwrap();

    // Member tries to delete admin
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/users/{admin_id}"))
                .header("Authorization", format!("Bearer {member_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// ===========================================================================
// Admin - audit log as non-admin returns 403
// ===========================================================================

#[tokio::test]
async fn admin_audit_log_as_non_admin_returns_403() {
    let (app, _pool, _db) = setup_app().await;
    let admin_token = create_admin_and_login(&app).await;
    let (member_token, _) = create_member_and_login(&app, &admin_token).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/audit-log")
                .header("Authorization", format!("Bearer {member_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// ===========================================================================
// Admin - library info as non-admin returns 403
// ===========================================================================

#[tokio::test]
async fn admin_library_info_as_non_admin_returns_403() {
    let (app, _pool, _db) = setup_app().await;
    let admin_token = create_admin_and_login(&app).await;
    let (member_token, _) = create_member_and_login(&app, &admin_token).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/library-info")
                .header("Authorization", format!("Bearer {member_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// ===========================================================================
// Books - auth required on all book endpoints
// ===========================================================================

#[tokio::test]
async fn books_search_without_auth_returns_401() {
    let (app, _pool, _db) = setup_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/search?q=test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn authors_without_auth_returns_401() {
    let (app, _pool, _db) = setup_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/authors")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tags_without_auth_returns_401() {
    let (app, _pool, _db) = setup_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/tags")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn formats_without_auth_returns_401() {
    let (app, _pool, _db) = setup_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/formats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ===========================================================================
// Users - invite as non-admin returns 403
// ===========================================================================

#[tokio::test]
async fn users_invite_as_non_admin_returns_403() {
    let (app, _pool, _db) = setup_app().await;
    let admin_token = create_admin_and_login(&app).await;
    let (member_token, _) = create_member_and_login(&app, &admin_token).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/users/invite")
                .header("Authorization", format!("Bearer {member_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
