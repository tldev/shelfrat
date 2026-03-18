use shelfrat::repositories::{
    audit_repo, book_repo, config_repo, job_repo, metadata_repo, user_repo,
};
use shelfrat::scanner::ScannedFile;
use std::path::PathBuf;
use std::str::FromStr;

async fn setup_test_db() -> (sqlx::SqlitePool, sea_orm::DatabaseConnection) {
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
    (pool, db)
}

// ---------------------------------------------------------------------------
// Helper: insert a book + metadata row for book_repo / metadata_repo tests
// ---------------------------------------------------------------------------
async fn insert_test_book(
    pool: &sqlx::SqlitePool,
    file_path: &str,
    file_hash: &str,
    file_format: &str,
    title: &str,
) -> i64 {
    let book_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO books (file_path, file_hash, file_format, file_size_bytes) VALUES (?, ?, ?, 1024) RETURNING id",
    )
    .bind(file_path)
    .bind(file_hash)
    .bind(file_format)
    .fetch_one(pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO book_metadata (book_id, title) VALUES (?, ?)")
        .bind(book_id)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();

    book_id
}

async fn insert_test_author(pool: &sqlx::SqlitePool, name: &str) -> i64 {
    sqlx::query_scalar::<_, i64>("INSERT INTO authors (name) VALUES (?) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn link_book_author(pool: &sqlx::SqlitePool, book_id: i64, author_id: i64, sort_order: i32) {
    sqlx::query("INSERT INTO book_authors (book_id, author_id, sort_order) VALUES (?, ?, ?)")
        .bind(book_id)
        .bind(author_id)
        .bind(sort_order)
        .execute(pool)
        .await
        .unwrap();
}

async fn insert_test_tag(pool: &sqlx::SqlitePool, name: &str) -> i64 {
    sqlx::query_scalar::<_, i64>("INSERT INTO tags (name) VALUES (?) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn link_book_tag(pool: &sqlx::SqlitePool, book_id: i64, tag_id: i64) {
    sqlx::query("INSERT INTO book_tags (book_id, tag_id) VALUES (?, ?)")
        .bind(book_id)
        .bind(tag_id)
        .execute(pool)
        .await
        .unwrap();
}

// ===========================================================================
//  config_repo tests
// ===========================================================================

#[tokio::test]
async fn config_get_returns_none_for_missing_key() {
    let (_pool, db) = setup_test_db().await;
    let val = config_repo::get(&db, "nonexistent_key").await.unwrap();
    assert!(val.is_none());
}

#[tokio::test]
async fn config_set_and_get_round_trip() {
    let (_pool, db) = setup_test_db().await;
    config_repo::set(&db, "my_key", "my_value").await.unwrap();
    let val = config_repo::get(&db, "my_key").await.unwrap();
    assert_eq!(val, Some("my_value".to_string()));
}

#[tokio::test]
async fn config_set_upserts_existing_key() {
    let (_pool, db) = setup_test_db().await;
    config_repo::set(&db, "my_key", "first").await.unwrap();
    config_repo::set(&db, "my_key", "second").await.unwrap();
    let val = config_repo::get(&db, "my_key").await.unwrap();
    assert_eq!(val, Some("second".to_string()));
}

#[tokio::test]
async fn config_get_all_returns_all_rows() {
    let (_pool, db) = setup_test_db().await;
    config_repo::set(&db, "alpha", "1").await.unwrap();
    config_repo::set(&db, "beta", "2").await.unwrap();
    let all = config_repo::get_all(&db).await.unwrap();
    // Migrations insert some default config rows too (job_cadence:library_scan, metadata_retry_hours)
    // So total should be at least 4
    let keys: Vec<String> = all.iter().map(|r| r.key.clone()).collect();
    assert!(keys.contains(&"alpha".to_string()));
    assert!(keys.contains(&"beta".to_string()));
}

#[tokio::test]
async fn config_get_by_prefix_filters_correctly() {
    let (_pool, db) = setup_test_db().await;
    config_repo::set(&db, "smtp_host", "mail.example.com")
        .await
        .unwrap();
    config_repo::set(&db, "smtp_port", "587").await.unwrap();
    config_repo::set(&db, "other_key", "val").await.unwrap();

    let smtp = config_repo::get_by_prefix(&db, "smtp_").await.unwrap();
    assert_eq!(smtp.len(), 2);
    let keys: Vec<String> = smtp.iter().map(|r| r.key.clone()).collect();
    assert!(keys.contains(&"smtp_host".to_string()));
    assert!(keys.contains(&"smtp_port".to_string()));
}

#[tokio::test]
async fn config_get_or_create_jwt_secret_creates_then_returns_same() {
    let (_pool, db) = setup_test_db().await;
    let secret1 = config_repo::get_or_create_jwt_secret(&db).await.unwrap();
    assert!(!secret1.is_empty());
    let secret2 = config_repo::get_or_create_jwt_secret(&db).await.unwrap();
    assert_eq!(secret1, secret2);
}

// ===========================================================================
//  user_repo tests
// ===========================================================================

#[tokio::test]
async fn user_create_admin_and_find_by_id() {
    let (_pool, db) = setup_test_db().await;
    let user = user_repo::create_admin(&db, "admin1", "admin@example.com", "hash123")
        .await
        .unwrap();
    assert_eq!(user.username, "admin1");
    assert_eq!(user.role, "admin");
    assert_eq!(user.email, "admin@example.com");

    let found = user_repo::find_by_id(&db, user.id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().username, "admin1");
}

#[tokio::test]
async fn user_find_by_username_works() {
    let (_pool, db) = setup_test_db().await;
    user_repo::create_admin(&db, "findme", "find@example.com", "hash")
        .await
        .unwrap();
    let found = user_repo::find_by_username(&db, "findme").await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().email, "find@example.com");
}

#[tokio::test]
async fn user_find_by_username_returns_none_for_nonexistent() {
    let (_pool, db) = setup_test_db().await;
    let found = user_repo::find_by_username(&db, "ghost").await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn user_list_all_returns_all_users() {
    let (_pool, db) = setup_test_db().await;
    user_repo::create_admin(&db, "u1", "u1@ex.com", "h1")
        .await
        .unwrap();
    user_repo::create_admin(&db, "u2", "u2@ex.com", "h2")
        .await
        .unwrap();
    let all = user_repo::list_all(&db).await.unwrap();
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn user_count_admins() {
    let (_pool, db) = setup_test_db().await;
    user_repo::create_admin(&db, "a1", "a1@ex.com", "h")
        .await
        .unwrap();
    user_repo::create_admin(&db, "a2", "a2@ex.com", "h")
        .await
        .unwrap();
    let count = user_repo::count_admins(&db).await.unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn user_count_by_username() {
    let (_pool, db) = setup_test_db().await;
    user_repo::create_admin(&db, "unique_user", "u@ex.com", "h")
        .await
        .unwrap();
    let count = user_repo::count_by_username(&db, "unique_user")
        .await
        .unwrap();
    assert_eq!(count, 1);
    let count_none = user_repo::count_by_username(&db, "nobody").await.unwrap();
    assert_eq!(count_none, 0);
}

#[tokio::test]
async fn user_create_invite_and_find_by_invite_token() {
    let (_pool, db) = setup_test_db().await;
    user_repo::create_invite(&db, "pending_user", "tok123")
        .await
        .unwrap();
    let found = user_repo::find_by_invite_token(&db, "tok123")
        .await
        .unwrap();
    assert!(found.is_some());
    let user = found.unwrap();
    assert_eq!(user.username, "pending_user");
    assert_eq!(user.invite_token, Some("tok123".to_string()));
    assert_eq!(user.role, "member");
}

#[tokio::test]
async fn user_register_invite_updates_user() {
    let (_pool, db) = setup_test_db().await;
    user_repo::create_invite(&db, "pending", "tok_register")
        .await
        .unwrap();
    let pending = user_repo::find_by_invite_token(&db, "tok_register")
        .await
        .unwrap()
        .unwrap();

    user_repo::register_invite(
        &db,
        pending.id,
        "registered_user",
        "reg@example.com",
        "secure_hash",
    )
    .await
    .unwrap();

    let updated = user_repo::find_by_id(&db, pending.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.username, "registered_user");
    assert_eq!(updated.email, "reg@example.com");
    assert_eq!(updated.password_hash, "secure_hash");
    assert!(updated.invite_token.is_none());
}

#[tokio::test]
async fn user_update_field_display_name() {
    let (_pool, db) = setup_test_db().await;
    let user = user_repo::create_admin(&db, "updater", "u@ex.com", "h")
        .await
        .unwrap();
    user_repo::update_field(
        &db,
        user.id,
        user_repo::UserColumn::DisplayName,
        "Cool Name",
    )
    .await
    .unwrap();
    let found = user_repo::find_by_id(&db, user.id).await.unwrap().unwrap();
    assert_eq!(found.display_name, Some("Cool Name".to_string()));
}

#[tokio::test]
async fn user_update_field_email() {
    let (_pool, db) = setup_test_db().await;
    let user = user_repo::create_admin(&db, "emailer", "old@ex.com", "h")
        .await
        .unwrap();
    user_repo::update_field(&db, user.id, user_repo::UserColumn::Email, "new@ex.com")
        .await
        .unwrap();
    let found = user_repo::find_by_id(&db, user.id).await.unwrap().unwrap();
    assert_eq!(found.email, "new@ex.com");
}

#[tokio::test]
async fn user_update_field_kindle_email() {
    let (_pool, db) = setup_test_db().await;
    let user = user_repo::create_admin(&db, "kindler", "k@ex.com", "h")
        .await
        .unwrap();
    user_repo::update_field(
        &db,
        user.id,
        user_repo::UserColumn::KindleEmail,
        "kindle@kindle.com",
    )
    .await
    .unwrap();
    let found = user_repo::find_by_id(&db, user.id).await.unwrap().unwrap();
    assert_eq!(found.kindle_email, Some("kindle@kindle.com".to_string()));
}

#[tokio::test]
async fn user_update_field_password_hash() {
    let (_pool, db) = setup_test_db().await;
    let user = user_repo::create_admin(&db, "hasher", "h@ex.com", "old_hash")
        .await
        .unwrap();
    user_repo::update_field(
        &db,
        user.id,
        user_repo::UserColumn::PasswordHash,
        "new_hash",
    )
    .await
    .unwrap();
    let found = user_repo::find_by_id(&db, user.id).await.unwrap().unwrap();
    assert_eq!(found.password_hash, "new_hash");
}

#[tokio::test]
async fn user_update_role() {
    let (_pool, db) = setup_test_db().await;
    let user = user_repo::create_admin(&db, "promoter", "p@ex.com", "h")
        .await
        .unwrap();
    assert_eq!(user.role, "admin");
    user_repo::update_role(&db, user.id, "member")
        .await
        .unwrap();
    let found = user_repo::find_by_id(&db, user.id).await.unwrap().unwrap();
    assert_eq!(found.role, "member");
}

#[tokio::test]
async fn user_delete_removes_user() {
    let (_pool, db) = setup_test_db().await;
    let user = user_repo::create_admin(&db, "todelete", "d@ex.com", "h")
        .await
        .unwrap();
    user_repo::delete(&db, user.id).await.unwrap();
    let found = user_repo::find_by_id(&db, user.id).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn user_ensure_unique_username_returns_base_if_unique() {
    let (_pool, db) = setup_test_db().await;
    let name = user_repo::ensure_unique_username(&db, "fresh_name")
        .await
        .unwrap();
    assert_eq!(name, "fresh_name");
}

#[tokio::test]
async fn user_ensure_unique_username_appends_number_if_taken() {
    let (_pool, db) = setup_test_db().await;
    user_repo::create_admin(&db, "taken", "t@ex.com", "h")
        .await
        .unwrap();
    let name = user_repo::ensure_unique_username(&db, "taken")
        .await
        .unwrap();
    assert_eq!(name, "taken1");
}

#[tokio::test]
async fn user_create_oidc_user_and_find_by_oidc() {
    let (_pool, db) = setup_test_db().await;
    user_repo::create_oidc_user(
        &db,
        "oidcuser",
        Some("OIDC User"),
        "oidc@example.com",
        "member",
        "subject-123",
        "https://issuer.example.com",
    )
    .await
    .unwrap();

    let found = user_repo::find_by_oidc(&db, "subject-123", "https://issuer.example.com")
        .await
        .unwrap();
    assert!(found.is_some());
    let user = found.unwrap();
    assert_eq!(user.username, "oidcuser");
    assert_eq!(user.display_name, Some("OIDC User".to_string()));
    assert_eq!(user.email, "oidc@example.com");
    assert_eq!(user.role, "member");
    assert_eq!(user.oidc_subject, Some("subject-123".to_string()));
    assert_eq!(
        user.oidc_issuer,
        Some("https://issuer.example.com".to_string())
    );
}

#[tokio::test]
async fn user_find_by_oidc_returns_none_when_not_found() {
    let (_pool, db) = setup_test_db().await;
    let found = user_repo::find_by_oidc(&db, "no-such-sub", "no-such-issuer")
        .await
        .unwrap();
    assert!(found.is_none());
}

// ===========================================================================
//  audit_repo tests
// ===========================================================================

#[tokio::test]
async fn audit_log_action_inserts_entry() {
    let (_pool, db) = setup_test_db().await;
    audit_repo::log_action(&db, None, "test_action", Some("some detail"))
        .await
        .unwrap();
    let (rows, total) = audit_repo::query_with_filters(&db, None, None, 10, 0)
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].action, "test_action");
    assert_eq!(rows[0].detail, Some("some detail".to_string()));
}

#[tokio::test]
async fn audit_query_no_filters_returns_all() {
    let (_pool, db) = setup_test_db().await;
    audit_repo::log_action(&db, None, "action_a", None)
        .await
        .unwrap();
    audit_repo::log_action(&db, None, "action_b", None)
        .await
        .unwrap();
    audit_repo::log_action(&db, None, "action_c", None)
        .await
        .unwrap();
    let (rows, total) = audit_repo::query_with_filters(&db, None, None, 100, 0)
        .await
        .unwrap();
    assert_eq!(total, 3);
    assert_eq!(rows.len(), 3);
}

#[tokio::test]
async fn audit_query_with_action_filter() {
    let (_pool, db) = setup_test_db().await;
    audit_repo::log_action(&db, None, "login", None)
        .await
        .unwrap();
    audit_repo::log_action(&db, None, "login", None)
        .await
        .unwrap();
    audit_repo::log_action(&db, None, "logout", None)
        .await
        .unwrap();

    let (rows, total) = audit_repo::query_with_filters(&db, Some("login"), None, 100, 0)
        .await
        .unwrap();
    assert_eq!(total, 2);
    assert_eq!(rows.len(), 2);
    for r in &rows {
        assert_eq!(r.action, "login");
    }
}

#[tokio::test]
async fn audit_query_with_user_id_filter() {
    let (_pool, db) = setup_test_db().await;
    let user = user_repo::create_admin(&db, "audited", "a@ex.com", "h")
        .await
        .unwrap();
    audit_repo::log_action(&db, Some(user.id), "action1", None)
        .await
        .unwrap();
    audit_repo::log_action(&db, None, "action2", None)
        .await
        .unwrap();

    let (rows, total) = audit_repo::query_with_filters(&db, None, Some(user.id), 100, 0)
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].user_id, Some(user.id));
}

#[tokio::test]
async fn audit_query_pagination() {
    let (_pool, db) = setup_test_db().await;
    for i in 0..5 {
        audit_repo::log_action(&db, None, &format!("act{i}"), None)
            .await
            .unwrap();
    }

    let (page1, total) = audit_repo::query_with_filters(&db, None, None, 2, 0)
        .await
        .unwrap();
    assert_eq!(total, 5);
    assert_eq!(page1.len(), 2);

    let (page2, _) = audit_repo::query_with_filters(&db, None, None, 2, 2)
        .await
        .unwrap();
    assert_eq!(page2.len(), 2);

    let (page3, _) = audit_repo::query_with_filters(&db, None, None, 2, 4)
        .await
        .unwrap();
    assert_eq!(page3.len(), 1);
}

#[tokio::test]
async fn audit_query_joins_username() {
    let (_pool, db) = setup_test_db().await;
    let user = user_repo::create_admin(&db, "loggeduser", "l@ex.com", "h")
        .await
        .unwrap();
    audit_repo::log_action(&db, Some(user.id), "login", None)
        .await
        .unwrap();

    let (rows, _) = audit_repo::query_with_filters(&db, None, None, 10, 0)
        .await
        .unwrap();
    assert_eq!(rows[0].username, Some("loggeduser".to_string()));
}

#[tokio::test]
async fn audit_query_username_is_none_for_null_user_id() {
    let (_pool, db) = setup_test_db().await;
    audit_repo::log_action(&db, None, "system", None)
        .await
        .unwrap();

    let (rows, _) = audit_repo::query_with_filters(&db, None, None, 10, 0)
        .await
        .unwrap();
    assert!(rows[0].username.is_none());
    assert!(rows[0].user_id.is_none());
}

// ===========================================================================
//  job_repo tests
// ===========================================================================

#[tokio::test]
async fn job_create_run_and_last_run() {
    let (_pool, db) = setup_test_db().await;
    let run_id = job_repo::create_run(&db, "test_job", Some("api"))
        .await
        .unwrap();
    assert!(run_id > 0);

    let last = job_repo::last_run(&db, "test_job").await.unwrap();
    assert!(last.is_some());
    let run = last.unwrap();
    assert_eq!(run.id, run_id);
    assert_eq!(run.job_name, "test_job");
    assert_eq!(run.status, "running");
    assert_eq!(run.triggered_by, Some("api".to_string()));
}

#[tokio::test]
async fn job_is_running_true_for_running_job() {
    let (_pool, db) = setup_test_db().await;
    job_repo::create_run(&db, "running_job", None)
        .await
        .unwrap();
    let running = job_repo::is_running(&db, "running_job").await.unwrap();
    assert!(running);
}

#[tokio::test]
async fn job_is_running_false_after_finish() {
    let (_pool, db) = setup_test_db().await;
    let run_id = job_repo::create_run(&db, "finish_job", None).await.unwrap();
    job_repo::finish_run(&db, run_id, "completed", "\"ok\"")
        .await
        .unwrap();
    let running = job_repo::is_running(&db, "finish_job").await.unwrap();
    assert!(!running);
}

#[tokio::test]
async fn job_finish_run_updates_status_and_result() {
    let (_pool, db) = setup_test_db().await;
    let run_id = job_repo::create_run(&db, "finish_test", None)
        .await
        .unwrap();
    job_repo::finish_run(&db, run_id, "failed", "\"some error\"")
        .await
        .unwrap();

    let run = job_repo::last_run(&db, "finish_test")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.status, "failed");
    assert_eq!(run.result, Some("\"some error\"".to_string()));
    assert!(run.finished_at.is_some());
}

#[tokio::test]
async fn job_list_runs_with_pagination() {
    let (_pool, db) = setup_test_db().await;
    for _ in 0..5 {
        job_repo::create_run(&db, "paginated_job", None)
            .await
            .unwrap();
    }

    let (runs, total) = job_repo::list_runs(&db, "paginated_job", 2, 0)
        .await
        .unwrap();
    assert_eq!(total, 5);
    assert_eq!(runs.len(), 2);

    let (runs2, _) = job_repo::list_runs(&db, "paginated_job", 2, 4)
        .await
        .unwrap();
    assert_eq!(runs2.len(), 1);
}

#[tokio::test]
async fn job_last_finished_at_returns_correct_timestamp() {
    let (_pool, db) = setup_test_db().await;
    // No runs yet
    let none = job_repo::last_finished_at(&db, "ts_job").await.unwrap();
    assert!(none.is_none());

    let run_id = job_repo::create_run(&db, "ts_job", None).await.unwrap();
    job_repo::finish_run(&db, run_id, "completed", "\"done\"")
        .await
        .unwrap();
    let ts = job_repo::last_finished_at(&db, "ts_job").await.unwrap();
    assert!(ts.is_some());
}

#[tokio::test]
async fn job_cleanup_stale_marks_running_as_failed() {
    let (_pool, db) = setup_test_db().await;
    let run_id = job_repo::create_run(&db, "stale_job", None).await.unwrap();
    assert!(job_repo::is_running(&db, "stale_job").await.unwrap());

    job_repo::cleanup_stale(&db).await.unwrap();

    let run = job_repo::last_run(&db, "stale_job").await.unwrap().unwrap();
    assert_eq!(run.id, run_id);
    assert_eq!(run.status, "failed");
    assert!(run.finished_at.is_some());
    assert_eq!(run.result, Some("\"interrupted by restart\"".to_string()));
    assert!(!job_repo::is_running(&db, "stale_job").await.unwrap());
}

// ===========================================================================
//  book_repo tests
// ===========================================================================

#[tokio::test]
async fn book_find_by_id_returns_none_for_nonexistent() {
    let (_pool, db) = setup_test_db().await;
    let found = book_repo::find_by_id(&db, 9999).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn book_find_by_id_returns_inserted_book() {
    let (pool, db) = setup_test_db().await;
    let book_id = insert_test_book(&pool, "/books/test.epub", "abc123", "epub", "Test Book").await;
    let found = book_repo::find_by_id(&db, book_id).await.unwrap();
    assert!(found.is_some());
    let book = found.unwrap();
    assert_eq!(book.file_path, "/books/test.epub");
    assert_eq!(book.file_format, "epub");
}

#[tokio::test]
async fn book_list_filtered_no_filters() {
    let (pool, db) = setup_test_db().await;
    insert_test_book(&pool, "/b/a.epub", "h1", "epub", "Alpha").await;
    insert_test_book(&pool, "/b/b.pdf", "h2", "pdf", "Beta").await;

    let (rows, total) = book_repo::list_filtered(&db, None, None, None, None, 10, 0)
        .await
        .unwrap();
    assert_eq!(total, 2);
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn book_list_filtered_with_author_filter() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/1.epub", "ha", "epub", "Book A").await;
    let b2 = insert_test_book(&pool, "/b/2.epub", "hb", "epub", "Book B").await;
    let a1 = insert_test_author(&pool, "Author One").await;
    let a2 = insert_test_author(&pool, "Author Two").await;
    link_book_author(&pool, b1, a1, 0).await;
    link_book_author(&pool, b2, a2, 0).await;

    let (rows, total) = book_repo::list_filtered(&db, None, Some("Author One"), None, None, 10, 0)
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, b1);
}

#[tokio::test]
async fn book_list_filtered_with_tag_filter() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/t1.epub", "ht1", "epub", "Tagged Book").await;
    let _b2 = insert_test_book(&pool, "/b/t2.epub", "ht2", "epub", "Other Book").await;
    let tag = insert_test_tag(&pool, "fiction").await;
    link_book_tag(&pool, b1, tag).await;

    let (rows, total) = book_repo::list_filtered(&db, None, None, Some("fiction"), None, 10, 0)
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, b1);
}

#[tokio::test]
async fn book_list_filtered_with_format_filter() {
    let (pool, db) = setup_test_db().await;
    insert_test_book(&pool, "/b/f1.epub", "hf1", "epub", "Epub Book").await;
    insert_test_book(&pool, "/b/f2.pdf", "hf2", "pdf", "Pdf Book").await;

    let (rows, total) = book_repo::list_filtered(&db, None, None, None, Some("pdf"), 10, 0)
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].file_format, "pdf");
}

#[tokio::test]
async fn book_list_filtered_sort_by_title() {
    let (pool, db) = setup_test_db().await;
    insert_test_book(&pool, "/b/z.epub", "hz", "epub", "Zebra").await;
    insert_test_book(&pool, "/b/a.epub", "ha2", "epub", "Apple").await;

    let (rows, _) = book_repo::list_filtered(&db, Some("title"), None, None, None, 10, 0)
        .await
        .unwrap();
    assert_eq!(rows[0].title, Some("Apple".to_string()));
    assert_eq!(rows[1].title, Some("Zebra".to_string()));
}

#[tokio::test]
async fn book_list_filtered_pagination() {
    let (pool, db) = setup_test_db().await;
    for i in 0..5 {
        insert_test_book(
            &pool,
            &format!("/b/p{i}.epub"),
            &format!("hp{i}"),
            "epub",
            &format!("Book {i}"),
        )
        .await;
    }

    let (page1, total) = book_repo::list_filtered(&db, None, None, None, None, 2, 0)
        .await
        .unwrap();
    assert_eq!(total, 5);
    assert_eq!(page1.len(), 2);

    let (page3, _) = book_repo::list_filtered(&db, None, None, None, None, 2, 4)
        .await
        .unwrap();
    assert_eq!(page3.len(), 1);
}

#[tokio::test]
async fn book_list_authors_with_counts() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/ac1.epub", "hac1", "epub", "B1").await;
    let b2 = insert_test_book(&pool, "/b/ac2.epub", "hac2", "epub", "B2").await;
    let a1 = insert_test_author(&pool, "Author A").await;
    let a2 = insert_test_author(&pool, "Author B").await;
    link_book_author(&pool, b1, a1, 0).await;
    link_book_author(&pool, b2, a1, 0).await;
    link_book_author(&pool, b2, a2, 1).await;

    let authors = book_repo::list_authors_with_counts(&db).await.unwrap();
    assert_eq!(authors.len(), 2);
    let author_a = authors.iter().find(|a| a.name == "Author A").unwrap();
    assert_eq!(author_a.book_count, 2);
    let author_b = authors.iter().find(|a| a.name == "Author B").unwrap();
    assert_eq!(author_b.book_count, 1);
}

#[tokio::test]
async fn book_list_tags_with_counts() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/tc1.epub", "htc1", "epub", "B1").await;
    let b2 = insert_test_book(&pool, "/b/tc2.epub", "htc2", "epub", "B2").await;
    let t1 = insert_test_tag(&pool, "scifi").await;
    let t2 = insert_test_tag(&pool, "fantasy").await;
    link_book_tag(&pool, b1, t1).await;
    link_book_tag(&pool, b2, t1).await;
    link_book_tag(&pool, b2, t2).await;

    let tags = book_repo::list_tags_with_counts(&db).await.unwrap();
    assert_eq!(tags.len(), 2);
    let scifi = tags.iter().find(|t| t.name == "scifi").unwrap();
    assert_eq!(scifi.book_count, 2);
    let fantasy = tags.iter().find(|t| t.name == "fantasy").unwrap();
    assert_eq!(fantasy.book_count, 1);
}

#[tokio::test]
async fn book_list_formats_with_counts() {
    let (pool, db) = setup_test_db().await;
    insert_test_book(&pool, "/b/fc1.epub", "hfc1", "epub", "E1").await;
    insert_test_book(&pool, "/b/fc2.epub", "hfc2", "epub", "E2").await;
    insert_test_book(&pool, "/b/fc3.pdf", "hfc3", "pdf", "P1").await;

    let formats = book_repo::list_formats_with_counts(&db).await.unwrap();
    assert_eq!(formats.len(), 2);
    let epub = formats.iter().find(|f| f.name == "epub").unwrap();
    assert_eq!(epub.book_count, 2);
    let pdf = formats.iter().find(|f| f.name == "pdf").unwrap();
    assert_eq!(pdf.book_count, 1);
}

#[tokio::test]
async fn book_library_stats() {
    let (pool, db) = setup_test_db().await;
    insert_test_book(&pool, "/b/s1.epub", "hs1", "epub", "S1").await;
    insert_test_book(&pool, "/b/s2.pdf", "hs2", "pdf", "S2").await;

    // Mark one as missing
    sqlx::query("UPDATE books SET missing = 1 WHERE file_path = '/b/s2.pdf'")
        .execute(&pool)
        .await
        .unwrap();

    let a1 = insert_test_author(&pool, "Stats Author").await;
    let b1_id = sqlx::query_scalar::<_, i64>("SELECT id FROM books WHERE file_path = '/b/s1.epub'")
        .fetch_one(&pool)
        .await
        .unwrap();
    link_book_author(&pool, b1_id, a1, 0).await;

    let stats = book_repo::library_stats(&db).await.unwrap();
    assert_eq!(stats.total_books, 2);
    assert_eq!(stats.available_books, 1);
    assert_eq!(stats.missing_books, 1);
    assert_eq!(stats.total_authors, 1);
    assert_eq!(stats.format_breakdown.len(), 1); // only non-missing epub
    assert_eq!(stats.format_breakdown[0].format, "epub");
    assert_eq!(stats.format_breakdown[0].count, 1);
}

#[tokio::test]
async fn book_get_authors_and_get_tags() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/at1.epub", "hat1", "epub", "AT1").await;
    let a1 = insert_test_author(&pool, "First Author").await;
    let a2 = insert_test_author(&pool, "Second Author").await;
    link_book_author(&pool, b1, a1, 0).await;
    link_book_author(&pool, b1, a2, 1).await;
    let t1 = insert_test_tag(&pool, "horror").await;
    link_book_tag(&pool, b1, t1).await;

    let authors = book_repo::get_authors(&db, b1).await.unwrap();
    assert_eq!(authors, vec!["First Author", "Second Author"]);

    let tags = book_repo::get_tags(&db, b1).await.unwrap();
    assert_eq!(tags, vec!["horror"]);
}

#[tokio::test]
async fn book_get_metadata() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/m1.epub", "hm1", "epub", "Meta Book").await;
    let meta = book_repo::get_metadata(&db, b1).await.unwrap();
    assert!(meta.is_some());
    let m = meta.unwrap();
    assert_eq!(m.title, Some("Meta Book".to_string()));
    assert_eq!(m.book_id, b1);
}

#[tokio::test]
async fn book_get_file_info_returns_none_for_missing() {
    let (_pool, db) = setup_test_db().await;
    let info = book_repo::get_file_info(&db, 9999).await.unwrap();
    assert!(info.is_none());
}

#[tokio::test]
async fn book_get_file_info_returns_info_for_existing() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/fi.epub", "hfi", "epub", "FI Book").await;
    let info = book_repo::get_file_info(&db, b1).await.unwrap();
    assert!(info.is_some());
    let fi = info.unwrap();
    assert_eq!(fi.id, b1);
    assert_eq!(fi.file_path, "/b/fi.epub");
    assert_eq!(fi.file_format, "epub");
}

#[tokio::test]
async fn book_get_file_info_returns_none_for_missing_book() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/miss.epub", "hmiss", "epub", "Missing").await;
    sqlx::query("UPDATE books SET missing = 1 WHERE id = ?")
        .bind(b1)
        .execute(&pool)
        .await
        .unwrap();
    let info = book_repo::get_file_info(&db, b1).await.unwrap();
    assert!(info.is_none());
}

#[tokio::test]
async fn book_get_cover_path() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/cv.epub", "hcv", "epub", "Cover Book").await;

    // No cover yet
    let path = book_repo::get_cover_path(&db, b1).await.unwrap();
    assert!(path.is_none());

    // Set cover path
    sqlx::query("UPDATE book_metadata SET cover_image_path = '/covers/1.jpg' WHERE book_id = ?")
        .bind(b1)
        .execute(&pool)
        .await
        .unwrap();

    let path = book_repo::get_cover_path(&db, b1).await.unwrap();
    assert_eq!(path, Some("/covers/1.jpg".to_string()));
}

// ===========================================================================
//  metadata_repo tests
// ===========================================================================

#[tokio::test]
async fn metadata_get_meta_lookup() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/ml.epub", "hml", "epub", "Lookup Book").await;
    sqlx::query("UPDATE book_metadata SET isbn_10 = '1234567890', isbn_13 = '9781234567890' WHERE book_id = ?")
        .bind(b1)
        .execute(&pool)
        .await
        .unwrap();

    let lookup = metadata_repo::get_meta_lookup(&db, b1).await.unwrap();
    assert!(lookup.is_some());
    let l = lookup.unwrap();
    assert_eq!(l.title, Some("Lookup Book".to_string()));
    assert_eq!(l.isbn_10, Some("1234567890".to_string()));
    assert_eq!(l.isbn_13, Some("9781234567890".to_string()));
}

#[tokio::test]
async fn metadata_get_meta_lookup_returns_none_for_missing() {
    let (_pool, db) = setup_test_db().await;
    let lookup = metadata_repo::get_meta_lookup(&db, 9999).await.unwrap();
    assert!(lookup.is_none());
}

#[tokio::test]
async fn metadata_update_if_null_only_updates_null_field() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/uin.epub", "huin", "epub", "UIN Book").await;

    // description is NULL, update should succeed
    metadata_repo::update_if_null(
        &db,
        b1,
        metadata_repo::MetadataColumn::Description,
        "First description",
    )
    .await
    .unwrap();

    let meta = book_repo::get_metadata(&db, b1).await.unwrap().unwrap();
    assert_eq!(meta.description, Some("First description".to_string()));

    // Try to update again — should NOT change because it's no longer NULL
    metadata_repo::update_if_null(
        &db,
        b1,
        metadata_repo::MetadataColumn::Description,
        "Second description",
    )
    .await
    .unwrap();

    let meta = book_repo::get_metadata(&db, b1).await.unwrap().unwrap();
    assert_eq!(meta.description, Some("First description".to_string()));
}

#[tokio::test]
async fn metadata_set_source_updates_metadata_source() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/src.epub", "hsrc", "epub", "Source Book").await;

    // Initially metadata_source is NULL, set it
    metadata_repo::set_source(&db, b1, "embedded", false)
        .await
        .unwrap();
    let meta = book_repo::get_metadata(&db, b1).await.unwrap().unwrap();
    assert_eq!(meta.metadata_source, Some("embedded".to_string()));
    assert!(meta.metadata_fetched_at.is_some());
}

#[tokio::test]
async fn metadata_set_source_only_if_lower_upgrades_from_embedded() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/src2.epub", "hsrc2", "epub", "Source2 Book").await;

    // Set to embedded first
    metadata_repo::set_source(&db, b1, "embedded", false)
        .await
        .unwrap();
    let meta = book_repo::get_metadata(&db, b1).await.unwrap().unwrap();
    assert_eq!(meta.metadata_source, Some("embedded".to_string()));

    // Upgrade with only_if_lower=true should work since current is 'embedded'
    metadata_repo::set_source(&db, b1, "openlibrary", true)
        .await
        .unwrap();
    let meta = book_repo::get_metadata(&db, b1).await.unwrap().unwrap();
    assert_eq!(meta.metadata_source, Some("openlibrary".to_string()));
}

#[tokio::test]
async fn metadata_set_title_overwrites_for_given_source() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/st.epub", "hst", "epub", "Original Title").await;
    metadata_repo::set_source(&db, b1, "embedded", false)
        .await
        .unwrap();

    metadata_repo::set_title(&db, b1, "Better Title", "embedded")
        .await
        .unwrap();

    let meta = book_repo::get_metadata(&db, b1).await.unwrap().unwrap();
    assert_eq!(meta.title, Some("Better Title".to_string()));
}

#[tokio::test]
async fn metadata_set_title_does_not_overwrite_for_different_source() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/st2.epub", "hst2", "epub", "Original").await;
    metadata_repo::set_source(&db, b1, "embedded", false)
        .await
        .unwrap();

    // Try to set title with a different source — should not match
    metadata_repo::set_title(&db, b1, "Wrong Source Title", "openlibrary")
        .await
        .unwrap();

    let meta = book_repo::get_metadata(&db, b1).await.unwrap().unwrap();
    assert_eq!(meta.title, Some("Original".to_string()));
}

#[tokio::test]
async fn metadata_set_cover_path_saves_cover() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/cp.epub", "hcp", "epub", "Cover Path").await;

    metadata_repo::set_cover_path(&db, b1, "/covers/42.jpg")
        .await
        .unwrap();

    let path = book_repo::get_cover_path(&db, b1).await.unwrap();
    assert_eq!(path, Some("/covers/42.jpg".to_string()));
}

#[tokio::test]
async fn metadata_upsert_author_creates_and_links() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/ua.epub", "hua", "epub", "Upsert Author").await;

    metadata_repo::upsert_author(&db, b1, "New Author", 0)
        .await
        .unwrap();

    let authors = book_repo::get_authors(&db, b1).await.unwrap();
    assert_eq!(authors, vec!["New Author"]);

    // Upserting same author again should not fail (IGNORE)
    metadata_repo::upsert_author(&db, b1, "New Author", 0)
        .await
        .unwrap();
    let authors = book_repo::get_authors(&db, b1).await.unwrap();
    assert_eq!(authors, vec!["New Author"]);
}

#[tokio::test]
async fn metadata_upsert_author_multiple_authors() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/ua2.epub", "hua2", "epub", "Multi Author").await;

    metadata_repo::upsert_author(&db, b1, "Author A", 0)
        .await
        .unwrap();
    metadata_repo::upsert_author(&db, b1, "Author B", 1)
        .await
        .unwrap();

    let authors = book_repo::get_authors(&db, b1).await.unwrap();
    assert_eq!(authors, vec!["Author A", "Author B"]);
}

#[tokio::test]
async fn metadata_record_provider_attempt_and_check() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/pa.epub", "hpa", "epub", "Provider Book").await;

    let attempted = metadata_repo::provider_attempted(&db, b1, "openlibrary")
        .await
        .unwrap();
    assert!(!attempted);

    metadata_repo::record_provider_attempt(&db, b1, "openlibrary")
        .await
        .unwrap();

    let attempted = metadata_repo::provider_attempted(&db, b1, "openlibrary")
        .await
        .unwrap();
    assert!(attempted);

    // Different provider should still return false
    let attempted2 = metadata_repo::provider_attempted(&db, b1, "googlebooks")
        .await
        .unwrap();
    assert!(!attempted2);
}

#[tokio::test]
async fn metadata_needs_enrichment_true_when_missing_data() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/ne.epub", "hne", "epub", "Needs Enrichment").await;

    // No description, no cover, no authors — needs enrichment
    let needs = metadata_repo::needs_enrichment(&db, b1).await.unwrap();
    assert!(needs);
}

#[tokio::test]
async fn metadata_needs_enrichment_false_when_complete() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/ne2.epub", "hne2", "epub", "Complete Book").await;

    // Add description, cover, and author
    sqlx::query(
        "UPDATE book_metadata SET description = 'A great book', cover_image_path = '/covers/1.jpg' WHERE book_id = ?",
    )
    .bind(b1)
    .execute(&pool)
    .await
    .unwrap();
    metadata_repo::upsert_author(&db, b1, "Some Author", 0)
        .await
        .unwrap();

    let needs = metadata_repo::needs_enrichment(&db, b1).await.unwrap();
    assert!(!needs);
}

#[tokio::test]
async fn metadata_needs_embedded_extraction_true_when_source_null() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/ee.epub", "hee", "epub", "Extract Me").await;

    let needs = metadata_repo::needs_embedded_extraction(&db, b1)
        .await
        .unwrap();
    assert!(needs);
}

#[tokio::test]
async fn metadata_needs_embedded_extraction_false_after_set_source() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/ee2.epub", "hee2", "epub", "Already Extracted").await;
    metadata_repo::set_source(&db, b1, "embedded", false)
        .await
        .unwrap();

    let needs = metadata_repo::needs_embedded_extraction(&db, b1)
        .await
        .unwrap();
    assert!(!needs);
}

#[tokio::test]
async fn metadata_books_needing_metadata_returns_correct_ids() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/bn1.epub", "hbn1", "epub", "Need Meta 1").await;
    let b2 = insert_test_book(&pool, "/b/bn2.epub", "hbn2", "epub", "Need Meta 2").await;
    let b3 = insert_test_book(&pool, "/b/bn3.epub", "hbn3", "epub", "Complete 3").await;

    // b3 is fully enriched
    sqlx::query(
        "UPDATE book_metadata SET description = 'desc', cover_image_path = '/c.jpg', metadata_source = 'embedded' WHERE book_id = ?",
    )
    .bind(b3)
    .execute(&pool)
    .await
    .unwrap();
    metadata_repo::upsert_author(&db, b3, "Author", 0)
        .await
        .unwrap();

    // b1 and b2 should need metadata
    let ids = metadata_repo::books_needing_metadata(&db, 24, 2)
        .await
        .unwrap();
    assert!(ids.contains(&b1));
    assert!(ids.contains(&b2));
    assert!(!ids.contains(&b3));
}

#[tokio::test]
async fn metadata_books_needing_metadata_respects_missing_flag() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/bnm.epub", "hbnm", "epub", "Missing Book").await;

    // Mark as missing
    sqlx::query("UPDATE books SET missing = 1 WHERE id = ?")
        .bind(b1)
        .execute(&pool)
        .await
        .unwrap();

    let ids = metadata_repo::books_needing_metadata(&db, 24, 2)
        .await
        .unwrap();
    assert!(!ids.contains(&b1));
}

// ===========================================================================
//  metadata_repo::import_scanned_files tests
// ===========================================================================

#[tokio::test]
async fn import_scanned_files_inserts_new_books() {
    let (pool, db) = setup_test_db().await;
    let files = vec![
        ScannedFile {
            path: PathBuf::from("/library/book1.epub"),
            format: "epub".to_string(),
            size_bytes: 1000,
            hash: "hash_aaa".to_string(),
        },
        ScannedFile {
            path: PathBuf::from("/library/book2.pdf"),
            format: "pdf".to_string(),
            size_bytes: 2000,
            hash: "hash_bbb".to_string(),
        },
    ];

    let result = metadata_repo::import_scanned_files(&pool, &files, false)
        .await
        .unwrap();
    assert_eq!(result.imported, 2);
    assert_eq!(result.updated, 0);
    assert_eq!(result.total_scanned, 2);

    // Verify in DB
    let (rows, total) = book_repo::list_filtered(&db, None, None, None, None, 10, 0)
        .await
        .unwrap();
    assert_eq!(total, 2);
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn import_scanned_files_updates_existing_by_path() {
    let (pool, _db) = setup_test_db().await;
    let files = vec![ScannedFile {
        path: PathBuf::from("/library/existing.epub"),
        format: "epub".to_string(),
        size_bytes: 1000,
        hash: "hash_first".to_string(),
    }];

    let r1 = metadata_repo::import_scanned_files(&pool, &files, false)
        .await
        .unwrap();
    assert_eq!(r1.imported, 1);

    // Re-scan with same path but different hash
    let files2 = vec![ScannedFile {
        path: PathBuf::from("/library/existing.epub"),
        format: "epub".to_string(),
        size_bytes: 1500,
        hash: "hash_second".to_string(),
    }];

    let r2 = metadata_repo::import_scanned_files(&pool, &files2, false)
        .await
        .unwrap();
    assert_eq!(r2.imported, 0);
    assert_eq!(r2.updated, 1);
}

#[tokio::test]
async fn import_scanned_files_updates_existing_by_hash() {
    let (pool, _db) = setup_test_db().await;
    let files = vec![ScannedFile {
        path: PathBuf::from("/library/original.epub"),
        format: "epub".to_string(),
        size_bytes: 1000,
        hash: "hash_moved".to_string(),
    }];

    metadata_repo::import_scanned_files(&pool, &files, false)
        .await
        .unwrap();

    // Same hash, different path (file was moved)
    let files2 = vec![ScannedFile {
        path: PathBuf::from("/library/moved.epub"),
        format: "epub".to_string(),
        size_bytes: 1000,
        hash: "hash_moved".to_string(),
    }];

    let r2 = metadata_repo::import_scanned_files(&pool, &files2, false)
        .await
        .unwrap();
    assert_eq!(r2.imported, 0);
    assert_eq!(r2.updated, 1);

    // Verify the path was updated
    let book = sqlx::query_scalar::<_, String>(
        "SELECT file_path FROM books WHERE file_hash = 'hash_moved'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(book, "/library/moved.epub");
}

#[tokio::test]
async fn import_scanned_files_mark_missing() {
    let (pool, _db) = setup_test_db().await;

    // First import
    let files = vec![
        ScannedFile {
            path: PathBuf::from("/library/keep.epub"),
            format: "epub".to_string(),
            size_bytes: 1000,
            hash: "hash_keep".to_string(),
        },
        ScannedFile {
            path: PathBuf::from("/library/remove.epub"),
            format: "epub".to_string(),
            size_bytes: 2000,
            hash: "hash_remove".to_string(),
        },
    ];
    metadata_repo::import_scanned_files(&pool, &files, false)
        .await
        .unwrap();

    // Second import with only one file, mark_missing=true
    let files2 = vec![ScannedFile {
        path: PathBuf::from("/library/keep.epub"),
        format: "epub".to_string(),
        size_bytes: 1000,
        hash: "hash_keep".to_string(),
    }];
    metadata_repo::import_scanned_files(&pool, &files2, true)
        .await
        .unwrap();

    // The removed book should be marked as missing
    let missing = sqlx::query_scalar::<_, bool>(
        "SELECT missing FROM books WHERE file_path = '/library/remove.epub'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(missing);

    // The kept book should NOT be missing
    let kept = sqlx::query_scalar::<_, bool>(
        "SELECT missing FROM books WHERE file_path = '/library/keep.epub'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!kept);
}

#[tokio::test]
async fn import_scanned_files_creates_metadata_with_title_from_filename() {
    let (pool, db) = setup_test_db().await;
    let files = vec![ScannedFile {
        path: PathBuf::from("/library/My Great Book.epub"),
        format: "epub".to_string(),
        size_bytes: 500,
        hash: "hash_title_test".to_string(),
    }];

    metadata_repo::import_scanned_files(&pool, &files, false)
        .await
        .unwrap();

    let (rows, _) = book_repo::list_filtered(&db, None, None, None, None, 10, 0)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, Some("My Great Book".to_string()));
}

// ===========================================================================
//  metadata_repo edge case tests
// ===========================================================================

#[tokio::test]
async fn metadata_update_if_null_all_columns() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/allcol.epub", "hallcol", "epub", "AllCol").await;

    // Update publisher (NULL -> value)
    metadata_repo::update_if_null(
        &db,
        b1,
        metadata_repo::MetadataColumn::Publisher,
        "Great Publisher",
    )
    .await
    .unwrap();
    let meta = book_repo::get_metadata(&db, b1).await.unwrap().unwrap();
    assert_eq!(meta.publisher, Some("Great Publisher".to_string()));

    // Update language (NULL -> value)
    metadata_repo::update_if_null(&db, b1, metadata_repo::MetadataColumn::Language, "en")
        .await
        .unwrap();
    let meta = book_repo::get_metadata(&db, b1).await.unwrap().unwrap();
    assert_eq!(meta.language, Some("en".to_string()));

    // Update published_date (NULL -> value)
    metadata_repo::update_if_null(
        &db,
        b1,
        metadata_repo::MetadataColumn::PublishedDate,
        "2024-01-01",
    )
    .await
    .unwrap();
    let meta = book_repo::get_metadata(&db, b1).await.unwrap().unwrap();
    assert_eq!(meta.published_date, Some("2024-01-01".to_string()));

    // Update isbn_10 (NULL -> value)
    metadata_repo::update_if_null(&db, b1, metadata_repo::MetadataColumn::Isbn10, "0123456789")
        .await
        .unwrap();
    let meta = book_repo::get_metadata(&db, b1).await.unwrap().unwrap();
    assert_eq!(meta.isbn_10, Some("0123456789".to_string()));

    // Update isbn_13 (NULL -> value)
    metadata_repo::update_if_null(
        &db,
        b1,
        metadata_repo::MetadataColumn::Isbn13,
        "9780123456789",
    )
    .await
    .unwrap();
    let meta = book_repo::get_metadata(&db, b1).await.unwrap().unwrap();
    assert_eq!(meta.isbn_13, Some("9780123456789".to_string()));
}

#[tokio::test]
async fn metadata_needs_enrichment_returns_false_for_nonexistent_book() {
    let (_pool, db) = setup_test_db().await;
    let needs = metadata_repo::needs_enrichment(&db, 99999).await.unwrap();
    assert!(!needs);
}

#[tokio::test]
async fn metadata_needs_embedded_extraction_returns_false_for_nonexistent_book() {
    let (_pool, db) = setup_test_db().await;
    let needs = metadata_repo::needs_embedded_extraction(&db, 99999)
        .await
        .unwrap();
    assert!(!needs);
}

#[tokio::test]
async fn metadata_provider_attempted_returns_false_for_nonexistent_book() {
    let (_pool, db) = setup_test_db().await;
    let attempted = metadata_repo::provider_attempted(&db, 99999, "openlibrary")
        .await
        .unwrap();
    assert!(!attempted);
}

#[tokio::test]
async fn metadata_upsert_author_shares_author_across_books() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/share1.epub", "hsh1", "epub", "Share1").await;
    let b2 = insert_test_book(&pool, "/b/share2.epub", "hsh2", "epub", "Share2").await;

    metadata_repo::upsert_author(&db, b1, "Shared Author", 0)
        .await
        .unwrap();
    metadata_repo::upsert_author(&db, b2, "Shared Author", 0)
        .await
        .unwrap();

    // Both books should have the author
    let a1 = book_repo::get_authors(&db, b1).await.unwrap();
    let a2 = book_repo::get_authors(&db, b2).await.unwrap();
    assert_eq!(a1, vec!["Shared Author"]);
    assert_eq!(a2, vec!["Shared Author"]);

    // There should be only one author row
    let count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM authors WHERE name = 'Shared Author'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

// ===========================================================================
//  book_repo edge cases
// ===========================================================================

#[tokio::test]
async fn book_list_filtered_with_combined_filters() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/combo1.epub", "hcb1", "epub", "Combo1").await;
    let b2 = insert_test_book(&pool, "/b/combo2.pdf", "hcb2", "pdf", "Combo2").await;
    let b3 = insert_test_book(&pool, "/b/combo3.epub", "hcb3", "epub", "Combo3").await;
    let a1 = insert_test_author(&pool, "Combo Author").await;
    link_book_author(&pool, b1, a1, 0).await;
    link_book_author(&pool, b3, a1, 0).await;

    // Filter by author AND format — should only return b1 (epub by Combo Author)
    let (rows, total) =
        book_repo::list_filtered(&db, None, Some("Combo Author"), None, Some("epub"), 10, 0)
            .await
            .unwrap();
    assert_eq!(total, 2); // b1 and b3 are both epub by Combo Author
    let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    assert!(ids.contains(&b1));
    assert!(ids.contains(&b3));
    assert!(!ids.contains(&b2));
}

#[tokio::test]
async fn book_list_filtered_excludes_missing_books() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/vis.epub", "hvis", "epub", "Visible").await;
    let b2 = insert_test_book(&pool, "/b/gone.epub", "hgone", "epub", "Gone").await;
    sqlx::query("UPDATE books SET missing = 1 WHERE id = ?")
        .bind(b2)
        .execute(&pool)
        .await
        .unwrap();

    let (rows, total) = book_repo::list_filtered(&db, None, None, None, None, 10, 0)
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(rows[0].id, b1);
}

#[tokio::test]
async fn book_get_metadata_returns_none_for_nonexistent() {
    let (_pool, db) = setup_test_db().await;
    let meta = book_repo::get_metadata(&db, 99999).await.unwrap();
    assert!(meta.is_none());
}

#[tokio::test]
async fn book_get_authors_empty_for_book_without_authors() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/noauth.epub", "hnoauth", "epub", "No Authors").await;
    let authors = book_repo::get_authors(&db, b1).await.unwrap();
    assert!(authors.is_empty());
}

#[tokio::test]
async fn book_get_tags_empty_for_book_without_tags() {
    let (pool, db) = setup_test_db().await;
    let b1 = insert_test_book(&pool, "/b/notag.epub", "hnotag", "epub", "No Tags").await;
    let tags = book_repo::get_tags(&db, b1).await.unwrap();
    assert!(tags.is_empty());
}

#[tokio::test]
async fn book_get_cover_path_returns_none_for_nonexistent_book() {
    let (_pool, db) = setup_test_db().await;
    let path = book_repo::get_cover_path(&db, 99999).await.unwrap();
    assert!(path.is_none());
}

// ===========================================================================
//  job_repo edge cases
// ===========================================================================

#[tokio::test]
async fn job_is_running_false_for_unknown_job() {
    let (_pool, db) = setup_test_db().await;
    let running = job_repo::is_running(&db, "nonexistent_job").await.unwrap();
    assert!(!running);
}

#[tokio::test]
async fn job_last_run_returns_none_for_unknown_job() {
    let (_pool, db) = setup_test_db().await;
    let last = job_repo::last_run(&db, "nonexistent_job").await.unwrap();
    assert!(last.is_none());
}

#[tokio::test]
async fn job_list_runs_empty_for_unknown_job() {
    let (_pool, db) = setup_test_db().await;
    let (runs, total) = job_repo::list_runs(&db, "nonexistent_job", 10, 0)
        .await
        .unwrap();
    assert_eq!(total, 0);
    assert!(runs.is_empty());
}

#[tokio::test]
async fn job_cleanup_stale_does_nothing_when_no_running_jobs() {
    let (_pool, db) = setup_test_db().await;
    // Create a completed job
    let run_id = job_repo::create_run(&db, "completed_job", None)
        .await
        .unwrap();
    job_repo::finish_run(&db, run_id, "completed", "\"ok\"")
        .await
        .unwrap();

    // cleanup_stale should not affect it
    job_repo::cleanup_stale(&db).await.unwrap();

    let run = job_repo::last_run(&db, "completed_job")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.status, "completed");
}

// ===========================================================================
//  config_repo edge cases
// ===========================================================================

#[tokio::test]
async fn config_get_by_prefix_returns_empty_for_no_match() {
    let (_pool, db) = setup_test_db().await;
    let results = config_repo::get_by_prefix(&db, "zzz_nonexistent_")
        .await
        .unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn config_get_all_includes_migration_defaults() {
    let (_pool, db) = setup_test_db().await;
    let all = config_repo::get_all(&db).await.unwrap();
    let keys: Vec<String> = all.iter().map(|r| r.key.clone()).collect();
    // Migrations insert these defaults
    assert!(keys.contains(&"job_cadence:library_scan".to_string()));
    assert!(keys.contains(&"metadata_retry_hours".to_string()));
}
