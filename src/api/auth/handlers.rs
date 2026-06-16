use axum::{
    extract::{Query, State},
    http::{header::SET_COOKIE, HeaderValue},
    response::{IntoResponse, Redirect},
    Json,
};
use axum_extra::extract::CookieJar;
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
            MfaTotpReq, RegisterReq, ResetPasswordReq, TokenResp, TotpEnrollResp, UpdateMeta,
            VerifyEmailParams, WebauthnCompleteReq,
        },
        email,
        models::{Credential, EmailToken, PasswordResetToken, Session, User},
        session::{create_session, AuthedUser},
        webauthn,
    },
    error::{ApiError, ApiResult},
    profiles::models::UserProfiles,
    query_create, query_delete, query_get, query_scale, query_update,
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
    let existing: bool = query_scale!(
        &state.pool,
        "SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)",
        &req.email
    );
    if existing {
        return Err(ApiError::conflict("email already registered"));
    }
    if req.password.is_empty() {
        return Err(ApiError::bad_request("password must not be empty"));
    }

    let stored = password::hash_new_password(&state.config.password_params(), &req.password);
    let avatar = req.avatar.filter(|s| !s.is_empty());
    let bio = req.bio.filter(|s| !s.is_empty());

    let user = query_create!(&state.pool, User, "users",
        "username" => &req.username,
        "email" => &req.email,
        "email_verified" => false,
        "password_salt" => &stored.salt,
        "password_hash" => &stored.hash,
        "role" => "player",
        "totp_secret" => None::<Vec<u8>>,
        "totp_enabled" => false,
        "avatar" => avatar,
        "bio" => bio,
        "created_at" => *NOW,
        "updated_at" => *NOW
    );

    issue_verification(&state, user.id, &req.email).await?;
    Ok(message(
        "registered; check your email to verify your address",
    ))
}

pub async fn update_meta_data(
    State(state): State<AppState>,
    auth: AuthedUser,
    Json(req): Json<UpdateMeta>,
) -> ApiResult<Json<UpdateMeta>> {
    let user = query_update!(
        &state.pool,
        User,
        "users",
        "id",
        auth.user.id,
        "bio" => &req.bio,
        "avatar" => &req.avatar
    );

    Ok(Json(user.into()))
}

pub async fn issue_verification(state: &AppState, user_id: Uuid, email: &str) -> ApiResult<()> {
    let token = random_token(32);
    let expires = *NOW + Duration::seconds(state.config.email_verify_ttl_seconds);

    query_delete!(&state.pool, "email_tokens", "user_id", user_id);
    query_create!(&state.pool, "email_tokens",
        "user_id" => user_id,
        "token" => &token,
        "expires_at" => expires
    );

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
) -> ApiResult<impl IntoResponse> {
    let token = params
        .token
        .ok_or_else(|| ApiError::bad_request("missing token"))?;

    let row = query_get!(&state.pool, EmailToken, "email_tokens", "token", &token);

    match row {
        Some(et) if et.expires_at >= *NOW => {
            query_update!(
                &state.pool,
                User,
                "users",
                "id",
                et.user_id,
                "email_verified" => Some(true)
            );
            query_delete!(&state.pool, "email_tokens", "id", et.id);
            Ok(
                Redirect::to(format!("{}/auth/login", state.config.origin.as_str()).as_str())
                    .into_response(),
            )
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

    let existing = query_get!(
        &state.pool,
        UserProfiles,
        "user_profiles",
        "user_id",
        user.id
    );
    if existing.is_none() {
        query_create!(&state.pool, UserProfiles, "user_profiles",
            "user_id" => user.id,
            "points" => 0,
            "level" => 1.0,
            "completed_hunts" => 0,
            "updated_at" => *NOW
        );
    }

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

    query_delete!(&state.pool, "password_reset_tokens", "user_id", user.id);
    query_create!(&state.pool, "password_reset_tokens",
        "user_id" => user.id,
        "token_hash" => &token_hash,
        "expires_at" => expires
    );

    let link = format!(
        "{}/auth/reset-password?token={}",
        state.config.frontend_url, token
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

    let row = query_get!(
        &state.pool,
        PasswordResetToken,
        "password_reset_tokens",
        "token_hash",
        &token_hash
    );

    let reset = match row {
        Some(r) if r.expires_at >= *NOW => r,
        Some(r) => {
            query_delete!(&state.pool, "password_reset_tokens", "id", r.id);
            return Err(invalid());
        }
        None => return Err(invalid()),
    };

    let stored = password::hash_new_password(&state.config.password_params(), &req.new_password);

    let mut tx = state.pool.begin().await?;
    query_update!(
        &mut *tx,
        User,
        "users",
        "id",
        reset.user_id,
        "password_salt" => Some(&stored.salt),
        "password_hash" => Some(&stored.hash),
        "updated_at" => Some(*NOW)
    );
    query_delete!(&mut *tx, "password_reset_tokens", "user_id", reset.user_id);
    query_delete!(&mut *tx, "sessions", "user_id", reset.user_id);
    tx.commit().await?;

    Ok(message("your password has been reset; please log in again"))
}

pub async fn mfa_totp(
    jar: CookieJar,
    State(state): State<AppState>,
    Json(req): Json<MfaTotpReq>,
) -> ApiResult<Json<TokenResp>> {
    let token = jar
        .get("session")
        .map(|c| c.value().to_string())
        .ok_or(ApiError::unauthorized("token not found"))?;
    let session = query_get!(&state.pool, Session, "sessions", "token", &token);

    let session = match session {
        Some(s) if s.expires_at >= *NOW => s,
        _ => return Err(ApiError::unauthorized("invalid or expired session")),
    };

    let user = query_get!(&state.pool, User, "users", "id", session.user_id);

    let secret = user
        .and_then(|u| u.totp_secret)
        .ok_or_else(|| ApiError::bad_request("TOTP is not enabled for this account"))?;

    if !totp::verify_code(&secret, &req.code) {
        return Err(ApiError::unauthorized("invalid code"));
    }

    query_update!(
        &state.pool,
        Session,
        "sessions",
        "id",
        session.id,
        "mfa_pending" => Some(false)
    );

    Ok(Json(TokenResp { token: token }))
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
        role: auth.user.role,
        username: auth.user.username,
        email: auth.user.email,
        email_verified: auth.user.email_verified,
        totp_enabled: auth.user.totp_enabled,
        passkeys,
        avatar: auth.user.avatar,
        bio: auth.user.bio,
    }))
}

pub async fn logout(
    State(state): State<AppState>,
    auth: AuthedUser,
) -> ApiResult<Json<MessageResp>> {
    query_delete!(&state.pool, "sessions", "user_id", auth.user.id);
    Ok(message("logged out"))
}

pub async fn totp_enroll_begin(
    State(state): State<AppState>,
    auth: AuthedUser,
) -> ApiResult<Json<TotpEnrollResp>> {
    let secret = totp::generate_secret();
    query_update!(
        &state.pool,
        User,
        "users",
        "id",
        auth.user.id,
        "totp_secret" => Some(&secret),
        "totp_enabled" => Some(false)
    );

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
    query_update!(
        &state.pool,
        User,
        "users",
        "id",
        auth.user.id,
        "totp_enabled" => Some(true)
    );
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
    query_update!(
        &state.pool,
        User,
        "users",
        "id",
        auth.user.id,
        "totp_enabled" => Some(false),
        "totp_secret" => Some(None::<Vec<u8>>)
    );
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
    let rows =
        sqlx::query_as::<_, Credential>("SELECT * FROM credentials WHERE user_id = $1 ORDER BY id")
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
    Ok(query_get!(&state.pool, User, "users", "email", email))
}

async fn find_user_by_username(
    state: &AppState,
    username: Option<&str>,
) -> ApiResult<Option<User>> {
    Ok(query_get!(&state.pool, User, "users", "username", username))
}

async fn count_passkeys(state: &AppState, user_id: Uuid) -> ApiResult<i64> {
    Ok(query_scale!(
        &state.pool,
        "SELECT COUNT(*) FROM credentials WHERE user_id = $1",
        user_id
    ))
}
