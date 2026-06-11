use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use crate::{
    api::{
        hunts::hunt_steps::{
            dto::HuntStepResp,
            models::{HuntStep, HuntStepCompletion},
        },
        middleware::ownership::OwnedHunt,
    },
    auth::session::{AuthedAdminOrPartner, AuthedUser},
    error::{ApiError, ApiResult},
    event::{event::Event, event_types, topics},
    hunts::{
        dto::{CreateHunt, HuntDetail, HuntParticipantResp, HuntResp, JoinHunt, UpdateHunt},
        models::{Hunt, HuntParticipant},
    },
    query_create, query_delete, query_get, query_join, query_list, query_scale, query_update,
    utils::contants::NOW,
    AppState,
};

async fn hunt_steps(state: &AppState, hunt_id: Uuid) -> anyhow::Result<Vec<HuntStepResp>> {
    let steps = query_list!(
        &state.pool,
        HuntStep,
        "hunt_steps",
        "hunt_id = $1 ORDER BY step_order",
        hunt_id
    );
    Ok(steps.into_iter().map(HuntStepResp::from).collect())
}

pub async fn list_hunts(
    State(state): State<AppState>,
    _auth: AuthedUser,
) -> ApiResult<Json<Vec<HuntResp>>> {
    let hunts: Vec<Hunt> = query_list!(
        &state.pool,
        Hunt,
        "hunts",
        "status = 'active' ORDER BY created_at DESC"
    );
    Ok(Json(hunts.into_iter().map(HuntResp::from).collect()))
}

pub async fn get_hunt(
    State(state): State<AppState>,
    _auth: AuthedUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<HuntDetail>> {
    let hunt = query_get!(&state.pool, Hunt, "hunts", "id", id)
        .ok_or_else(|| ApiError::not_found("hunt not found"))?;
    let steps: Vec<HuntStepResp> = hunt_steps(&state, id).await?;
    Ok(Json(HuntDetail {
        hunt: hunt.into(),
        steps,
    }))
}

pub async fn create_hunt(
    State(state): State<AppState>,
    auth: AuthedAdminOrPartner,
    Json(req): Json<CreateHunt>,
) -> ApiResult<impl IntoResponse> {
    if req.steps.is_empty() {
        return Err(ApiError::bad_request("hunt must have at least one step"));
    }
    let mut tx = state.pool.begin().await?;

    let hunt = query_create!(&mut *tx, Hunt, "hunts",
        "title"              => req.title.clone(),
        "description"        => req.description.clone(),
        "image"                => req.image.clone(),
        "partner_id"           => auth.user.id,
        "difficulty"           => req.difficulty.clone(),
        "estimated_duration"   => req.estimated_duration,
        "status"               => req.status.clone().unwrap_or_else(|| "draft".to_string()),
        "created_at"           => *NOW,
        "updated_at"           => *NOW
    );

    let hunt_id = hunt.id;
    let mut steps = Vec::with_capacity(req.steps.len());
    for step in &req.steps {
        let created = query_create!(&mut *tx, HuntStep, "hunt_steps",
            "hunt_id"     => hunt_id,
            "step_order"  => step.step_order,
            "title"       => step.title.clone(),
            "description" => step.description.clone(),
            "type"        => step.r#type.clone(),
            "latitude"    => step.latitude.clone(),
            "longitude"   => step.longitude.clone(),
            "points"      => step.points,
            "created_at"  => *NOW
        );
        query_create!(&mut *tx, HuntStepCompletion, "hunt_step_completions", 
            "hunt_id"=> hunt_id, 
            "user_id" => auth.user.id, 
            "step_id"=>created.id);
        steps.push(created);
    }

    tx.commit().await?;

    let detail = HuntDetail {
        hunt: hunt.clone().into(),
        steps: steps.into_iter().map(HuntStepResp::from).collect(),
    };

    state.event_handler.publish(Event::new(
        event_types::HUNTS_CREATED,
        topics::HUNTS,
        serde_json::to_value(&detail).unwrap_or(serde_json::Value::Null),
    ));

    Ok((StatusCode::CREATED, Json(detail)))
}

pub async fn update_hunt(
    State(state): State<AppState>,
    OwnedHunt(_hunt): OwnedHunt,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateHunt>,
) -> ApiResult<impl IntoResponse> {
    let hunt = query_update!(
        &state.pool,
        Hunt,
        "hunts",
        "id",
        id,
        "title"              => req.title,
        "description"        => req.description,
        "image"              => req.image,
        "difficulty"         => req.difficulty,
        "estimated_duration" => req.estimated_duration,
        "latitude"           => req.latitude,
        "longitude"          => req.longitude,
        "status"             => req.status,
        "rating"             => req.rating,
        "updated_at"         => Some(*NOW)
    );

    let resp = HuntResp::from(hunt);
    state.event_handler.publish(
        Event::new(
            event_types::HUNTS_UPDATED,
            topics::HUNTS,
            serde_json::to_value(&resp).unwrap_or(serde_json::Value::Null),
        )
        .with_resource_id(id),
    );

    Ok((StatusCode::ACCEPTED, Json(resp)))
}

pub async fn delete_hunt(
    State(state): State<AppState>,
    OwnedHunt(_hunt): OwnedHunt,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let rows = query_delete!(&state.pool, "hunts", "id", id);
    if rows.rows_affected() == 0 {
        return Err(ApiError::not_found("hunt not found"));
    }

    state.event_handler.publish(
        Event::new(
            event_types::HUNTS_DELETED,
            topics::HUNTS,
            serde_json::json!({ "huntId": id }),
        )
        .with_resource_id(id),
    );

    Ok(StatusCode::NO_CONTENT)
}

pub async fn join_hunt(
    State(state): State<AppState>,
    auth: AuthedUser,
    Json(req): Json<JoinHunt>,
) -> ApiResult<StatusCode> {
    let hunt_valid: bool = query_scale!(
        &state.pool,
        "SELECT EXISTS(SELECT 1 FROM hunts WHERE id = $1 AND status = 'active')",
        req.hunt_id.clone()
    );
    if !hunt_valid {
        return Err(ApiError::not_found("hunt not found or not available"));
    }

    let already_joined: bool = query_scale!(
        &state.pool,
        "SELECT EXISTS(SELECT 1 FROM hunt_participants WHERE hunt_id = $1 AND user_id = $2)",
        req.hunt_id.clone(),
        auth.user.id
    );
    if already_joined {
        return Err(ApiError::conflict("already joined this hunt"));
    }

    query_create!(&state.pool, HuntParticipant, "hunt_participants",
        "user_id"   => auth.user.id,
        "hunt_id"   => &req.hunt_id,
        "joined_at" => *NOW
    );

    state.event_handler.publish(
        Event::new(
            event_types::HUNTS_JOINED,
            topics::HUNTS,
            serde_json::json!({
                "huntId": &req.hunt_id,
                "userId": auth.user.id,
                "joinedAt": *NOW,
            }),
        )
        .with_resource_id(req.hunt_id),
    );

    Ok(StatusCode::NO_CONTENT)
}

pub async fn leave_hunt(
    State(state): State<AppState>,
    auth: AuthedUser,
    Json(req): Json<JoinHunt>,
) -> ApiResult<StatusCode> {
    let hunt_valid: bool = query_scale!(
        &state.pool,
        "SELECT EXISTS(SELECT 1 FROM hunts WHERE id = $1 AND status = 'active')",
        req.hunt_id.clone()
    );
    if !hunt_valid {
        return Err(ApiError::not_found("hunt not found or not available"));
    }

    let joined: bool = query_scale!(
        &state.pool,
        "SELECT EXISTS(SELECT 1 FROM hunt_participants WHERE hunt_id = $1 AND user_id = $2)",
        req.hunt_id.clone(),
        auth.user.id
    );
    if !joined {
        return Err(ApiError::conflict("have not joined this hunt"));
    }

    let rows = query_delete!(&state.pool, "hunt_participants", "user_id", auth.user.id);
    if rows.rows_affected() == 0 {
        return Err(ApiError::not_found("hunt not found"));
    }

    state.event_handler.publish(
        Event::new(
            event_types::HUNTS_LEAVE,
            topics::HUNTS,
            serde_json::json!({
                "huntId": &req.hunt_id,
                "userId": auth.user.id,
                "leftAt": *NOW,
            }),
        )
        .with_resource_id(req.hunt_id),
    );

    Ok(StatusCode::NO_CONTENT)
}

pub async fn hunts_in_progrss(
    State(state): State<AppState>,
    auth: AuthedUser,
) -> ApiResult<Json<Vec<HuntResp>>> {
    let hunts: Vec<Hunt> = query_list!(
        &state.pool,
        Hunt,
        "hunts",
        "id IN (SELECT hunt_id FROM hunt_participants WHERE user_id = $1 AND completed_at IS NULL)",
        auth.user.id
    );
    Ok(Json(hunts.into_iter().map(HuntResp::from).collect()))
}

pub async fn get_hunt_participants(
    State(state): State<AppState>,
    OwnedHunt(hunt): OwnedHunt,
) -> ApiResult<Json<Vec<HuntParticipantResp>>> {
    let participants: Vec<HuntParticipantResp> = query_join!(
        &state.pool,
        HuntParticipantResp,
        r#"
        SELECT
            u.id                    AS user_id,
            u.email                 AS email,
            up.points               AS points,
            up.level                AS level,
            up.completed_hunts      AS completed_hunts,
            hp.points_awarded       AS points_awarded,
            hp.joined_at            AS joined_at,
            hp.completed_at         AS completed_at
        FROM hunt_participants hp
        JOIN users u               ON u.id = hp.user_id
        LEFT JOIN user_profiles up ON up.user_id = hp.user_id
        WHERE hp.hunt_id = $1
        ORDER BY hp.joined_at DESC
        "#,
        hunt.id
    );
    Ok(Json(participants))
}
