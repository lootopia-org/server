use std::marker::PhantomData;

use axum::extract::FromRequestParts;
use axum::http::header::{AUTHORIZATION, COOKIE};
use axum::http::request::Parts;
use axum::http::HeaderMap;
use chrono::Duration;
use uuid::Uuid;

use crate::auth::crypto::jwt::{self, Claims};
use crate::auth::models::{
    Role, RoleAdmin, RoleAdminOrPartener, RolePartner, RolePlyer, Session, User,
};
use crate::error::ApiError;
use crate::state::AppState;
use crate::utils::contants::{NOW, SESSION_MAX_AGE};
use crate::{query_create, query_get, query_scale};

pub trait RequiredRole {
    fn roles() -> &'static [Role];
}

#[derive(Debug, Clone)]
pub struct Auth<R: RequiredRole> {
    pub user: User,
    _role: PhantomData<R>,
}
pub type AuthedUser = Auth<RolePlyer>;
pub type AuthedAdmin = Auth<RoleAdmin>;
pub type AuthedPartner = Auth<RolePartner>;
pub type AuthedAdminOrPartner = Auth<RoleAdminOrPartener>;

pub fn extract_session_token(headers: &HeaderMap) -> Option<String> {
    if let Some(cookie_header) = headers.get(COOKIE).and_then(|v| v.to_str().ok()) {
        if let Some(token) = cookie_header.split(';').map(str::trim).find_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            (k == "session").then(|| v.to_string())
        }) {
            return Some(token);
        }
    }

    headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
}

pub async fn create_session(
    state: &AppState,
    user_id: Uuid,
    mfa_pending: bool,
) -> Result<String, ApiError> {
    let role: Option<String> =
        query_scale!(&state.pool, "SELECT role FROM users WHERE id = $1", user_id);
    let role = role.unwrap_or_else(|| "player".to_string());

    let token = {
        let claims = Claims {
            sub: user_id,
            role,
            sid: Uuid::new_v4().to_string(),
            iat: (*NOW).timestamp(),
            exp: SESSION_MAX_AGE,
        };
        jwt::encode(state.config.jwt_secret.as_bytes(), &claims).map_err(ApiError::from)?
    };

    query_create!(
        &state.pool,
        "sessions",
        "user_id" => user_id,
        "token" => &token,
        "mfa_pending" => mfa_pending,
        "expires_at" => *NOW + Duration::seconds(SESSION_MAX_AGE),
        "created_at" => *NOW
    );

    Ok(token)
}

pub async fn lookup_valid_session(
    state: &AppState,
    token: &str,
) -> Result<Option<(Session, User)>, ApiError> {
    let Some(session) = query_get!(&state.pool, Session, "sessions", "token", token) else {
        return Ok(None);
    };
    if session.expires_at < *NOW {
        return Ok(None);
    }
    let user = query_get!(&state.pool, User, "users", "id", session.user_id);
    Ok(user.map(|u| (session, u)))
}

impl<R: RequiredRole + Send + Sync + 'static> FromRequestParts<AppState> for Auth<R> {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = extract_session_token(&parts.headers)
            .ok_or_else(|| ApiError::unauthorized("missing session token"))?;

        let (session, user) = lookup_valid_session(state, &token)
            .await?
            .ok_or_else(|| ApiError::unauthorized("invalid or expired session"))?;

        if session.mfa_pending {
            return Err(ApiError::unauthorized("MFA not completed for this session"));
        }

        let role = match user.role.as_str() {
            "admin" => Role::Admin,
            "partner" => Role::Partner,
            "player" => Role::Player,
            _ => return Err(ApiError::forbidden("unknown role")),
        };
        if !R::roles().contains(&role) {
            return Err(ApiError::forbidden("insufficient permissions"));
        }

        Ok(Auth {
            user,
            _role: PhantomData,
        })
    }
}
