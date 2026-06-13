use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use std::collections::HashMap;
use uuid::Uuid;

use crate::{
    api::hunts::hunt_steps::models::HuntStep, auth::session::AuthedAdminOrPartner, error::ApiError,
    hunts::models::Hunt, query_get, AppState,
};

pub struct OwnedHunt(pub Hunt);

impl FromRequestParts<AppState> for OwnedHunt {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = AuthedAdminOrPartner::from_request_parts(parts, state).await?;

        let Path(params) = Path::<HashMap<String, String>>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::bad_request("missing path parameter"))?;

        let id: Uuid = params
            .get("id")
            .and_then(|v| Uuid::parse_str(v).ok())
            .ok_or_else(|| ApiError::bad_request("invalid or missing hunt id"))?;

        let hunt = query_get!(&state.pool, Hunt, "hunts", "id", id)
            .ok_or_else(|| ApiError::not_found("hunt not found"))?;

        if auth.user.role != "admin" && hunt.partner_id != auth.user.id {
            return Err(ApiError::forbidden("you do not have access to this hunt"));
        }

        Ok(OwnedHunt(hunt))
    }
}

pub struct OwnedHuntStep(pub HuntStep);

impl FromRequestParts<AppState> for OwnedHuntStep {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = AuthedAdminOrPartner::from_request_parts(parts, state).await?;

        let Path(params) = Path::<HashMap<String, String>>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::bad_request("missing path parameter"))?;

        let step_id: Uuid = params
            .get("id")
            .and_then(|v| Uuid::parse_str(v).ok())
            .ok_or_else(|| ApiError::bad_request("invalid or missing step id"))?;

        let step = query_get!(&state.pool, HuntStep, "hunt_steps", "id", step_id)
            .ok_or_else(|| ApiError::not_found("step not found"))?;

        if auth.user.role != "admin" {
            let hunt = query_get!(&state.pool, Hunt, "hunts", "id", step.hunt_id)
                .ok_or_else(|| ApiError::not_found("parent hunt not found"))?;

            if hunt.partner_id != auth.user.id {
                return Err(ApiError::forbidden(
                    "you do not have access to this hunt's steps",
                ));
            }
        }

        Ok(OwnedHuntStep(step))
    }
}
