use std::path::PathBuf;
use std::time::{Duration, Instant};

use sea_orm::DatabaseConnection;
use sqlx::SqlitePool;
use tokio::sync::mpsc;

use crate::config;
use crate::metaqueue::MetaQueue;
use crate::repositories::job_repo;
use crate::services::scan_service;

/// Known job names.
pub const KNOWN_JOBS: &[(&str, &str)] = &[(
    "library_scan",
    "Scan library directory for new, modified, and removed ebooks",
)];

/// Handle used to trigger jobs from API handlers.
#[derive(Clone)]
pub struct JobHandle {
    trigger_tx: mpsc::UnboundedSender<(String, Option<String>)>,
}

impl JobHandle {
    /// Trigger a job by name, optionally recording who triggered it.
    pub fn trigger(&self, job_name: &str, triggered_by: Option<&str>) {
        let _ = self
            .trigger_tx
            .send((job_name.to_string(), triggered_by.map(|s| s.to_string())));
    }
}

/// Start the job scheduler. Returns a handle for triggering ad-hoc runs.
pub fn start(
    pool: SqlitePool,
    db: DatabaseConnection,
    library_path: Option<PathBuf>,
    meta_queue: Option<MetaQueue>,
    covers_dir: PathBuf,
) -> JobHandle {
    let (trigger_tx, trigger_rx) = mpsc::unbounded_channel::<(String, Option<String>)>();

    tokio::spawn(async move {
        scheduler_loop(trigger_rx, pool, db, library_path, meta_queue, covers_dir).await;
    });

    tracing::info!("job scheduler started");

    JobHandle { trigger_tx }
}

/// Main scheduler loop: ticks every 5 seconds, checks for due jobs, handles triggers.
async fn scheduler_loop(
    mut trigger_rx: mpsc::UnboundedReceiver<(String, Option<String>)>,
    pool: SqlitePool,
    db: DatabaseConnection,
    library_path: Option<PathBuf>,
    meta_queue: Option<MetaQueue>,
    covers_dir: PathBuf,
) {
    // Clean up stale "running" rows from previous crashes
    if let Err(e) = job_repo::cleanup_stale(&db).await {
        tracing::warn!("failed to clean up stale job runs: {e}");
    }

    let mut cadence_cache: Option<(Instant, std::collections::HashMap<String, u64>)> = None;

    let mut interval = tokio::time::interval(Duration::from_secs(5));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Check cadences and spawn due jobs
                let cadences = get_cadences(&db, &mut cadence_cache).await;

                for &(job_name, _) in KNOWN_JOBS {
                    let cadence_secs = cadences.get(job_name).copied().unwrap_or(300);
                    if cadence_secs == 0 {
                        continue; // disabled
                    }

                    if is_job_due(&db, job_name, cadence_secs).await {
                        spawn_job(
                            job_name,
                            Some("schedule"),
                            &pool,
                            &db,
                            &library_path,
                            &meta_queue,
                            &covers_dir,
                        )
                        .await;
                    }
                }
            }
            Some((job_name, triggered_by)) = trigger_rx.recv() => {
                let triggered_by_str = triggered_by.as_deref().unwrap_or("api");
                spawn_job(
                    &job_name,
                    Some(triggered_by_str),
                    &pool,
                    &db,
                    &library_path,
                    &meta_queue,
                    &covers_dir,
                )
                .await;
            }
        }
    }
}

/// Read cadences via config module (env > DB), cached for 30 seconds.
async fn get_cadences(
    db: &DatabaseConnection,
    cache: &mut Option<(Instant, std::collections::HashMap<String, u64>)>,
) -> std::collections::HashMap<String, u64> {
    if let Some((fetched_at, ref map)) = cache {
        if fetched_at.elapsed() < Duration::from_secs(30) {
            return map.clone();
        }
    }

    let mut map = std::collections::HashMap::new();
    for &(job_name, _) in KNOWN_JOBS {
        let key = format!("job_cadence:{job_name}");
        if let Some(val) = config::get(db, &key).await {
            if let Ok(secs) = val.parse::<u64>() {
                map.insert(job_name.to_string(), secs);
            }
        }
    }

    *cache = Some((Instant::now(), map.clone()));
    map
}

/// Check if a job is due to run based on its last completion time and cadence.
async fn is_job_due(db: &DatabaseConnection, job_name: &str, cadence_secs: u64) -> bool {
    // Don't schedule if already running
    if job_repo::is_running(db, job_name).await.unwrap_or(false) {
        return false;
    }

    let last_finished = job_repo::last_finished_at(db, job_name)
        .await
        .unwrap_or(None);

    match last_finished {
        None => true, // Never run before
        Some(finished) => {
            let now = chrono::Utc::now().naive_utc();
            let elapsed = (now - finished).num_seconds();
            elapsed >= cadence_secs as i64
        }
    }
}

/// Spawn a job by name.
async fn spawn_job(
    job_name: &str,
    triggered_by: Option<&str>,
    pool: &SqlitePool,
    db: &DatabaseConnection,
    library_path: &Option<PathBuf>,
    meta_queue: &Option<MetaQueue>,
    covers_dir: &std::path::Path,
) {
    // Don't start if already running
    if job_repo::is_running(db, job_name).await.unwrap_or(false) {
        tracing::info!("job {job_name} already running, skipping");
        return;
    }

    // Insert a "running" row
    let run_id = match job_repo::create_run(db, job_name, triggered_by).await {
        Ok(id) => id,
        Err(e) => {
            tracing::warn!("failed to create job_run row for {job_name}: {e}");
            return;
        }
    };

    tracing::info!("starting job {job_name} (run_id={run_id}, triggered_by={triggered_by:?})");

    match job_name {
        "library_scan" => {
            let pool = pool.clone();
            let db = db.clone();
            let library_path = library_path.clone();
            let meta_queue = meta_queue.clone();
            let _covers_dir = covers_dir.to_path_buf();
            tokio::spawn(async move {
                let result =
                    scan_service::run_library_scan_job(&db, &pool, &library_path, &meta_queue)
                        .await;
                finish_job_run(&db, run_id, result).await;
            });
        }
        _ => {
            tracing::warn!("unknown job: {job_name}");
            if let Err(e) = job_repo::finish_run(
                db,
                run_id,
                "failed",
                &format!("\"unknown job: {job_name}\""),
            )
            .await
            {
                tracing::warn!("failed to update job_run {run_id}: {e}");
            }
        }
    }
}

/// Mark a job run as completed or failed.
async fn finish_job_run(db: &DatabaseConnection, run_id: i64, result: Result<String, String>) {
    let (status, result_text) = match result {
        Ok(r) => ("completed", r),
        Err(e) => ("failed", e),
    };

    if let Err(e) = job_repo::finish_run(db, run_id, status, &result_text).await {
        tracing::warn!("failed to update job_run {run_id}: {e}");
    }

    tracing::info!("job run {run_id} finished: {status}");
}
