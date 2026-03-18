use axum::extract::{Path, Query, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::AdminUser;
use crate::config;
use crate::error::AppError;
use crate::jobs::KNOWN_JOBS;
use crate::repositories::job_repo;
use crate::services::admin_service;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/jobs", get(list_jobs))
        .route("/admin/jobs/{name}/run", post(trigger_job))
        .route("/admin/jobs/{name}/runs", get(job_runs))
        .route("/admin/jobs/{name}/cadence", put(update_cadence))
}

async fn list_jobs(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let mut jobs = Vec::new();

    for &(name, description) in KNOWN_JOBS {
        let cadence_key = format!("job_cadence:{name}");
        let cadence_seconds: u64 = config::get(&state.db, &cadence_key)
            .await
            .and_then(|v| v.parse().ok())
            .unwrap_or(300);

        let running = job_repo::is_running(&state.db, name).await.unwrap_or(false);
        let last_run = job_repo::last_run(&state.db, name).await?;

        let last_run_json = last_run.map(|r| {
            json!({
                "id": r.id,
                "status": r.status,
                "started_at": r.started_at.to_string(),
                "finished_at": r.finished_at.map(|t| t.to_string()),
                "result": r.result,
                "triggered_by": r.triggered_by,
            })
        });

        jobs.push(json!({
            "name": name,
            "description": description,
            "cadence_seconds": cadence_seconds,
            "enabled": cadence_seconds > 0,
            "last_run": last_run_json,
            "running": running,
        }));
    }

    Ok(Json(json!({ "jobs": jobs })))
}

async fn trigger_job(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, AppError> {
    if !KNOWN_JOBS.iter().any(|&(n, _)| n == name) {
        return Err(AppError::BadRequest(format!("unknown job: {name}")));
    }

    let job_handle = state
        .job_handle
        .as_ref()
        .ok_or_else(|| AppError::Internal("job scheduler not running".into()))?;

    let triggered_by = format!("user:{}", admin.username);
    job_handle.trigger(&name, Some(&triggered_by));

    Ok(Json(json!({
        "message": format!("job {name} triggered"),
        "triggered_by": triggered_by,
    })))
}

#[derive(Debug, Deserialize)]
struct RunsQuery {
    limit: Option<u64>,
    offset: Option<u64>,
}

async fn job_runs(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(params): Query<RunsQuery>,
) -> Result<Json<Value>, AppError> {
    if !KNOWN_JOBS.iter().any(|&(n, _)| n == name) {
        return Err(AppError::BadRequest(format!("unknown job: {name}")));
    }

    let limit = params.limit.unwrap_or(25).min(200);
    let offset = params.offset.unwrap_or(0);

    let (runs, total) = job_repo::list_runs(&state.db, &name, limit, offset).await?;

    let entries: Vec<Value> = runs
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "job_name": r.job_name,
                "status": r.status,
                "started_at": r.started_at.to_string(),
                "finished_at": r.finished_at.map(|t| t.to_string()),
                "result": r.result,
                "triggered_by": r.triggered_by,
            })
        })
        .collect();

    Ok(Json(json!({
        "runs": entries,
        "total": total,
        "limit": limit,
        "offset": offset,
    })))
}

#[derive(Debug, Deserialize)]
struct CadenceBody {
    seconds: u64,
}

async fn update_cadence(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<CadenceBody>,
) -> Result<Json<Value>, AppError> {
    if !KNOWN_JOBS.iter().any(|&(n, _)| n == name) {
        return Err(AppError::BadRequest(format!("unknown job: {name}")));
    }
    let result =
        admin_service::update_job_cadence(&state.db, admin.id, &name, body.seconds).await?;
    Ok(Json(result))
}
