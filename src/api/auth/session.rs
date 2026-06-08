use std::marker::PhantomData;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::auth::crypto::jwt::{self, Claims};
use crate::auth::crypto::token::random_token;
use crate::auth::models::{RequiredRole, Role, RoleAdmin, RolePartner, RoleUser, Session, User};
use crate::error::ApiError;
use crate::state::AppState;
use crate::utils::contants::{NOW, SESSION_MAX_AGE};

#[derive(Debug, Clone)]
pub struct Auth<R: RequiredRole> {
    pub user: User,
    _role: PhantomData<R>,
}
pub type AuthedUser = Auth<RoleUser>;
pub type AuthedAdmin = Auth<RoleAdmin>;
pub type AuthedPartner = Auth<RolePartner>;

pub async fn create_session(
    state: &AppState,
    user_id: Uuid,
    mfa_pending: bool,
) -> Result<String, ApiError> {
    let token = random_token(32);
    sqlx::query(
        "INSERT INTO sessions (user_id, token, mfa_pending, expires_at, created_at) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(user_id)
    .bind(&token)
    .bind(mfa_pending)
    .bind(SESSION_MAX_AGE)
    .bind(*NOW)
    .execute(&state.pool)
    .await?;

    let role = sqlx::query_scalar::<_, Option<String>>("SELECT role FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.pool)
        .await?
        .unwrap_or_else(|| "player".to_string());

    let claims = Claims {
        sub: user_id,
        role,
        sid: token,
        iat: (*NOW).timestamp(),
        exp: SESSION_MAX_AGE,
    };
    jwt::encode(state.config.jwt_secret.as_bytes(), &claims).map_err(ApiError::from)
}

pub fn session_token_from_jwt(state: &AppState, token: &str) -> Result<String, ApiError> {
    let claims = jwt::decode(state.config.jwt_secret.as_bytes(), token)?;
    Ok(claims.sid)
}

pub async fn lookup_valid_session(
    state: &AppState,
    token: &str,
) -> Result<Option<(Session, User)>, ApiError> {
    let Some(session) = sqlx::query_as::<_, Session>("SELECT * FROM sessions WHERE token = $1")
        .bind(token)
        .fetch_optional(&state.pool)
        .await?
    else {
        return Ok(None);
    };
    if session.expires_at < *NOW {
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

impl<R: RequiredRole + Send + Sync + 'static> FromRequestParts<AppState> for Auth<R> {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let bearer =
            extract_token(parts).ok_or_else(|| ApiError::unauthorized("missing session token"))?;
        let session_token = session_token_from_jwt(state, &bearer)?;

        let (session, user) = lookup_valid_session(state, &session_token)
            .await?
            .ok_or_else(|| ApiError::unauthorized("invalid or expired session"))?;

        if session.mfa_pending {
            return Err(ApiError::unauthorized("MFA not completed for this session"));
        }

        if let Some(required) = R::role() {
            let role = match user.role.as_str() {
                "admin" => Some(Role::Admin),
                "partener" => Some(Role::Partner),
                "user" => Some(Role::User),
                _ => None,
            };
            if role != Some(required) {
                return Err(ApiError::forbidden("insufficient permissions"));
            }
        }

        Ok(Auth {
            user,
            _role: PhantomData,
        })
    }
}
