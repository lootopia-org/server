use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::auth::crypto::password::{verify_password, StoredPassword};
use crate::auth::crypto::token::random_token;
use crate::auth::crypto::{password, totp};
use crate::auth::error::{ApiError, ApiResult};
use crate::auth::models::User;
use crate::auth::session::{create_session, AuthedUser};
use crate::auth::state::AppState;
use crate::auth::{email, webauthn};

#[derive(Debug, Deserialize)]
struct RegisterReq {
    email: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct EmailReq {
    email: String,
}

#[derive(Debug, Deserialize)]
struct LoginReq {
    email: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct MfaTotpReq {
    token: String,
    code: String,
}

#[derive(Debug, Deserialize)]
struct CodeReq {
    code: String,
}

#[derive(Debug, Deserialize)]
struct WebauthnCompleteReq {
    handle: String,
    credential: Value,
}

#[derive(Debug, Deserialize)]
struct VerifyEmailParams {
    token: Option<String>,
}

#[derive(Debug, Serialize)]
struct MessageResp {
    message: String,
}

#[derive(Debug, Serialize)]
struct TokenResp {
    token: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LoginResp {
    token: String,
    mfa_required: bool,
    mfa_methods: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeResp {
    id: i64,
    email: String,
    email_verified: bool,
    totp_enabled: bool,
    passkeys: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TotpEnrollResp {
    secret: String,
    otpauth_uri: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CredentialResp {
    id: i64,
    created_at: DateTime<Utc>,
}

fn message(text: impl Into<String>) -> Json<MessageResp> {
    Json(MessageResp {
        message: text.into(),
    })
}

pub fn router(state: AppState) -> Router {
    Router::new()
        // public
        .route("/auth/register", post(register))
        .route("/auth/verify-email", get(verify_email))
        .route("/auth/resend-verification", post(resend))
        .route("/auth/login", post(login))
        .route("/auth/mfa/totp", post(mfa_totp))
        .route("/auth/webauthn/login/begin", post(webauthn_login_begin))
        .route("/auth/webauthn/login/complete", post(webauthn_login_complete))
        // protected
        .route("/me", get(me))
        .route("/auth/logout", post(logout))
        .route("/auth/totp/enroll/begin", post(totp_enroll_begin))
        .route("/auth/totp/enroll/verify", post(totp_enroll_verify))
        .route("/auth/totp/disable", post(totp_disable))
        .route("/auth/webauthn/register/begin", post(webauthn_register_begin))
        .route(
            "/auth/webauthn/register/complete",
            post(webauthn_register_complete),
        )
        .route("/auth/webauthn/credentials", get(credentials))
        .with_state(state)
}

async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterReq>,
) -> ApiResult<Json<MessageResp>> {
    let existing = sqlx::query_scalar::<_, i64>("SELECT id FROM users WHERE email = $1")
        .bind(&req.email)
        .fetch_optional(&state.pool)
        .await?;
    if existing.is_some() {
        return Err(ApiError::conflict("email already registered"));
    }
    if req.password.is_empty() {
        return Err(ApiError::bad_request("password must not be empty"));
    }

    let stored = password::hash_new_password(&state.config.password_params(), &req.password);
    let user_handle = uuid::Uuid::new_v4().as_bytes().to_vec();
    let now = Utc::now();

    let user_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users \
         (email, email_verified, password_salt, password_hash, user_handle, \
          totp_secret, totp_enabled, created_at) \
         VALUES ($1, false, $2, $3, $4, NULL, false, $5) RETURNING id",
    )
    .bind(&req.email)
    .bind(&stored.salt)
    .bind(&stored.hash)
    .bind(&user_handle)
    .bind(now)
    .fetch_one(&state.pool)
    .await?;

    issue_verification(&state, user_id, &req.email).await?;
    Ok(message(
        "registered; check your email to verify your address",
    ))
}

async fn issue_verification(state: &AppState, user_id: i64, email: &str) -> ApiResult<()> {
    let token = random_token(32);
    let expires = Utc::now() + Duration::seconds(state.config.email_verify_ttl_seconds);

    sqlx::query("DELETE FROM email_tokens WHERE user_id = $1")
        .bind(user_id)
        .execute(&state.pool)
        .await?;
    sqlx::query("INSERT INTO email_tokens (user_id, token, expires_at) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(&token)
        .bind(expires)
        .execute(&state.pool)
        .await?;

    let link = format!(
        "{}/auth/verify-email?token={}",
        state.config.public_base_url, token
    );
    email::send_verification_email(&state.config, email, &link).await;
    Ok(())
}

async fn verify_email(
    State(state): State<AppState>,
    Query(params): Query<VerifyEmailParams>,
) -> ApiResult<Json<MessageResp>> {
    let token = params
        .token
        .ok_or_else(|| ApiError::bad_request("missing token"))?;

    let row = sqlx::query_as::<_, crate::auth::models::EmailToken>(
        "SELECT * FROM email_tokens WHERE token = $1",
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await?;

    match row {
        Some(et) if et.expires_at >= Utc::now() => {
            sqlx::query("UPDATE users SET email_verified = true WHERE id = $1")
                .bind(et.user_id)
                .execute(&state.pool)
                .await?;
            sqlx::query("DELETE FROM email_tokens WHERE id = $1")
                .bind(et.id)
                .execute(&state.pool)
                .await?;
            Ok(message("email verified"))
        }
        _ => Err(ApiError::bad_request("invalid or expired token")),
    }
}

async fn resend(
    State(state): State<AppState>,
    Json(req): Json<EmailReq>,
) -> ApiResult<Json<MessageResp>> {
    let user = find_user_by_email(&state, &req.email).await?;
    if let Some(user) = user {
        if !user.email_verified {
            issue_verification(&state, user.id, &req.email).await?;
        }
    }
    // Never reveal whether the address exists / is already verified.
    Ok(message(
        "if the address exists and is unverified, a new link has been sent",
    ))
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginReq>,
) -> ApiResult<Json<LoginResp>> {
    let invalid = || ApiError::unauthorized("invalid email or password");

    let user = find_user_by_email(&state, &req.email)
        .await?
        .ok_or_else(invalid)?;

    let stored = StoredPassword {
        salt: user.password_salt.clone(),
        hash: user.password_hash.clone(),
    };
    if !verify_password(&state.config.password_params(), &req.password, &stored) {
        return Err(invalid());
    }
    if state.config.require_verified_email && !user.email_verified {
        return Err(ApiError::forbidden("email_not_verified"));
    }

    let mfa_needed = user.totp_enabled;
    let token = create_session(&state, user.id, mfa_needed).await?;
    Ok(Json(LoginResp {
        token,
        mfa_required: mfa_needed,
        mfa_methods: if mfa_needed {
            vec!["totp".to_string()]
        } else {
            vec![]
        },
    }))
}

async fn mfa_totp(
    State(state): State<AppState>,
    Json(req): Json<MfaTotpReq>,
) -> ApiResult<Json<TokenResp>> {
    let session = sqlx::query_as::<_, crate::auth::models::Session>(
        "SELECT * FROM sessions WHERE token = $1",
    )
    .bind(&req.token)
    .fetch_optional(&state.pool)
    .await?;

    let session = match session {
        Some(s) if s.expires_at >= Utc::now() => s,
        _ => return Err(ApiError::unauthorized("invalid or expired session")),
    };

    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(session.user_id)
        .fetch_optional(&state.pool)
        .await?;

    let secret = user
        .and_then(|u| u.totp_secret)
        .ok_or_else(|| ApiError::bad_request("TOTP is not enabled for this account"))?;

    if !totp::verify_code(&secret, &req.code) {
        return Err(ApiError::unauthorized("invalid code"));
    }

    sqlx::query("UPDATE sessions SET mfa_pending = false WHERE id = $1")
        .bind(session.id)
        .execute(&state.pool)
        .await?;

    Ok(Json(TokenResp { token: req.token }))
}

async fn webauthn_login_begin(
    State(state): State<AppState>,
    Json(req): Json<EmailReq>,
) -> ApiResult<Json<Value>> {
    let value = webauthn::begin_authentication(&state, &req.email).await?;
    Ok(Json(value))
}

async fn webauthn_login_complete(
    State(state): State<AppState>,
    Json(req): Json<WebauthnCompleteReq>,
) -> ApiResult<Json<TokenResp>> {
    let user_id = webauthn::complete_authentication(&state, &req.handle, &req.credential).await?;
    let token = create_session(&state, user_id, false).await?;
    Ok(Json(TokenResp { token }))
}

async fn me(State(state): State<AppState>, auth: AuthedUser) -> ApiResult<Json<MeResp>> {
    let passkeys = count_passkeys(&state, auth.user.id).await?;
    Ok(Json(MeResp {
        id: auth.user.id,
        email: auth.user.email,
        email_verified: auth.user.email_verified,
        totp_enabled: auth.user.totp_enabled,
        passkeys,
    }))
}

async fn logout(State(state): State<AppState>, auth: AuthedUser) -> ApiResult<Json<MessageResp>> {
    sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(auth.session.id)
        .execute(&state.pool)
        .await?;
    Ok(message("logged out"))
}

async fn totp_enroll_begin(
    State(state): State<AppState>,
    auth: AuthedUser,
) -> ApiResult<Json<TotpEnrollResp>> {
    let secret = totp::generate_secret();
    sqlx::query("UPDATE users SET totp_secret = $1, totp_enabled = false WHERE id = $2")
        .bind(&secret)
        .bind(auth.user.id)
        .execute(&state.pool)
        .await?;

    Ok(Json(TotpEnrollResp {
        secret: totp::secret_to_base32(&secret),
        otpauth_uri: totp::otpauth_uri(&state.config.rp_name, &auth.user.email, &secret),
    }))
}

async fn totp_enroll_verify(
    State(state): State<AppState>,
    auth: AuthedUser,
    Json(req): Json<CodeReq>,
) -> ApiResult<Json<MessageResp>> {
    let secret = auth
        .user
        .totp_secret
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("no TOTP enrollment in progress"))?;
    if !totp::verify_code(secret, &req.code) {
        return Err(ApiError::unauthorized("invalid code"));
    }
    sqlx::query("UPDATE users SET totp_enabled = true WHERE id = $1")
        .bind(auth.user.id)
        .execute(&state.pool)
        .await?;
    Ok(message("TOTP enabled"))
}

async fn totp_disable(
    State(state): State<AppState>,
    auth: AuthedUser,
    Json(req): Json<CodeReq>,
) -> ApiResult<Json<MessageResp>> {
    let secret = auth
        .user
        .totp_secret
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("TOTP is not enabled"))?;
    if !totp::verify_code(secret, &req.code) {
        return Err(ApiError::unauthorized("invalid code"));
    }
    sqlx::query("UPDATE users SET totp_enabled = false, totp_secret = NULL WHERE id = $1")
        .bind(auth.user.id)
        .execute(&state.pool)
        .await?;
    Ok(message("TOTP disabled"))
}

async fn webauthn_register_begin(
    State(state): State<AppState>,
    auth: AuthedUser,
) -> ApiResult<Json<Value>> {
    let value = webauthn::begin_registration(&state, &auth.user).await?;
    Ok(Json(value))
}

async fn webauthn_register_complete(
    State(state): State<AppState>,
    auth: AuthedUser,
    Json(req): Json<WebauthnCompleteReq>,
) -> ApiResult<Json<MessageResp>> {
    webauthn::complete_registration(&state, &auth.user, &req.handle, &req.credential).await?;
    Ok(message("passkey registered"))
}

async fn credentials(
    State(state): State<AppState>,
    auth: AuthedUser,
) -> ApiResult<Json<Vec<CredentialResp>>> {
    let rows = sqlx::query_as::<_, crate::auth::models::Credential>(
        "SELECT * FROM credentials WHERE user_id = $1 ORDER BY id",
    )
    .bind(auth.user.id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|c| CredentialResp {
                id: c.id,
                created_at: c.created_at,
            })
            .collect(),
    ))
}

async fn find_user_by_email(state: &AppState, email: &str) -> ApiResult<Option<User>> {
    Ok(sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
        .bind(email)
        .fetch_optional(&state.pool)
        .await?)
}

async fn count_passkeys(state: &AppState, user_id: i64) -> ApiResult<i64> {
    Ok(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM credentials WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&state.pool)
            .await?,
    )
}