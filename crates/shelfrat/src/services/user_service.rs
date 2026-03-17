use sea_orm::DatabaseConnection;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::AppError;
use crate::repositories::{audit_repo, user_repo};
use crate::repositories::user_repo::UserColumn;
use crate::services::auth_service;

pub async fn list_users(db: &DatabaseConnection) -> Result<Value, AppError> {
    let users = user_repo::list_all(db).await?;

    let users_json: Vec<Value> = users.into_iter().map(|u| user_to_json(&u)).collect();

    Ok(json!({ "users": users_json }))
}

pub async fn get_user(db: &DatabaseConnection, id: i64) -> Result<Value, AppError> {
    let user = user_repo::find_by_id(db, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(user_to_json(&user))
}

pub async fn create_invite(
    db: &DatabaseConnection,
    admin_id: i64,
    admin_username: &str,
) -> Result<Value, AppError> {
    let token = Uuid::new_v4().to_string();
    let pending_username = format!("pending_{}", &token[..8]);

    user_repo::create_invite(db, &pending_username, &token).await?;

    audit_repo::log_action(
        db,
        Some(admin_id),
        "invite_created",
        Some(&format!("admin {} created invite", admin_username)),
    )
    .await?;

    Ok(json!({
        "invite_token": token,
        "invite_url": format!("/invite/{token}"),
    }))
}

pub async fn register_with_invite(
    db: &DatabaseConnection,
    token: &str,
    username: &str,
    email: &str,
    password: &str,
) -> Result<Value, AppError> {
    let user = user_repo::find_by_invite_token(db, token)
        .await?
        .ok_or(AppError::NotFound)?;

    if !user.password_hash.is_empty() {
        return Err(AppError::Conflict("invite already used".into()));
    }

    auth_service::validate_password(password)?;
    let password_hash = auth_service::hash_password(password)?;
    user_repo::register_invite(db, user.id, username, email, &password_hash).await?;

    audit_repo::log_action(
        db,
        Some(user.id),
        "user_joined",
        Some(&format!("user {} joined via invite", username)),
    )
    .await?;

    Ok(json!({
        "message": "registration complete",
        "username": username,
    }))
}

#[allow(clippy::too_many_arguments)]
pub async fn update_user(
    db: &DatabaseConnection,
    target_id: i64,
    caller_id: i64,
    caller_role: &str,
    _caller_username: &str,
    display_name: Option<&str>,
    email: Option<&str>,
    kindle_email: Option<&str>,
    role: Option<&str>,
    current_password: Option<&str>,
    new_password: Option<&str>,
) -> Result<Value, AppError> {
    let user = user_repo::find_by_id(db, target_id)
        .await?
        .ok_or(AppError::NotFound)?;

    if let Some(dn) = display_name {
        user_repo::update_field(db, target_id, UserColumn::DisplayName, dn).await?;
    }
    if let Some(em) = email {
        user_repo::update_field(db, target_id, UserColumn::Email, em).await?;
    }
    if let Some(ke) = kindle_email {
        user_repo::update_field(db, target_id, UserColumn::KindleEmail, ke).await?;
    }

    if let Some(r) = role {
        if caller_role != "admin" {
            return Err(AppError::Forbidden);
        }
        if caller_id == target_id {
            return Err(AppError::BadRequest("cannot change your own role".into()));
        }
        if r != "admin" && r != "member" {
            return Err(AppError::BadRequest(
                "role must be 'admin' or 'member'".into(),
            ));
        }
        user_repo::update_role(db, target_id, r).await?;
    }

    if let Some(new_pw) = new_password {
        if caller_role != "admin" {
            let current = current_password.ok_or_else(|| {
                AppError::BadRequest("current_password required to change password".into())
            })?;
            verify_current_password(current, &user.password_hash)?;
        }
        auth_service::validate_password(new_pw)?;
        let new_hash = auth_service::hash_password(new_pw)?;
        user_repo::update_field(db, target_id, UserColumn::PasswordHash, &new_hash).await?;
    }

    audit_repo::log_action(
        db,
        Some(caller_id),
        "profile_updated",
        Some(&format!("user {} profile updated", user.username)),
    )
    .await?;

    let updated = user_repo::find_by_id(db, target_id)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(user_to_json(&updated))
}

pub async fn revoke_user(
    db: &DatabaseConnection,
    target_id: i64,
    admin_id: i64,
    admin_username: &str,
) -> Result<Value, AppError> {
    if admin_id == target_id {
        return Err(AppError::BadRequest("cannot revoke yourself".into()));
    }

    let user = user_repo::find_by_id(db, target_id)
        .await?
        .ok_or(AppError::NotFound)?;

    user_repo::delete(db, target_id).await?;

    audit_repo::log_action(
        db,
        Some(admin_id),
        "user_revoked",
        Some(&format!(
            "admin {} revoked user {}",
            admin_username, user.username
        )),
    )
    .await?;

    Ok(json!({
        "message": "user revoked",
        "username": user.username,
    }))
}

fn verify_current_password(password: &str, hash: &str) -> Result<(), AppError> {
    use argon2::{Argon2, PasswordHash, PasswordVerifier};

    let parsed = PasswordHash::new(hash)
        .map_err(|e| AppError::Internal(format!("invalid password hash: {e}")))?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| AppError::BadRequest("incorrect current password".into()))
}

fn user_to_json(u: &crate::entities::user::Model) -> Value {
    json!({
        "id": u.id,
        "username": u.username,
        "display_name": u.display_name,
        "email": u.email,
        "role": u.role,
        "kindle_email": u.kindle_email,
        "created_at": u.created_at,
    })
}
