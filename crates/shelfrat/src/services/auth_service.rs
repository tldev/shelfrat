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

#[cfg(test)]
mod tests {
    use super::*;

    // ── validate_password ──────────────────────────────────────────

    #[test]
    fn validate_password_too_short() {
        let result = validate_password("1234567");
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::BadRequest(msg) => assert!(msg.contains("at least 8")),
            other => panic!("expected BadRequest, got: {other:?}"),
        }
    }

    #[test]
    fn validate_password_too_long() {
        let long = "a".repeat(129);
        let result = validate_password(&long);
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::BadRequest(msg) => assert!(msg.contains("at most 128")),
            other => panic!("expected BadRequest, got: {other:?}"),
        }
    }

    #[test]
    fn validate_password_exactly_8() {
        assert!(validate_password("12345678").is_ok());
    }

    #[test]
    fn validate_password_exactly_128() {
        let pw = "a".repeat(128);
        assert!(validate_password(&pw).is_ok());
    }

    #[test]
    fn validate_password_valid_length() {
        assert!(validate_password("a_perfectly_fine_password").is_ok());
    }

    #[test]
    fn validate_password_empty() {
        assert!(validate_password("").is_err());
    }

    // ── hash_password ──────────────────────────────────────────────

    #[test]
    fn hash_password_produces_argon2_hash() {
        let hash = hash_password("testpassword123").unwrap();
        assert!(hash.starts_with("$argon2"), "expected argon2 hash prefix, got: {hash}");
    }

    #[test]
    fn hash_password_different_passwords_get_different_hashes() {
        let h1 = hash_password("password_one").unwrap();
        let h2 = hash_password("password_two").unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_password_same_password_different_salts() {
        let h1 = hash_password("same_password").unwrap();
        let h2 = hash_password("same_password").unwrap();
        // Different salts should produce different hashes even for the same input.
        assert_ne!(h1, h2);
    }

    // ── verify_password ────────────────────────────────────────────

    #[test]
    fn verify_password_correct_succeeds() {
        let hash = hash_password("correct_horse").unwrap();
        assert!(verify_password("correct_horse", &hash).is_ok());
    }

    #[test]
    fn verify_password_wrong_fails() {
        let hash = hash_password("correct_horse").unwrap();
        let result = verify_password("wrong_horse", &hash);
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Unauthorized => {}
            other => panic!("expected Unauthorized, got: {other:?}"),
        }
    }

    #[test]
    fn verify_password_empty_hash_fails() {
        let result = verify_password("anything", "");
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Unauthorized => {}
            other => panic!("expected Unauthorized, got: {other:?}"),
        }
    }

    #[test]
    fn verify_password_garbage_hash_fails() {
        let result = verify_password("anything", "not-a-valid-hash");
        assert!(result.is_err());
    }

    // ── create_jwt ─────────────────────────────────────────────────

    #[test]
    fn create_jwt_produces_valid_token() {
        use jsonwebtoken::{decode, DecodingKey, Validation};

        let user = crate::entities::user::Model {
            id: 42,
            username: "testuser".to_string(),
            display_name: None,
            email: "test@example.com".to_string(),
            password_hash: String::new(),
            role: "admin".to_string(),
            kindle_email: None,
            invite_token: None,
            created_at: chrono::Utc::now().naive_utc(),
            oidc_subject: None,
            oidc_issuer: None,
        };

        let secret = "super_secret_key";
        let token = create_jwt(&user, secret).unwrap();

        // Token should have three dot-separated parts (header.payload.signature).
        assert_eq!(token.matches('.').count(), 2);

        // Decode and verify claims.
        let decoded = decode::<crate::auth::Claims>(
            &token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &Validation::default(),
        )
        .unwrap();

        assert_eq!(decoded.claims.sub, 42);
        assert_eq!(decoded.claims.username, "testuser");
        assert_eq!(decoded.claims.role, "admin");
        assert!(decoded.claims.exp > decoded.claims.iat);
        assert!(decoded.claims.iat > 0);
    }

    #[test]
    fn create_jwt_wrong_secret_fails_decode() {
        use jsonwebtoken::{decode, DecodingKey, Validation};

        let user = crate::entities::user::Model {
            id: 1,
            username: "u".to_string(),
            display_name: None,
            email: "u@e.com".to_string(),
            password_hash: String::new(),
            role: "member".to_string(),
            kindle_email: None,
            invite_token: None,
            created_at: chrono::Utc::now().naive_utc(),
            oidc_subject: None,
            oidc_issuer: None,
        };

        let token = create_jwt(&user, "secret_a").unwrap();
        let result = decode::<crate::auth::Claims>(
            &token,
            &DecodingKey::from_secret(b"secret_b"),
            &Validation::default(),
        );
        assert!(result.is_err());
    }
}
