use sea_orm::*;

use crate::entities::user;

pub async fn find_by_id(db: &DatabaseConnection, id: i64) -> Result<Option<user::Model>, DbErr> {
    user::Entity::find_by_id(id).one(db).await
}

pub async fn find_by_username(
    db: &DatabaseConnection,
    username: &str,
) -> Result<Option<user::Model>, DbErr> {
    user::Entity::find()
        .filter(user::Column::Username.eq(username))
        .one(db)
        .await
}

pub async fn find_by_invite_token(
    db: &DatabaseConnection,
    token: &str,
) -> Result<Option<user::Model>, DbErr> {
    user::Entity::find()
        .filter(user::Column::InviteToken.eq(token))
        .one(db)
        .await
}

pub async fn find_by_oidc(
    db: &DatabaseConnection,
    subject: &str,
    issuer: &str,
) -> Result<Option<user::Model>, DbErr> {
    user::Entity::find()
        .filter(user::Column::OidcSubject.eq(subject))
        .filter(user::Column::OidcIssuer.eq(issuer))
        .one(db)
        .await
}

pub async fn list_all(db: &DatabaseConnection) -> Result<Vec<user::Model>, DbErr> {
    user::Entity::find()
        .order_by_desc(user::Column::CreatedAt)
        .all(db)
        .await
}

pub async fn count_admins(db: &DatabaseConnection) -> Result<u64, DbErr> {
    user::Entity::find()
        .filter(user::Column::Role.eq("admin"))
        .count(db)
        .await
}

pub async fn count_by_username(db: &DatabaseConnection, username: &str) -> Result<u64, DbErr> {
    user::Entity::find()
        .filter(user::Column::Username.eq(username))
        .count(db)
        .await
}

pub async fn create_admin(
    db: &DatabaseConnection,
    username: &str,
    email: &str,
    password_hash: &str,
) -> Result<user::Model, DbErr> {
    let model = user::ActiveModel {
        username: Set(username.to_string()),
        email: Set(email.to_string()),
        password_hash: Set(password_hash.to_string()),
        role: Set("admin".to_string()),
        ..Default::default()
    };
    let res = user::Entity::insert(model).exec(db).await?;
    find_by_id(db, res.last_insert_id).await.map(|o| o.unwrap())
}

pub async fn create_invite(
    db: &DatabaseConnection,
    pending_username: &str,
    token: &str,
) -> Result<(), DbErr> {
    let model = user::ActiveModel {
        username: Set(pending_username.to_string()),
        email: Set(String::new()),
        password_hash: Set(String::new()),
        role: Set("member".to_string()),
        invite_token: Set(Some(token.to_string())),
        ..Default::default()
    };
    user::Entity::insert(model).exec(db).await?;
    Ok(())
}

pub async fn register_invite(
    db: &DatabaseConnection,
    user_id: i64,
    username: &str,
    email: &str,
    password_hash: &str,
) -> Result<(), DbErr> {
    let model = user::ActiveModel {
        id: Set(user_id),
        username: Set(username.to_string()),
        email: Set(email.to_string()),
        password_hash: Set(password_hash.to_string()),
        invite_token: Set(None),
        ..Default::default()
    };
    model.update(db).await?;
    Ok(())
}

/// Allowed columns for user field updates. Prevents SQL injection by
/// ensuring only known column names are interpolated into queries.
pub enum UserColumn {
    DisplayName,
    Email,
    KindleEmail,
    PasswordHash,
    Role,
}

impl UserColumn {
    fn as_str(&self) -> &'static str {
        match self {
            Self::DisplayName => "display_name",
            Self::Email => "email",
            Self::KindleEmail => "kindle_email",
            Self::PasswordHash => "password_hash",
            Self::Role => "role",
        }
    }
}

pub async fn update_field(
    db: &DatabaseConnection,
    id: i64,
    field: UserColumn,
    value: &str,
) -> Result<(), DbErr> {
    let column = field.as_str();
    let sql = format!("UPDATE users SET {column} = ? WHERE id = ?");
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        &sql,
        [value.into(), id.into()],
    ))
    .await?;
    Ok(())
}

pub async fn update_role(db: &DatabaseConnection, id: i64, role: &str) -> Result<(), DbErr> {
    update_field(db, id, UserColumn::Role, role).await
}

pub async fn delete(db: &DatabaseConnection, id: i64) -> Result<(), DbErr> {
    user::Entity::delete_by_id(id).exec(db).await?;
    Ok(())
}

pub async fn create_oidc_user(
    db: &DatabaseConnection,
    username: &str,
    display_name: Option<&str>,
    email: &str,
    role: &str,
    oidc_subject: &str,
    oidc_issuer: &str,
) -> Result<(), DbErr> {
    let model = user::ActiveModel {
        username: Set(username.to_string()),
        display_name: Set(display_name.map(|s| s.to_string())),
        email: Set(email.to_string()),
        password_hash: Set(String::new()),
        role: Set(role.to_string()),
        oidc_subject: Set(Some(oidc_subject.to_string())),
        oidc_issuer: Set(Some(oidc_issuer.to_string())),
        ..Default::default()
    };
    user::Entity::insert(model).exec(db).await?;
    Ok(())
}

/// Ensure a username is unique, appending a number if needed.
pub async fn ensure_unique_username(
    db: &DatabaseConnection,
    base: &str,
) -> Result<String, DbErr> {
    if count_by_username(db, base).await? == 0 {
        return Ok(base.to_string());
    }

    for i in 1..100 {
        let candidate = format!("{base}{i}");
        if count_by_username(db, &candidate).await? == 0 {
            return Ok(candidate);
        }
    }

    Ok(format!(
        "{base}_{}",
        &uuid::Uuid::new_v4().to_string()[..8]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── UserColumn::as_str ─────────────────────────────────────────

    #[test]
    fn user_column_display_name() {
        assert_eq!(UserColumn::DisplayName.as_str(), "display_name");
    }

    #[test]
    fn user_column_email() {
        assert_eq!(UserColumn::Email.as_str(), "email");
    }

    #[test]
    fn user_column_kindle_email() {
        assert_eq!(UserColumn::KindleEmail.as_str(), "kindle_email");
    }

    #[test]
    fn user_column_password_hash() {
        assert_eq!(UserColumn::PasswordHash.as_str(), "password_hash");
    }

    #[test]
    fn user_column_role() {
        assert_eq!(UserColumn::Role.as_str(), "role");
    }
}
