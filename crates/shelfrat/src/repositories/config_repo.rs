use sea_orm::*;

use crate::entities::app_config;

/// Get a config value by key.
pub async fn get(db: &DatabaseConnection, key: &str) -> Result<Option<String>, DbErr> {
    Ok(app_config::Entity::find_by_id(key)
        .one(db)
        .await?
        .map(|m| m.value))
}

/// Set a config value (upsert).
pub async fn set(db: &DatabaseConnection, key: &str, value: &str) -> Result<(), DbErr> {
    let model = app_config::ActiveModel {
        key: Set(key.to_string()),
        value: Set(value.to_string()),
    };
    app_config::Entity::insert(model)
        .on_conflict(
            sea_query::OnConflict::column(app_config::Column::Key)
                .update_column(app_config::Column::Value)
                .to_owned(),
        )
        .exec(db)
        .await?;
    Ok(())
}

/// Get all config rows.
pub async fn get_all(db: &DatabaseConnection) -> Result<Vec<app_config::Model>, DbErr> {
    app_config::Entity::find().all(db).await
}

/// Get all config rows matching a key prefix.
pub async fn get_by_prefix(
    db: &DatabaseConnection,
    prefix: &str,
) -> Result<Vec<app_config::Model>, DbErr> {
    app_config::Entity::find()
        .filter(app_config::Column::Key.starts_with(prefix))
        .all(db)
        .await
}

/// Get or create the JWT secret. Handles race conditions with ON CONFLICT DO NOTHING.
pub async fn get_or_create_jwt_secret(db: &DatabaseConnection) -> Result<String, DbErr> {
    if let Some(existing) = get(db, "jwt_secret").await? {
        return Ok(existing);
    }

    let secret = uuid::Uuid::new_v4().to_string();
    let model = app_config::ActiveModel {
        key: Set("jwt_secret".to_string()),
        value: Set(secret),
    };
    app_config::Entity::insert(model)
        .on_conflict(
            sea_query::OnConflict::column(app_config::Column::Key)
                .do_nothing()
                .to_owned(),
        )
        .exec_without_returning(db)
        .await?;

    // Re-fetch in case another request won the race
    get(db, "jwt_secret")
        .await?
        .ok_or_else(|| DbErr::Custom("jwt_secret not found after insert".into()))
}
