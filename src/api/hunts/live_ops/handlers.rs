use std::collections::HashMap;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use crate::{
    api::hunts::live_ops::dto::{
        HuntLiveOpsResp, StepLiveOpsOverride, UpdateStepLiveOpsReq,
    },
    api::middleware::ownership::OwnedHunt,
    auth::session::AuthedUser,
    error::{ApiError, ApiResult},
    event::{event::Event, event_types, topics},
    hunts::hunt_steps::models::HuntStep,
    query_get,
    AppState,
};

const LIVE_OPS_TTL_SECS: u64 = 86_400;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct HuntLiveOpsState {
    hunt_id: Uuid,
    steps: HashMap<String, StepLiveOpsOverride>,
}

fn storage_key(hunt_id: Uuid) -> String {
    format!("live_ops:{hunt_id}")
}

async fn load_state(state: &AppState, hunt_id: Uuid) -> ApiResult<HuntLiveOpsState> {
    Ok(state
        .event_handler
        .get::<HuntLiveOpsState>(&storage_key(hunt_id))
        .await
        .map_err(|_| ApiError::internal("failed to load live ops state"))?
        .unwrap_or(HuntLiveOpsState {
            hunt_id,
            steps: HashMap::new(),
        }))
}

async fn save_state(state: &AppState, live_ops: &HuntLiveOpsState) -> ApiResult<()> {
    state
        .event_handler
        .set_with_ttl(&storage_key(live_ops.hunt_id), live_ops, LIVE_OPS_TTL_SECS)
        .await
        .map_err(|_| ApiError::internal("failed to store live ops state"))
}

fn to_resp(state: &HuntLiveOpsState) -> HuntLiveOpsResp {
    HuntLiveOpsResp {
        hunt_id: state.hunt_id,
        steps: state.steps.clone(),
    }
}

fn publish_live_ops_updated(state: &AppState, hunt_id: Uuid, step_id: Uuid, override_state: &StepLiveOpsOverride) {
    state.event_handler.publish(
        Event::new(
            event_types::HUNT_STEPS_LIVE_OPS_UPDATED,
            topics::HUNT_STEPS,
            serde_json::json!({
                "huntId": hunt_id,
                "stepId": step_id,
                "paused": override_state.paused,
                "redirect": override_state.redirect,
            }),
        )
        .with_resource_id(hunt_id),
    );
}

pub async fn get_hunt_live_ops(
    State(state): State<AppState>,
    _auth: AuthedUser,
    Path(hunt_id): Path<Uuid>,
) -> ApiResult<Json<HuntLiveOpsResp>> {
    let live_ops = load_state(&state, hunt_id).await?;
    Ok(Json(to_resp(&live_ops)))
}

pub async fn update_step_live_ops(
    State(state): State<AppState>,
    OwnedHunt(hunt): OwnedHunt,
    Path((hunt_id, step_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateStepLiveOpsReq>,
) -> ApiResult<Json<HuntLiveOpsResp>> {
    if hunt_id != hunt.id {
        return Err(ApiError::bad_request("hunt id mismatch"));
    }

    let step = query_get!(&state.pool, HuntStep, "hunt_steps", "id", step_id)
        .ok_or_else(|| ApiError::not_found("step not found"))?;
    if step.hunt_id != hunt_id {
        return Err(ApiError::bad_request("step does not belong to this hunt"));
    }

    let mut live_ops = load_state(&state, hunt_id).await?;
    let key = step_id.to_string();
    let mut current = live_ops.steps.remove(&key).unwrap_or_default();

    if let Some(paused) = req.paused {
        current.paused = Some(paused);
    }
    if req.clear_redirect == Some(true) {
        current.redirect = None;
    }
    if let Some(redirect) = req.redirect {
        current.redirect = Some(redirect);
    }

    if current.paused.is_none() && current.redirect.is_none() {
        live_ops.steps.remove(&key);
    } else {
        live_ops.steps.insert(key.clone(), current.clone());
    }

    save_state(&state, &live_ops).await?;
    publish_live_ops_updated(&state, hunt_id, step_id, &current);

    Ok(Json(to_resp(&live_ops)))
}

pub async fn clear_step_live_ops(
    State(state): State<AppState>,
    OwnedHunt(hunt): OwnedHunt,
    Path((hunt_id, step_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<impl IntoResponse> {
    if hunt_id != hunt.id {
        return Err(ApiError::bad_request("hunt id mismatch"));
    }

    let mut live_ops = load_state(&state, hunt_id).await?;
    live_ops.steps.remove(&step_id.to_string());
    save_state(&state, &live_ops).await?;
    publish_live_ops_updated(
        &state,
        hunt_id,
        step_id,
        &StepLiveOpsOverride::default(),
    );

    Ok(StatusCode::NO_CONTENT)
}

pub async fn load_step_override(
    state: &AppState,
    hunt_id: Uuid,
    step_id: Uuid,
) -> ApiResult<Option<StepLiveOpsOverride>> {
    let live_ops = load_state(state, hunt_id).await?;
    Ok(live_ops.steps.get(&step_id.to_string()).cloned())
}
