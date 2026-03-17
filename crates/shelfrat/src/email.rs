use std::path::Path;

use lettre::message::{header::ContentType, Attachment, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use sqlx::SqlitePool;

/// SMTP configuration loaded from app_config.
#[derive(Debug, Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub from: String,
    pub encryption: String, // "tls", "starttls", or "none"
}

async fn get_config(db: &SqlitePool, key: &str) -> Result<String, EmailError> {
    sqlx::query_scalar::<_, String>("SELECT value FROM app_config WHERE key = ?")
        .bind(key)
        .fetch_optional(db)
        .await
        .map_err(|e| EmailError::Config(format!("db error: {e}")))?
        .ok_or_else(|| EmailError::Config(format!("missing config: {key}")))
}

impl SmtpConfig {
    /// Load SMTP config from the app_config table.
    pub async fn from_db(db: &SqlitePool) -> Result<Self, EmailError> {
        let host = get_config(db, "smtp_host").await?;
        let port_str = get_config(db, "smtp_port").await.unwrap_or_else(|_| "587".to_string());
        let port = port_str
            .parse::<u16>()
            .map_err(|_| EmailError::Config("invalid smtp_port".to_string()))?;
        let user = get_config(db, "smtp_user").await?;
        let password = get_config(db, "smtp_password").await?;
        let from = get_config(db, "smtp_from").await?;
        let encryption = get_config(db, "smtp_encryption").await.unwrap_or_else(|_| "starttls".to_string());

        if host.is_empty() {
            return Err(EmailError::Config("smtp_host is empty".into()));
        }
        if from.is_empty() {
            return Err(EmailError::Config("smtp_from is empty".into()));
        }

        Ok(SmtpConfig {
            host,
            port,
            user,
            password,
            from,
            encryption,
        })
    }
}

/// Send an ebook file as an email attachment (for Send-to-Kindle or similar).
pub async fn send_book_email(
    config: &SmtpConfig,
    to_email: &str,
    filename: &str,
    file_path: &Path,
    content_type_str: &str,
) -> Result<(), EmailError> {
    let file_data = tokio::fs::read(file_path)
        .await
        .map_err(|e| EmailError::Io(format!("cannot read file: {e}")))?;

    let content_type = ContentType::parse(content_type_str)
        .unwrap_or(ContentType::parse("application/octet-stream").expect("hardcoded MIME type"));

    let attachment = Attachment::new(filename.to_string())
        .body(file_data, content_type);

    let email = Message::builder()
        .from(
            config
                .from
                .parse()
                .map_err(|e| EmailError::Config(format!("invalid from address: {e}")))?,
        )
        .to(to_email
            .parse()
            .map_err(|e| EmailError::Config(format!("invalid to address: {e}")))?)
        .subject(format!("Book: {filename}"))
        .multipart(
            MultiPart::mixed()
                .singlepart(
                    SinglePart::builder()
                        .content_type(ContentType::TEXT_PLAIN)
                        .body("Sent from shelfrat".to_string()),
                )
                .singlepart(attachment),
        )
        .map_err(|e| EmailError::Send(format!("failed to build email: {e}")))?;

    let transport = build_transport(config)?;

    transport
        .send(email)
        .await
        .map_err(|e| EmailError::Send(format!("SMTP send failed: {e}")))?;

    Ok(())
}

fn build_transport(
    config: &SmtpConfig,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, EmailError> {
    let creds = Credentials::new(config.user.clone(), config.password.clone());

    let transport = match config.encryption.as_str() {
        "tls" => AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
            .map_err(|e| EmailError::Config(format!("TLS relay error: {e}")))?
            .port(config.port)
            .credentials(creds)
            .build(),
        "none" => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host)
            .port(config.port)
            .credentials(creds)
            .build(),
        _ => {
            // Default: STARTTLS
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
                .map_err(|e| EmailError::Config(format!("STARTTLS relay error: {e}")))?
                .port(config.port)
                .credentials(creds)
                .build()
        }
    };

    Ok(transport)
}

#[derive(Debug, thiserror::Error)]
pub enum EmailError {
    #[error("email config error: {0}")]
    Config(String),

    #[error("I/O error: {0}")]
    Io(String),

    #[error("email send error: {0}")]
    Send(String),
}

impl From<EmailError> for crate::error::AppError {
    fn from(e: EmailError) -> Self {
        match e {
            EmailError::Config(msg) => {
                tracing::warn!("email config error: {msg}");
                crate::error::AppError::BadRequest(
                    "email is not configured — ask an admin to set up SMTP".into(),
                )
            }
            EmailError::Io(msg) => crate::error::AppError::Internal(msg),
            EmailError::Send(msg) => crate::error::AppError::Internal(msg),
        }
    }
}
