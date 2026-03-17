use sea_orm::*;

use crate::entities::job_run;

pub async fn create_run(
    db: &DatabaseConnection,
    job_name: &str,
    triggered_by: Option<&str>,
) -> Result<i64, DbErr> {
    let model = job_run::ActiveModel {
        job_name: Set(job_name.to_string()),
        status: Set("running".to_string()),
        triggered_by: Set(triggered_by.map(|s| s.to_string())),
        ..Default::default()
    };
    let res = job_run::Entity::insert(model).exec(db).await?;
    Ok(res.last_insert_id)
}

pub async fn finish_run(
    db: &DatabaseConnection,
    run_id: i64,
    status: &str,
    result: &str,
) -> Result<(), DbErr> {
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "UPDATE job_runs SET status = ?, finished_at = datetime('now'), result = ? WHERE id = ?",
        [status.into(), result.into(), run_id.into()],
    ))
    .await?;
    Ok(())
}

pub async fn is_running(db: &DatabaseConnection, job_name: &str) -> Result<bool, DbErr> {
    let count = job_run::Entity::find()
        .filter(job_run::Column::JobName.eq(job_name))
        .filter(job_run::Column::Status.eq("running"))
        .count(db)
        .await?;
    Ok(count > 0)
}

pub async fn last_run(
    db: &DatabaseConnection,
    job_name: &str,
) -> Result<Option<job_run::Model>, DbErr> {
    job_run::Entity::find()
        .filter(job_run::Column::JobName.eq(job_name))
        .order_by_desc(job_run::Column::StartedAt)
        .one(db)
        .await
}

pub async fn list_runs(
    db: &DatabaseConnection,
    job_name: &str,
    limit: u64,
    offset: u64,
) -> Result<(Vec<job_run::Model>, u64), DbErr> {
    let total = job_run::Entity::find()
        .filter(job_run::Column::JobName.eq(job_name))
        .count(db)
        .await?;

    let runs = job_run::Entity::find()
        .filter(job_run::Column::JobName.eq(job_name))
        .order_by_desc(job_run::Column::StartedAt)
        .offset(offset)
        .limit(limit)
        .all(db)
        .await?;

    Ok((runs, total))
}

/// Last finished time for a job (completed or failed).
pub async fn last_finished_at(
    db: &DatabaseConnection,
    job_name: &str,
) -> Result<Option<chrono::NaiveDateTime>, DbErr> {
    #[derive(FromQueryResult)]
    struct FinishedAt {
        finished_at: chrono::NaiveDateTime,
    }

    Ok(FinishedAt::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT finished_at FROM job_runs WHERE job_name = ? AND status IN ('completed', 'failed') ORDER BY finished_at DESC LIMIT 1",
        [job_name.into()],
    ))
    .one(db)
    .await?
    .map(|r| r.finished_at))
}

/// Clean up stale running jobs from previous crashes.
pub async fn cleanup_stale(db: &DatabaseConnection) -> Result<(), DbErr> {
    db.execute(Statement::from_string(
        DatabaseBackend::Sqlite,
        "UPDATE job_runs SET status = 'failed', finished_at = datetime('now'), result = '\"interrupted by restart\"' WHERE status = 'running'",
    ))
    .await?;
    Ok(())
}
