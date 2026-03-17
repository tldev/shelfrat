use chrono::Utc;
use jsonwebtoken::{encode, EncodingKey, Header};
use sea_orm::DatabaseConnection;

use crate::auth::Claims;
use crate::error::AppError;
use crate::repositories::{audit_repo, config_repo, user_repo};

pub async fn login(
    db: &DatabaseConnection,
    username: &str,
    password: &str,
) -> Result<(String, crate::entities::user::Model), AppError> {
    let user = user_repo::find_by_username(db, username)
        .await?
        .ok_or(AppError::Unauthorized)?;

    verify_password(password, &user.password_hash)?;

    let jwt_secret = config_repo::get_or_create_jwt_secret(db)
        .await
        .map_err(|_| AppError::Internal("failed to get jwt secret".into()))?;

    let token = create_jwt(&user, &jwt_secret)?;

    audit_repo::log_action(
        db,
        Some(user.id),
        "login",
        Some(&format!("user {} logged in", user.username)),
    )
    .await?;

    Ok((token, user))
}

pub fn create_jwt(
    user: &crate::entities::user::Model,
    jwt_secret: &str,
) -> Result<String, AppError> {
    let now = Utc::now();
    let claims = Claims {
        sub: user.id,
        username: user.username.clone(),
        role: user.role.clone(),
        iat: now.timestamp(),
        exp: (now + chrono::Duration::hours(24)).timestamp(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("token creation failed: {e}")))
}

pub fn verify_password(password: &str, hash: &str) -> Result<(), AppError> {
    use argon2::{Argon2, PasswordHash, PasswordVerifier};

    let parsed = PasswordHash::new(hash).map_err(|_| AppError::Unauthorized)?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| AppError::Unauthorized)?;
    Ok(())
}

/// Validate password meets minimum security requirements (NIST 800-63B).
pub fn validate_password(password: &str) -> Result<(), AppError> {
    if password.len() < 8 {
        return Err(AppError::BadRequest(
            "password must be at least 8 characters".into(),
        ));
    }
    if password.len() > 128 {
        return Err(AppError::BadRequest(
            "password must be at most 128 characters".into(),
        ));
    }
    Ok(())
}

pub fn hash_password(password: &str) -> Result<String, AppError> {
    use argon2::{
        password_hash::{rand_core::OsRng, SaltString},
        Argon2, PasswordHasher,
    };

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| AppError::Internal(format!("password hashing failed: {e}")))?;
    Ok(hash.to_string())
}
