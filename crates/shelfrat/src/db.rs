use sea_orm::DatabaseConnection;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

pub async fn init(database_url: &str) -> anyhow::Result<(SqlitePool, DatabaseConnection)> {
    let opts = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(30))
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await?;

    sqlx::migrate!("../../migrations").run(&pool).await?;

    tracing::info!("migrations applied");

    let db = sea_orm::SqlxSqliteConnector::from_sqlx_sqlite_pool(pool.clone());

    Ok((pool, db))
}
