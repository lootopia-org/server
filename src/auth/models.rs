use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub email_verified: bool,
    pub password_salt: Vec<u8>,
    pub password_hash: Vec<u8>,
    pub user_handle: Vec<u8>,
    pub totp_secret: Option<Vec<u8>>,
    pub totp_enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct EmailToken {
    pub id: i64,
    pub user_id: i64,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct Session {
    pub id: i64,
    pub user_id: i64,
    pub token: String,
    pub mfa_pending: bool,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct Credential {
    pub id: i64,
    pub user_id: i64,
    pub credential_id: Vec<u8>,
    pub passkey: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct AuthCeremony {
    pub id: i64,
    pub handle: String,
    pub purpose: String,
    pub state: Value,
    pub user_id: Option<i64>,
    pub expires_at: DateTime<Utc>,
}
