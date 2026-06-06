use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::{Duration, Utc};

use crate::auth::crypto::token::random_token;
use crate::auth::error::ApiError;
use crate::auth::models::{Session, User};
use crate::auth::state::AppState;

#[derive(Debug, Clone)]
pub struct AuthedUser {
    pub user: User,
    pub session: Session,
}

pub async fn create_session(
    state: &AppState,
    user_id: i64,
    mfa_pending: bool,
) -> Result<String, ApiError> {
    let token = random_token(32);
    let now = Utc::now();
    let expires = now + Duration::seconds(state.config.session_ttl_seconds);
    sqlx::query(
        "INSERT INTO sessions (user_id, token, mfa_pending, expires_at, created_at) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(user_id)
    .bind(&token)
    .bind(mfa_pending)
    .bind(expires)
    .bind(now)
    .execute(&state.pool)
    .await?;
    Ok(token)
}

pub async fn lookup_valid_session(
    state: &AppState,
    token: &str,
) -> Result<Option<(Session, User)>, ApiError> {
    let Some(session) =
        sqlx::query_as::<_, Session>("SELECT * FROM sessions WHERE token = $1")
            .bind(token)
            .fetch_optional(&state.pool)
            .await?
    else {
        return Ok(None);
    };
    if session.expires_at < Utc::now() {
        return Ok(None);
    }
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(session.user_id)
        .fetch_optional(&state.pool)
        .await?;
    Ok(user.map(|u| (session, u)))
}

pub fn extract_token(parts: &Parts) -> Option<String> {
    if let Some(value) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        if let Some(token) = value.strip_prefix("Bearer ") {
            return Some(token.to_string());
        }
    }

    let cookie_header = parts
        .headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())?;
    for pair in cookie_header.split(';') {
        if let Some((name, value)) = pair.trim().split_once('=') {
            if name == "session" {
                return Some(value.to_string());
            }
        }
    }
    None
}

impl FromRequestParts<AppState> for AuthedUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token =
            extract_token(parts).ok_or_else(|| ApiError::unauthorized("missing session token"))?;

        let (session, user) = lookup_valid_session(state, &token)
            .await?
            .ok_or_else(|| ApiError::unauthorized("invalid or expired session"))?;

        if session.mfa_pending {
            return Err(ApiError::unauthorized(
                "MFA not completed for this session",
            ));
        }

        Ok(AuthedUser { user, session })
    }
}
