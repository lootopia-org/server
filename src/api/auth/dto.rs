use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::utils::types::nullable;

#[derive(Debug, Deserialize)]
pub struct RegisterReq {
    pub email: String,
    pub username: String,
    pub avatar: String,
    pub bio: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct EmailReq {
    pub email: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LoginReq {
    #[serde(default, deserialize_with = "nullable")]
    pub username: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub email: Option<String>,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct ForgotPasswordReq {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordReq {
    pub token: String,
    pub new_password: String,
}

#[derive(Debug, Deserialize)]
pub struct MfaTotpReq {
    pub code: String,
}

#[derive(Debug, Deserialize)]
pub struct CodeReq {
    pub code: String,
}

#[derive(Debug, Deserialize)]
pub struct WebauthnCompleteReq {
    pub handle: String,
    pub credential: Value,
}

#[derive(Debug, Deserialize)]
pub struct VerifyEmailParams {
    pub token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TokenResp {
    pub token: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResp {
    pub token: String,
    pub mfa_required: bool,
    pub mfa_methods: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeResp {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub email_verified: bool,
    pub totp_enabled: bool,
    pub passkeys: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TotpEnrollResp {
    pub secret: String,
    pub otpauth_uri: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialResp {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
}
