use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use uuid::Uuid;

use crate::{
    api::{
        hunt_steps::{
            dto::{CompleteStepReq, HuntStepResp, UpdateHuntStep},
            models::{HuntStep, HuntStepCompletion},
        },
        middleware::ownership::OwnedHuntStep,
    },
    auth::session::AuthedUser,
    error::{ApiError, ApiResult},
    event::{event::Event, event_types, topics},
    query_delete, query_get, query_list, query_scale, query_update,
    utils::contants::NOW,
    AppState,
};

pub async fn get_step(
    State(state): State<AppState>,
    _auth: AuthedUser,
    Path(step_id): Path<Uuid>,
) -> ApiResult<Json<HuntStepResp>> {
    let step = query_get!(&state.pool, HuntStep, "hunt_steps", "id", step_id)
        .ok_or_else(|| ApiError::not_found("step not found"))?;
    Ok(Json(HuntStepResp::from(step)))
}

pub async fn complete_step(
    State(state): State<AppState>,
    auth: AuthedUser,
    Path(id): Path<Uuid>,
    Json(req): Json<CompleteStepReq>,
) -> ApiResult<Json<HuntStepResp>> {
    let step = query_get!(&state.pool, HuntStep, "hunt_steps", "id", id)
        .ok_or_else(|| ApiError::not_found("step not found"))?;

    let joined: bool = query_scale!(
        &state.pool,
        "SELECT EXISTS(SELECT 1 FROM hunt_participants WHERE hunt_id = $1 AND user_id = $2)",
        step.hunt_id,
        auth.user.id
    );
    if !joined {
        return Err(ApiError::forbidden("you have not joined this hunt"));
    }

    let already_done: bool = query_scale!(
        &state.pool,
        "SELECT EXISTS(SELECT 1 FROM hunt_step_completions WHERE user_id = $1 AND step_id = $2)",
        auth.user.id,
        id
    );
    if already_done {
        return Err(ApiError::conflict("step already completed"));
    }

    match step.r#type.as_deref() {
        Some("checkpoint") => {
            if req.latitude != step.latitude || req.longitude != step.longitude {
                return Err(ApiError::bad_request("not in the right location"));
            }
        }
        _ => {
            if req.answer != step.awnser {
                return Err(ApiError::bad_request("this was not the correct answer"));
            }
        }
    }

    query_update!(
        &state.pool,
        HuntStepCompletion,
        "hunt_step_completions",
        "user_id",
        auth.user.id,
        "completed_at" => Some(*NOW),
    );

    let resp = HuntStepResp::from(step);
    state.event_handler.publish(
        Event::new(
            event_types::HUNT_STEPS_COMPLETE,
            topics::HUNT_STEPS,
            serde_json::to_value(&resp).unwrap_or(serde_json::Value::Null),
        )
        .with_resource_id(id),
    );

    Ok(Json(resp))
}

pub async fn update_step(
    State(state): State<AppState>,
    OwnedHuntStep(_step): OwnedHuntStep,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateHuntStep>,
) -> ApiResult<Json<HuntStepResp>> {
    let step = query_update!(
        &state.pool,
        HuntStep,
        "hunt_steps",
        "id",
        id,
        "step_order"  => req.step_order,
        "title"       => req.title,
        "description" => req.description,
        "type"        => req.r#type,
        "latitude"    => req.latitude,
        "longitude"   => req.longitude,
        "awnser"      => req.awnser,
        "points"      => req.points,
    );

    let resp = HuntStepResp::from(step);
    state.event_handler.publish(
        Event::new(
            event_types::HUNT_STEPS_UPDATE,
            topics::HUNT_STEPS,
            serde_json::to_value(&resp).unwrap_or(serde_json::Value::Null),
        )
        .with_resource_id(id),
    );

    Ok(Json(resp))
}

pub async fn delete_step(
    State(state): State<AppState>,
    OwnedHuntStep(_step): OwnedHuntStep,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let rows = query_delete!(&state.pool, "hunt_steps", "id", id);
    if rows.rows_affected() == 0 {
        return Err(ApiError::not_found("step not found"));
    }

    state.event_handler.publish(
        Event::new(
            event_types::HUNT_STEPS_DELETE,
            topics::HUNT_STEPS,
            serde_json::json!({ "stepId": id }),
        )
        .with_resource_id(id),
    );

    Ok(StatusCode::NO_CONTENT)
}

pub async fn completed_steps(
    State(state): State<AppState>,
    auth: AuthedUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<HuntStepResp>>> {
    let steps: Vec<HuntStep> = query_list!(
        &state.pool,
        HuntStep,
        "hunt_steps",
        "id IN (SELECT step_id FROM hunt_step_completions WHERE user_id = $1 AND hunt_id = $2) ORDER BY step_order",
        auth.user.id,
        id
    );
    Ok(Json(steps.into_iter().map(HuntStepResp::from).collect()))
}
