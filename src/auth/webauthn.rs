use anyhow::Context;
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use webauthn_rs::prelude::*;

use crate::config::Config;
use crate::auth::crypto::token::random_token;
use crate::auth::error::ApiError;
use crate::auth::models::{AuthCeremony, Credential, User};
use crate::auth::state::AppState;

pub fn build(config: &Config) -> anyhow::Result<Webauthn> {
    let rp_origin = Url::parse(&config.origin).context("parsing ORIGIN as a URL")?;
    WebauthnBuilder::new(&config.rp_id, &rp_origin)
        .context("invalid WebAuthn configuration")?
        .rp_name(&config.rp_name)
        .build()
        .context("building WebAuthn instance")
}

pub async fn begin_registration(state: &AppState, user: &User) -> Result<Value, ApiError> {
    let user_unique_id = user_uuid(user)?;

    let exclude: Vec<CredentialID> = load_credentials(state, user.id)
        .await?
        .iter()
        .map(|c| CredentialID::from(c.credential_id.as_slice()))
        .collect();

    let (ccr, reg_state) = state
        .webauthn
        .start_passkey_registration(
            user_unique_id,
            &user.email,
            &user.email,
            Some(exclude),
        )
        .map_err(|e| ApiError::bad_request(format!("could not start registration: {e}")))?;

    let state_json = serde_json::to_value(&reg_state).map_err(internal)?;
    let handle = store_ceremony(state, "register", Some(user.id), state_json).await?;

    public_key_response(handle, &ccr).map_err(internal)
}

pub async fn complete_registration(
    state: &AppState,
    user: &User,
    handle: &str,
    credential_json: &Value,
) -> Result<(), ApiError> {
    let ceremony = load_ceremony(state, "register", handle).await?;
    let reg_state: PasskeyRegistration =
        serde_json::from_value(ceremony.state).map_err(internal)?;
    let reg: RegisterPublicKeyCredential = serde_json::from_value(credential_json.clone())
        .map_err(|e| ApiError::bad_request(format!("could not parse credential: {e}")))?;

    let passkey = state
        .webauthn
        .finish_passkey_registration(&reg, &reg_state)
        .map_err(|e| ApiError::bad_request(format!("registration failed: {e}")))?;

    let cred_id = passkey.cred_id().as_ref().to_vec();

    if let Some(existing) =
        sqlx::query_as::<_, Credential>("SELECT * FROM credentials WHERE credential_id = $1")
            .bind(&cred_id)
            .fetch_optional(&state.pool)
            .await?
    {
        if existing.user_id != user.id {
            return Err(ApiError::bad_request(
                "credential already registered to another account",
            ));
        }
    }

    let passkey_json = serde_json::to_value(&passkey).map_err(internal)?;
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO credentials (user_id, credential_id, passkey, created_at) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (credential_id) DO UPDATE \
         SET user_id = EXCLUDED.user_id, passkey = EXCLUDED.passkey",
    )
    .bind(user.id)
    .bind(&cred_id)
    .bind(&passkey_json)
    .bind(now)
    .execute(&state.pool)
    .await?;

    Ok(())
}

pub async fn begin_authentication(state: &AppState, email: &str) -> Result<Value, ApiError> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
        .bind(email)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::not_found("no such user"))?;

    let passkeys = load_passkeys(state, user.id).await?;
    if passkeys.is_empty() {
        return Err(ApiError::not_found("user has no passkeys"));
    }

    let (rcr, auth_state) = state
        .webauthn
        .start_passkey_authentication(&passkeys)
        .map_err(|e| ApiError::bad_request(format!("could not start authentication: {e}")))?;

    let state_json = serde_json::to_value(&auth_state).map_err(internal)?;
    let handle = store_ceremony(state, "authenticate", Some(user.id), state_json).await?;

    public_key_response(handle, &rcr).map_err(internal)
}

pub async fn complete_authentication(
    state: &AppState,
    handle: &str,
    credential_json: &Value,
) -> Result<i64, ApiError> {
    let ceremony = load_ceremony(state, "authenticate", handle).await?;
    let user_id = ceremony
        .user_id
        .ok_or_else(|| ApiError::bad_request("ceremony has no user"))?;
    let auth_state: PasskeyAuthentication =
        serde_json::from_value(ceremony.state).map_err(internal)?;
    let pkc: PublicKeyCredential = serde_json::from_value(credential_json.clone())
        .map_err(|e| ApiError::bad_request(format!("could not parse credential: {e}")))?;

    let result = state
        .webauthn
        .finish_passkey_authentication(&pkc, &auth_state)
        .map_err(|e| ApiError::unauthorized(format!("authentication failed: {e}")))?;

    if result.needs_update() {
        let cred_id = result.cred_id().as_ref().to_vec();
        if let Some(row) =
            sqlx::query_as::<_, Credential>("SELECT * FROM credentials WHERE credential_id = $1")
                .bind(&cred_id)
                .fetch_optional(&state.pool)
                .await?
        {
            let mut passkey: Passkey = serde_json::from_value(row.passkey).map_err(internal)?;
            if passkey.update_credential(&result) == Some(true) {
                let passkey_json = serde_json::to_value(&passkey).map_err(internal)?;
                sqlx::query("UPDATE credentials SET passkey = $1 WHERE id = $2")
                    .bind(&passkey_json)
                    .bind(row.id)
                    .execute(&state.pool)
                    .await?;
            }
        }
    }

    Ok(user_id)
}

fn user_uuid(user: &User) -> Result<Uuid, ApiError> {
    Uuid::from_slice(&user.user_handle)
        .map_err(|_| ApiError::internal("stored user handle is not a valid id"))
}

fn public_key_response<T: serde::Serialize>(
    handle: String,
    challenge: &T,
) -> serde_json::Result<Value> {
    let value = serde_json::to_value(challenge)?;
    let public_key = value
        .get("publicKey")
        .cloned()
        .unwrap_or(value);
    Ok(json!({ "handle": handle, "publicKey": public_key }))
}

async fn load_credentials(state: &AppState, user_id: i64) -> Result<Vec<Credential>, ApiError> {
    Ok(
        sqlx::query_as::<_, Credential>("SELECT * FROM credentials WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(&state.pool)
            .await?,
    )
}

async fn load_passkeys(state: &AppState, user_id: i64) -> Result<Vec<Passkey>, ApiError> {
    let mut passkeys = Vec::new();
    for cred in load_credentials(state, user_id).await? {
        passkeys.push(serde_json::from_value(cred.passkey).map_err(internal)?);
    }
    Ok(passkeys)
}

async fn store_ceremony(
    state: &AppState,
    purpose: &str,
    user_id: Option<i64>,
    ceremony_state: Value,
) -> Result<String, ApiError> {
    let handle = random_token(24);
    let expires = Utc::now() + Duration::seconds(state.config.ceremony_ttl_seconds);
    sqlx::query(
        "INSERT INTO auth_ceremonies (handle, purpose, state, user_id, expires_at) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&handle)
    .bind(purpose)
    .bind(&ceremony_state)
    .bind(user_id)
    .bind(expires)
    .execute(&state.pool)
    .await?;
    Ok(handle)
}

async fn load_ceremony(
    state: &AppState,
    purpose: &str,
    handle: &str,
) -> Result<AuthCeremony, ApiError> {
    let ceremony =
        sqlx::query_as::<_, AuthCeremony>("SELECT * FROM auth_ceremonies WHERE handle = $1")
            .bind(handle)
            .fetch_optional(&state.pool)
            .await?;

    match ceremony {
        Some(c) if c.purpose == purpose && c.expires_at >= Utc::now() => {
            sqlx::query("DELETE FROM auth_ceremonies WHERE id = $1")
                .bind(c.id)
                .execute(&state.pool)
                .await?;
            Ok(c)
        }
        _ => Err(ApiError::bad_request("unknown or expired ceremony")),
    }
}

fn internal<E: std::fmt::Display>(err: E) -> ApiError {
    tracing::error!(error = %err, "webauthn serialization error");
    ApiError::internal("internal server error")
}
