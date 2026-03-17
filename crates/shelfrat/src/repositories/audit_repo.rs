use sea_orm::*;

use crate::entities::audit_log;

/// Insert an audit log entry.
pub async fn log_action(
    db: &DatabaseConnection,
    user_id: Option<i64>,
    action: &str,
    detail: Option<&str>,
) -> Result<(), DbErr> {
    let model = audit_log::ActiveModel {
        user_id: Set(user_id),
        action: Set(action.to_string()),
        detail: Set(detail.map(|s| s.to_string())),
        ..Default::default()
    };
    audit_log::Entity::insert(model).exec(db).await?;
    Ok(())
}

/// Query result row with joined username.
#[derive(Debug, FromQueryResult, serde::Serialize)]
pub struct AuditLogRow {
    pub id: i64,
    pub user_id: Option<i64>,
    pub action: String,
    pub detail: Option<String>,
    pub ip_address: Option<String>,
    pub created_at: chrono::NaiveDateTime,
    pub username: Option<String>,
}

/// Query audit log with optional filters, pagination, and joined username.
pub async fn query_with_filters(
    db: &DatabaseConnection,
    action: Option<&str>,
    user_id: Option<i64>,
    limit: u64,
    offset: u64,
) -> Result<(Vec<AuditLogRow>, u64), DbErr> {
    let mut conditions = Vec::new();
    let mut values: Vec<Value> = Vec::new();

    if let Some(action) = action {
        conditions.push("al.action = ?");
        values.push(action.into());
    }
    if let Some(uid) = user_id {
        conditions.push("al.user_id = ?");
        values.push(uid.into());
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let query = format!(
        "SELECT al.id, al.user_id, al.action, al.detail, al.ip_address, al.created_at, u.username \
         FROM audit_log al LEFT JOIN users u ON u.id = al.user_id \
         {where_clause} ORDER BY al.created_at DESC LIMIT ? OFFSET ?"
    );

    let mut stmt_values = values.clone();
    stmt_values.push((limit as i64).into());
    stmt_values.push((offset as i64).into());

    let rows = AuditLogRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        &query,
        stmt_values,
    ))
    .all(db)
    .await?;

    let count_query = format!("SELECT COUNT(*) as count FROM audit_log al {where_clause}");

    #[derive(FromQueryResult)]
    struct CountResult {
        count: i64,
    }

    let total = CountResult::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        &count_query,
        values,
    ))
    .one(db)
    .await?
    .map(|r| r.count as u64)
    .unwrap_or(0);

    Ok((rows, total))
}
