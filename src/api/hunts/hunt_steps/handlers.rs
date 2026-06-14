use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::collections::HashSet;
use uuid::Uuid;

use crate::{
    api::{
        hunts::{
            hunt_steps::{
                dto::{CompleteStepReq, CreateStepReq, HuntStepResp, SyncHuntStepsReq, UpdateHuntStep},
                models::{HuntStep, HuntStepCompletion},
            },
            live_ops::handlers::load_step_override,
        },
        middleware::{
            caching::{invalidate_user_profile_cache, invalidate_hunt_step_caches},
            ownership::{OwnedHunt, OwnedHuntStep},
        },
        profiles::{
            dto::Profile,
            service::{award_step_points, maybe_mark_hunt_completed},
        },
    },
    auth::session::{AuthedAdminOrPartner, AuthedUser},
    error::{ApiError, ApiResult},
    event::{event::Event, event_types, topics},
    hunts::{
        dto::HuntDetail,
        models::Hunt,
    },
    query_create, query_delete, query_get, query_list, query_scale, query_update,
    utils::{
        contants::{NOW, PROXIMITY_THRESHOLD_METERS},
        geo::within_proximity,
        image_data::compare_step_photos,
    },
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

pub async fn create_step(
    State(state): State<AppState>,
    auth: AuthedAdminOrPartner,
    Json(req): Json<CreateStepReq>,
) -> ApiResult<impl IntoResponse> {
    let hunt = query_get!(&state.pool, Hunt, "hunts", "id", req.hunt_id)
        .ok_or_else(|| ApiError::not_found("hunt not found"))?;

    if auth.user.role != "admin" && hunt.partner_id != auth.user.id {
        return Err(ApiError::forbidden("you do not have access to this hunt"));
    }

    let step = query_create!(&state.pool, HuntStep, "hunt_steps",
        "hunt_id"     => req.hunt_id,
        "step_order"  => req.step_order,
        "title"       => req.title.clone(),
        "description" => req.description.clone(),
        "type"        => req.r#type.clone(),
        "latitude"    => req.latitude.clone(),
        "longitude"   => req.longitude.clone(),
        "awnser"      => req.awnser.clone(),
        "points"      => req.points,
        "created_at"  => *NOW
    );

    let resp = HuntStepResp::from(step);
    state.event_handler.publish(
        Event::new(
            event_types::HUNT_STEPS_UPDATE,
            topics::HUNT_STEPS,
            serde_json::to_value(&resp).unwrap_or(serde_json::Value::Null),
        )
        .with_resource_id(resp.id),
    );

    invalidate_hunt_step_caches(&state, req.hunt_id).await;

    Ok((StatusCode::CREATED, Json(resp)))
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

    let hunt_active: bool = query_scale!(
        &state.pool,
        "SELECT EXISTS(SELECT 1 FROM hunts WHERE id = $1 AND status = 'active')",
        step.hunt_id
    );
    if !hunt_active {
        return Err(ApiError::bad_request("hunt is not active"));
    }

    let live_override = load_step_override(&state, step.hunt_id, id).await?;
    if live_override.as_ref().and_then(|value| value.paused) == Some(true) {
        return Err(ApiError::bad_request("step is paused by organizer"));
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

    if let (Some(step_lat), Some(step_lng)) = (step.latitude.as_deref(), step.longitude.as_deref()) {
        let (check_lat, check_lng) = if let Some(redirect) = live_override.as_ref().and_then(|value| value.redirect.as_ref()) {
            (redirect.latitude.to_string(), redirect.longitude.to_string())
        } else {
            (step_lat.to_string(), step_lng.to_string())
        };

        let (Some(user_lat), Some(user_lng)) =
            (auth.user.latitude.as_deref(), auth.user.longitude.as_deref())
        else {
            return Err(ApiError::bad_request(
                "location not available; send your location via websocket first",
            ));
        };

        if !within_proximity(&user_lat, &user_lng, &check_lat, &check_lng, PROXIMITY_THRESHOLD_METERS) {
            return Err(ApiError::bad_request("not in the right location"));
        }
    }

    match step.r#type.as_deref() {
        Some("photo") => {
            let submitted = req
                .answer
                .as_ref()
                .ok_or_else(|| ApiError::bad_request("missing photo data"))?;
            let reference = step
                .awnser
                .as_ref()
                .ok_or_else(|| ApiError::bad_request("missing reference photo"))?;

            let matches = compare_step_photos(reference, submitted, &state.s3, 10)
                .await
                .map_err(|err| ApiError::bad_request(format!("invalid photo data: {err}")))?;

            if !matches {
                return Err(ApiError::bad_request("photo does not match"));
            }
        }
        _ => {
            if req.answer != step.awnser {
                return Err(ApiError::bad_request("this was not the correct answer"));
            }
        }
    }

    let hunt_id = step.hunt_id;
    let step_points = step.points.unwrap_or(0).max(0);

    let mut tx = state.pool.begin().await?;

    query_create!(
        &mut *tx,
        HuntStepCompletion,
        "hunt_step_completions",
        "hunt_id" => hunt_id,
        "user_id" => auth.user.id,
        "step_id" => id,
        "completed_at" => *NOW
    );

    let mut profile = award_step_points(&mut tx, auth.user.id, hunt_id, step_points).await?;
    profile = maybe_mark_hunt_completed(&mut tx, auth.user.id, hunt_id, profile).await?;

    tx.commit().await?;

    let resp = HuntStepResp::from(step);
    state.event_handler.publish(
        Event::new(
            event_types::HUNT_STEPS_COMPLETE,
            topics::HUNT_STEPS,
            serde_json::to_value(&resp).unwrap_or(serde_json::Value::Null),
        )
        .with_resource_id(id),
    );

    invalidate_user_profile_cache(&state, auth.user.id).await;
    state.event_handler.publish(Event::new(
        event_types::PROFILE_UPDATED,
        topics::PROFILE,
        serde_json::to_value(Profile::from(profile)).unwrap_or(serde_json::Value::Null),
    ));

    invalidate_hunt_step_caches(&state, hunt_id).await;

    Ok(Json(resp))
}

pub async fn update_step(
    State(state): State<AppState>,
    OwnedHuntStep(existing): OwnedHuntStep,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateHuntStep>,
) -> ApiResult<Json<HuntStepResp>> {
    let hunt_id = existing.hunt_id;
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

    invalidate_hunt_step_caches(&state, hunt_id).await;

    Ok(Json(resp))
}

pub async fn delete_step(
    State(state): State<AppState>,
    OwnedHuntStep(step): OwnedHuntStep,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let hunt_id = step.hunt_id;
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

    invalidate_hunt_step_caches(&state, hunt_id).await;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn sync_hunt_steps(
    State(state): State<AppState>,
    OwnedHunt(hunt): OwnedHunt,
    Json(req): Json<SyncHuntStepsReq>,
) -> ApiResult<Json<HuntDetail>> {
    if req.steps.is_empty() {
        return Err(ApiError::bad_request("hunt must have at least one step"));
    }

    let hunt_id = hunt.id;
    let mut tx = state.pool.begin().await?;

    let existing: Vec<HuntStep> = query_list!(
        &mut *tx,
        HuntStep,
        "hunt_steps",
        "hunt_id = $1",
        hunt_id
    );
    let existing_ids: HashSet<Uuid> = existing.iter().map(|step| step.id).collect();
    let keep_ids: HashSet<Uuid> = req.steps.iter().filter_map(|step| step.id).collect();

    for step in existing {
        if !keep_ids.contains(&step.id) {
            query_delete!(&mut *tx, "hunt_steps", "id", step.id);
        }
    }

    for (index, item) in req.steps.iter().enumerate() {
        if let Some(id) = item.id {
            if !existing_ids.contains(&id) {
                return Err(ApiError::bad_request("step does not belong to this hunt"));
            }
            query_update!(
                &mut *tx,
                HuntStep,
                "hunt_steps",
                "id",
                id,
                "step_order" => Some(10_000_i32 + index as i32),
            );
        }
    }

    for (index, item) in req.steps.iter().enumerate() {
        let step_order = (index + 1) as i32;
        if let Some(id) = item.id {
            query_update!(
                &mut *tx,
                HuntStep,
                "hunt_steps",
                "id",
                id,
                "step_order"  => Some(step_order),
                "title"       => Some(item.title.clone()),
                "description" => Some(item.description.clone()),
                "type"        => Some(item.r#type.clone()),
                "latitude"    => Some(item.latitude.clone()),
                "longitude"   => Some(item.longitude.clone()),
                "awnser"      => Some(item.awnser.clone()),
                "points"      => Some(item.points),
            );
        } else {
            query_create!(
                &mut *tx,
                HuntStep,
                "hunt_steps",
                "hunt_id"     => hunt_id,
                "step_order"  => step_order,
                "title"       => item.title.clone(),
                "description" => item.description.clone(),
                "type"        => item.r#type.clone(),
                "latitude"    => item.latitude.clone(),
                "longitude"   => item.longitude.clone(),
                "awnser"      => item.awnser.clone(),
                "points"      => item.points,
                "created_at"  => *NOW
            );
        }
    }

    tx.commit().await?;

    let steps: Vec<HuntStep> = query_list!(
        &state.pool,
        HuntStep,
        "hunt_steps",
        "hunt_id = $1 ORDER BY step_order",
        hunt_id
    );

    let detail = HuntDetail {
        hunt: hunt.into(),
        steps: steps.into_iter().map(HuntStepResp::from).collect(),
    };

    state.event_handler.publish(
        Event::new(
            event_types::HUNT_STEPS_UPDATE,
            topics::HUNT_STEPS,
            serde_json::json!({ "huntId": hunt_id, "synced": true }),
        )
        .with_resource_id(hunt_id),
    );

    invalidate_hunt_step_caches(&state, hunt_id).await;

    Ok(Json(detail))
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
