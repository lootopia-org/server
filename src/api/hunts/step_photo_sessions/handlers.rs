use axum::{
    extract::{Path, State},
    Json,
};
use uuid::Uuid;

use crate::{
    api::hunts::step_photo_sessions::dto::{
        CreateStepPhotoSessionReq, StepPhotoSessionResp, SubmitStepPhotoReq,
    },
    auth::session::AuthedAdminOrPartner,
    error::{ApiError, ApiResult},
    event::{event::Event, event_types, topics},
    AppState,
};

const SESSION_TTL_SECS: u64 = 1800;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StepPhotoSession {
    session_id: Uuid,
    partner_id: Uuid,
    step_key: String,
    hunt_id: Option<Uuid>,
    photo_url: Option<String>,
}

fn session_key(id: Uuid) -> String {
    format!("step_photo_session:{id}")
}

fn index_key(partner_id: Uuid, step_key: &str) -> String {
    format!("step_photo_index:{partner_id}:{step_key}")
}

fn to_resp(session: &StepPhotoSession) -> StepPhotoSessionResp {
    StepPhotoSessionResp {
        session_id: session.session_id,
        step_key: session.step_key.clone(),
        hunt_id: session.hunt_id,
        photo_url: session.photo_url.clone(),
        status: if session.photo_url.is_some() {
            "completed".to_string()
        } else {
            "pending".to_string()
        },
    }
}

async fn load_session(state: &AppState, id: Uuid) -> ApiResult<StepPhotoSession> {
    state
        .event_handler
        .get::<StepPhotoSession>(&session_key(id))
        .await
        .map_err(|_| ApiError::internal("failed to load capture session"))?
        .ok_or_else(|| ApiError::not_found("capture session not found"))
}

pub async fn create_step_photo_session(
    State(state): State<AppState>,
    auth: AuthedAdminOrPartner,
    Json(req): Json<CreateStepPhotoSessionReq>,
) -> ApiResult<Json<StepPhotoSessionResp>> {
    if req.step_key.trim().is_empty() {
        return Err(ApiError::bad_request("stepKey is required"));
    }

    let index = index_key(auth.user.id, req.step_key.trim());
    if let Ok(Some(existing_id)) = state.event_handler.get::<String>(&index).await {
        if let Ok(existing_uuid) = Uuid::parse_str(&existing_id) {
            if let Ok(session) = load_session(&state, existing_uuid).await {
                if session.photo_url.is_none() {
                    return Ok(Json(to_resp(&session)));
                }
            }
        }
    }

    let session = StepPhotoSession {
        session_id: Uuid::new_v4(),
        partner_id: auth.user.id,
        step_key: req.step_key.trim().to_string(),
        hunt_id: req.hunt_id,
        photo_url: None,
    };

    state
        .event_handler
        .set_with_ttl(&session_key(session.session_id), &session, SESSION_TTL_SECS)
        .await
        .map_err(|_| ApiError::internal("failed to store capture session"))?;
    state
        .event_handler
        .set_with_ttl(
            &index,
            &session.session_id.to_string(),
            SESSION_TTL_SECS,
        )
        .await
        .map_err(|_| ApiError::internal("failed to store capture session index"))?;

    Ok(Json(to_resp(&session)))
}

pub async fn get_step_photo_session(
    State(state): State<AppState>,
    auth: AuthedAdminOrPartner,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<StepPhotoSessionResp>> {
    let session = load_session(&state, id).await?;
    if session.partner_id != auth.user.id && auth.user.role != "admin" {
        return Err(ApiError::forbidden("you do not have access to this capture session"));
    }
    Ok(Json(to_resp(&session)))
}

pub async fn submit_step_photo(
    State(state): State<AppState>,
    auth: AuthedAdminOrPartner,
    Path(id): Path<Uuid>,
    Json(req): Json<SubmitStepPhotoReq>,
) -> ApiResult<Json<StepPhotoSessionResp>> {
    if req.photo_url.trim().is_empty() {
        return Err(ApiError::bad_request("photoUrl is required"));
    }

    let mut session = load_session(&state, id).await?;
    if session.partner_id != auth.user.id {
        return Err(ApiError::forbidden("you do not have access to this capture session"));
    }

    session.photo_url = Some(req.photo_url.trim().to_string());
    state
        .event_handler
        .set_with_ttl(&session_key(id), &session, SESSION_TTL_SECS)
        .await
        .map_err(|_| ApiError::internal("failed to update capture session"))?;

    state.event_handler.publish(
        Event::new(
            event_types::HUNT_STEPS_PHOTO_CAPTURED,
            topics::HUNT_STEPS,
            serde_json::json!({
                "sessionId": session.session_id,
                "stepKey": session.step_key,
                "photoUrl": session.photo_url,
                "huntId": session.hunt_id,
                "partnerId": session.partner_id,
            }),
        )
        .with_resource_id(session.partner_id),
    );

    Ok(Json(to_resp(&session)))
}
