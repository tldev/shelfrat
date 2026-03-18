use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

use shelfrat::{api, db, jobs, metaqueue, state};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:shelfrat.db".into());
    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let library_path = std::env::var("LIBRARY_PATH")
        .ok()
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from);

    let (pool, db) = db::init(&database_url).await?;

    tracing::info!("database initialized");

    if let Some(ref path) = library_path {
        tracing::info!("library path: {}", path.display());
    }

    // Derive covers directory from DATABASE_URL (same dir as DB file) or /data
    let covers_dir = {
        let db_path = database_url
            .strip_prefix("sqlite:")
            .unwrap_or(&database_url);
        let parent = std::path::Path::new(db_path)
            .parent()
            .unwrap_or(std::path::Path::new("."));
        parent.join("covers")
    };
    tracing::info!("covers directory: {}", covers_dir.display());

    // Start background metadata queue if library path is configured
    let meta_queue = library_path
        .as_ref()
        .map(|_| metaqueue::start(pool.clone(), db.clone(), covers_dir.clone()));

    // Start the job scheduler
    let job_handle = jobs::start(
        pool.clone(),
        db.clone(),
        library_path.clone(),
        meta_queue.clone(),
        covers_dir.clone(),
    );

    let app_state = state::AppState::new(db, pool, library_path, meta_queue, Some(job_handle));
    let app = api::router(app_state);

    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    tracing::info!("listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
