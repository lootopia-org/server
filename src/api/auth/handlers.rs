use axum::{
    extract::{Query, State},
    http::{header::SET_COOKIE, HeaderValue},
    response::IntoResponse,
    Json,
};
use chrono::Duration;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    auth::{
        crypto::{
            password::{self, verify_password, StoredPassword},
            token::{self, random_token},
            totp,
        },
        dto::{
            CodeReq, CredentialResp, EmailReq, ForgotPasswordReq, LoginReq, LoginResp, MeResp,
            MfaTotpReq, RegisterReq, ResetPasswordReq, TokenResp, TotpEnrollResp,
            VerifyEmailParams, WebauthnCompleteReq,
        },
        email,
        models::User,
        session::{create_session, session_token_from_jwt, AuthedUser},
        webauthn,
    },
    error::{ApiError, ApiResult},
    state::AppState,
    utils::{
        contants::{NOW, SESSION_MAX_AGE},
        json::{message, MessageResp},
    },
};

pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterReq>,
) -> ApiResult<Json<MessageResp>> {
    let existing = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE email = $1")
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

    let user_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users \
         (username, email, email_verified, password_salt, password_hash, role, \
          totp_secret, totp_enabled, avatar, bio, created_at, updated_at) \
         VALUES ($1, $2, false, $3, $4, $5, user, NULL, false, $6, $7, &8, $9) RETURNING id",
    )
    .bind(&req.username)
    .bind(&req.email)
    .bind(&stored.salt)
    .bind(&stored.hash)
    .bind(&req.avatar)
    .bind(&req.bio)
    .bind(*NOW)
    .bind(*NOW)
    .fetch_one(&state.pool)
    .await?;

    issue_verification(&state, user_id, &req.email).await?;
    Ok(message(
        "registered; check your email to verify your address",
    ))
}

pub async fn issue_verification(state: &AppState, user_id: Uuid, email: &str) -> ApiResult<()> {
    let token = random_token(32);
    let expires = *NOW + Duration::seconds(state.config.email_verify_ttl_seconds);

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

pub async fn verify_email(
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
        Some(et) if et.expires_at >= *NOW => {
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

pub async fn resend(
    State(state): State<AppState>,
    Json(req): Json<EmailReq>,
) -> ApiResult<Json<MessageResp>> {
    let user = find_user_by_email(&state, Some(&req.email)).await?;
    if let Some(user) = user {
        if !user.email_verified {
            issue_verification(&state, user.id, &req.email).await?;
        }
    }
    Ok(message(
        "if the address exists and is unverified, a new link has been sent",
    ))
}

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginReq>,
) -> ApiResult<impl IntoResponse> {
    let invalid = || ApiError::unauthorized("invalid user login");

    let user = if req.email.is_some() {
        find_user_by_email(&state, req.email.as_deref())
            .await?
            .ok_or_else(invalid)?
    } else if req.username.is_some() {
        find_user_by_username(&state, req.username.as_deref())
            .await?
            .ok_or_else(invalid)?
    } else {
        return Err(ApiError::bad_request("Can not process request"));
    };

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
    let cookie = format!(
        "session={}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age={}",
        token, SESSION_MAX_AGE
    );
    Ok((
        [(SET_COOKIE, HeaderValue::from_str(&cookie).unwrap())],
        Json(LoginResp {
            token,
            mfa_required: mfa_needed,
            mfa_methods: if mfa_needed {
                vec!["totp".to_string()]
            } else {
                vec![]
            },
        }),
    ))
}

pub async fn forgot_password(
    State(state): State<AppState>,
    Json(req): Json<ForgotPasswordReq>,
) -> ApiResult<Json<MessageResp>> {
    let generic = || message("if an account exists for that email, a reset link has been sent");

    let Some(user) = find_user_by_email(&state, Some(&req.email)).await? else {
        return Ok(generic());
    };

    let token = random_token(32);
    let token_hash = token::sha256(token.as_bytes());
    let expires = *NOW + Duration::seconds(SESSION_MAX_AGE);

    sqlx::query("DELETE FROM password_reset_tokens WHERE user_id = $1")
        .bind(user.id)
        .execute(&state.pool)
        .await?;
    sqlx::query(
        "INSERT INTO password_reset_tokens (user_id, token_hash, expires_at) \
         VALUES ($1, $2, $3)",
    )
    .bind(user.id)
    .bind(&token_hash)
    .bind(expires)
    .execute(&state.pool)
    .await?;

    let link = format!(
        "{}/auth/reset-password?token={}",
        state.config.public_base_url, token
    );
    email::send_password_reset_email(&state.config, &user.email, &link).await;

    Ok(generic())
}

pub async fn reset_password(
    State(state): State<AppState>,
    Json(req): Json<ResetPasswordReq>,
) -> ApiResult<Json<MessageResp>> {
    if req.new_password.is_empty() {
        return Err(ApiError::bad_request("new_password must not be empty"));
    }

    let token_hash = token::sha256(req.token.as_bytes());
    let invalid = || ApiError::bad_request("invalid or expired token");

    let row = sqlx::query_as::<_, crate::auth::models::PasswordResetToken>(
        "SELECT * FROM password_reset_tokens WHERE token_hash = $1",
    )
    .bind(&token_hash)
    .fetch_optional(&state.pool)
    .await?;

    let reset = match row {
        Some(r) if r.expires_at >= *NOW => r,
        Some(r) => {
            sqlx::query("DELETE FROM password_reset_tokens WHERE id = $1")
                .bind(r.id)
                .execute(&state.pool)
                .await?;
            return Err(invalid());
        }
        None => return Err(invalid()),
    };

    let stored = password::hash_new_password(&state.config.password_params(), &req.new_password);

    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "UPDATE users SET password_salt = $1, password_hash = $2, updated_at = $3 WHERE id = $4",
    )
    .bind(&stored.salt)
    .bind(&stored.hash)
    .bind(*NOW)
    .bind(reset.user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM password_reset_tokens WHERE user_id = $1")
        .bind(reset.user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(reset.user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(message("your password has been reset; please log in again"))
}

pub async fn mfa_totp(
    State(state): State<AppState>,
    Json(req): Json<MfaTotpReq>,
) -> ApiResult<Json<TokenResp>> {
    let session_token = session_token_from_jwt(&state, &req.token)?;
    let session = sqlx::query_as::<_, crate::auth::models::Session>(
        "SELECT * FROM sessions WHERE token = $1",
    )
    .bind(&session_token)
    .fetch_optional(&state.pool)
    .await?;

    let session = match session {
        Some(s) if s.expires_at >= *NOW => s,
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

pub async fn webauthn_login_begin(
    State(state): State<AppState>,
    Json(req): Json<EmailReq>,
) -> ApiResult<Json<Value>> {
    let value = webauthn::begin_authentication(&state, &req.email).await?;
    Ok(Json(value))
}

pub async fn webauthn_login_complete(
    State(state): State<AppState>,
    Json(req): Json<WebauthnCompleteReq>,
) -> ApiResult<Json<TokenResp>> {
    let user_id = webauthn::complete_authentication(&state, &req.handle, &req.credential).await?;
    let token = create_session(&state, user_id, false).await?;
    Ok(Json(TokenResp { token }))
}

pub async fn me(State(state): State<AppState>, auth: AuthedUser) -> ApiResult<Json<MeResp>> {
    let passkeys = count_passkeys(&state, auth.user.id).await?;
    Ok(Json(MeResp {
        id: auth.user.id,
        username: auth.user.username,
        email: auth.user.email,
        email_verified: auth.user.email_verified,
        totp_enabled: auth.user.totp_enabled,
        passkeys,
    }))
}

pub async fn logout(
    State(state): State<AppState>,
    auth: AuthedUser,
) -> ApiResult<Json<MessageResp>> {
    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(auth.user.id)
        .execute(&state.pool)
        .await?;
    Ok(message("logged out"))
}

pub async fn totp_enroll_begin(
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

pub async fn totp_enroll_verify(
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

pub async fn totp_disable(
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

pub async fn webauthn_register_begin(
    State(state): State<AppState>,
    auth: AuthedUser,
) -> ApiResult<Json<Value>> {
    let value = webauthn::begin_registration(&state, &auth.user).await?;
    Ok(Json(value))
}

pub async fn webauthn_register_complete(
    State(state): State<AppState>,
    auth: AuthedUser,
    Json(req): Json<WebauthnCompleteReq>,
) -> ApiResult<Json<MessageResp>> {
    webauthn::complete_registration(&state, &auth.user, &req.handle, &req.credential).await?;
    Ok(message("passkey registered"))
}

pub async fn credentials(
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

async fn find_user_by_email(state: &AppState, email: Option<&str>) -> ApiResult<Option<User>> {
    Ok(
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
            .bind(email)
            .fetch_optional(&state.pool)
            .await?,
    )
}

async fn find_user_by_username(
    state: &AppState,
    username: Option<&str>,
) -> ApiResult<Option<User>> {
    Ok(
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = $1")
            .bind(username)
            .fetch_optional(&state.pool)
            .await?,
    )
}

async fn count_passkeys(state: &AppState, user_id: Uuid) -> ApiResult<i64> {
    Ok(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM credentials WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&state.pool)
            .await?,
    )
}
