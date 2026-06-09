use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use crate::{
    auth::session::{AuthedAdminOrPartner, AuthedUser},
    error::{ApiError, ApiResult},
    hunts::{
        dto::{CreateHunt, HuntDetail, HuntResp, HuntStepResp, JoinHunt, UpdateHunt},
        models::{Hunt, HuntParticipant, HuntStep},
    },
    query_create, query_delete, query_get, query_list, query_scale, query_update,
    utils::contants::NOW,
    AppState,
};

async fn hunt_steps(pool: &sqlx::PgPool, hunt_id: Uuid) -> Result<Vec<HuntStep>, ApiError> {
    let steps: Vec<HuntStep> = query_list!(
        pool,
        HuntStep,
        "hunt_steps",
        "hunt_id = $1 ORDER BY step_order",
        hunt_id
    );
    Ok(steps)
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

    let steps = hunt_steps(&state.pool, id).await?;

    Ok(Json(HuntDetail {
        hunt: hunt.into(),
        steps: steps.into_iter().map(HuntStepResp::from).collect(),
    }))
}

pub async fn create_hunt(
    State(state): State<AppState>,
    _auth: AuthedAdminOrPartner,
    Json(req): Json<CreateHunt>,
) -> ApiResult<impl IntoResponse> {
    if req.steps.is_empty() {
        return Err(ApiError::bad_request("hunt must have at least one step"));
    }

    let partner_id = Uuid::parse_str(&req.partner_id)
        .map_err(|_| ApiError::bad_request("invalid partner_id"))?;

    let mut tx = state.pool.begin().await?;

    let hunt = query_create!(&mut *tx, Hunt, "hunts",
        "title"              => req.title.clone(),
        "description"        => req.description.clone(),
        "image"                => req.image.clone(),
        "partner_id"           => partner_id,
        "difficulty"           => req.difficulty.clone(),
        "estimated_duration"   => req.estimated_duration,
        "status"               => req.status.clone().unwrap_or_else(|| "draft".to_string()),
        "created_at"           => *NOW,
        "updated_at"           => *NOW
    );

    let mut steps = Vec::with_capacity(req.steps.len());
    for step in &req.steps {
        let created = query_create!(&mut *tx, HuntStep, "hunt_steps",
            "hunt_id"     => hunt.id,
            "step_order"  => step.step_order,
            "title"       => step.title.clone(),
            "description" => step.description.clone(),
            "type"        => step.r#type.clone(),
            "latitude"    => step.latitude.clone(),
            "longitude"   => step.longitude.clone(),
            "points"      => step.points,
            "created_at"  => *NOW
        );
        steps.push(created);
    }

    tx.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(HuntDetail {
            hunt: hunt.into(),
            steps: steps.into_iter().map(HuntStepResp::from).collect(),
        }),
    ))
}

pub async fn update_hunt(
    State(state): State<AppState>,
    _auth: AuthedAdminOrPartner,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateHunt>,
) -> ApiResult<impl IntoResponse> {
    if query_get!(&state.pool, Hunt, "hunts", "id", id).is_none() {
        return Err(ApiError::not_found("hunt not found"));
    }

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

    Ok((StatusCode::ACCEPTED, Json(HuntResp::from(hunt))))
}

pub async fn delete_hunt(
    State(state): State<AppState>,
    _auth: AuthedAdminOrPartner,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let rows = query_delete!(&state.pool, "hunts", "id", id);
    if rows.rows_affected() == 0 {
        return Err(ApiError::not_found("hunt not found"));
    }
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

    Ok(StatusCode::NO_CONTENT)
}
