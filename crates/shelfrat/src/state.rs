use std::path::PathBuf;

use sea_orm::DatabaseConnection;
use sqlx::SqlitePool;

use crate::jobs::JobHandle;
use crate::metaqueue::MetaQueue;
use crate::repositories::config_repo;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub pool: SqlitePool,
    pub library_path: Option<PathBuf>,
    pub meta_queue: Option<MetaQueue>,
    pub job_handle: Option<JobHandle>,
}

impl AppState {
    pub fn new(
        db: DatabaseConnection,
        pool: SqlitePool,
        library_path: Option<PathBuf>,
        meta_queue: Option<MetaQueue>,
        job_handle: Option<JobHandle>,
    ) -> Self {
        Self {
            db,
            pool,
            library_path,
            meta_queue,
            job_handle,
        }
    }

    /// Resolve the library path: env var first, then app_config, then the field set at startup.
    pub async fn resolve_library_path(&self) -> Option<PathBuf> {
        if let Ok(env_path) = std::env::var("LIBRARY_PATH") {
            if !env_path.is_empty() {
                return Some(PathBuf::from(env_path));
            }
        }

        if let Ok(Some(db_path)) = config_repo::get(&self.db, "library_path").await {
            if !db_path.is_empty() {
                return Some(PathBuf::from(db_path));
            }
        }

        self.library_path.clone()
    }
}
