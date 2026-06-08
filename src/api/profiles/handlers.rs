use axum::{extract::State, http::StatusCode, Json};
use chrono::{DateTime, Utc};

use crate::{
    auth::session::AuthedUser,
    error::{ApiError, ApiResult},
    profiles::{
        dto::{Profile, UpdateProfile},
        models::UserProfiles,
    },
    query_create, query_delete, query_get, query_scale, query_update,
    utils::contants::NOW,
    AppState,
};

pub async fn get_profile(
    State(state): State<AppState>,
    auth: AuthedUser,
) -> ApiResult<Json<Profile>> {
    let profile = query_get!(
        &state.pool,
        UserProfiles,
        "user_profiles",
        "user_id",
        auth.user.id
    )
    .ok_or_else(|| ApiError::not_found("resource from request not_ found"))?;

    Ok(Json(profile.into()))
}

pub async fn create_profile(
    State(state): State<AppState>,
    auth: AuthedUser,
) -> ApiResult<Json<Profile>> {
    let existing = query_get!(
        &state.pool,
        UserProfiles,
        "user_profiles",
        "user_id",
        auth.user.id
    );
    if existing.is_some() {
        return Err(ApiError::conflict("profile already exists"));
    }

    let profile = query_create!(&state.pool, UserProfiles, "user_profiles",
        "user_id" => auth.user.id,
        "points" => 0,
        "level" => 1.0,
        "completed_hunts" => 0,
        "updated_at" => *NOW
    );

    Ok(Json(profile.into()))
}

pub async fn update_profile(
    State(state): State<AppState>,
    auth: AuthedUser,
    Json(req): Json<UpdateProfile>,
) -> ApiResult<Json<Profile>> {
    let hunt_valid: bool = query_scale!(
        &state.pool,
        "SELECT EXISTS(SELECT 1 FROM hunts WHERE id = $1 AND status = 'active' AND launch_mode = 'live')",
        req.hunt_id.clone()
    );
    if !hunt_valid {
        return Err(ApiError::not_found("hunt not found or not available"));
    }

    let hunt_points: i32 = query_scale!(
        &state.pool,
        "SELECT COALESCE(SUM(points), 0)::int FROM hunt_steps WHERE hunt_id = $1",
        req.hunt_id.clone()
    );
    if hunt_points == 0 {
        return Err(ApiError::not_found("hunt not found or has no steps"));
    }

    let hunt_belongs: bool = query_scale!(
        &state.pool,
        "SELECT EXISTS(SELECT 1 FROM hunts_participants WHERE hunt_id = $1 AND user_id = $2)",
        req.hunt_id.clone(),
        auth.user.id
    );
    if !hunt_belongs {
        return Err(ApiError::forbidden("hunt does not belong to this user"));
    }

    let mut tx = state.pool.begin().await?;

    let completed_at: Option<DateTime<Utc>> = query_scale!(
        &mut *tx,
        "SELECT completed_at FROM hunts_participants WHERE user_id=$1 AND hunt_id=$2",
        auth.user.id,
        req.hunt_id.clone()
    );
    if completed_at.is_some() {
        tx.rollback().await?;
        return Err(ApiError::conflict("hunt already completed"));
    }

    let current = query_get!(
        &mut *tx,
        UserProfiles,
        "user_profiles",
        "user_id",
        auth.user.id
    )
    .ok_or_else(|| ApiError::not_found("User profile not found"))?;

    let new_points = current.points + hunt_points;
    let new_level = new_points as f32 / 100.0;
    let new_hunts = current.completed_hunts + 1;

    let profile = query_update!(
        &mut *tx,
        UserProfiles,
        "user_profiles",
        "user_id",
        auth.user.id,
        "points"          => Some(new_points),
        "level"           => Some(new_level),
        "completed_hunts" => Some(new_hunts),
        "updated_at"      => Some(*NOW)
    );

    query_create!(&mut *tx, "completed_hunts",
        "user_id" => auth.user.id,
        "hunt_id" => req.hunt_id.clone(),
        "points_awarded" => hunt_points,
        "completed_at" => *NOW
    );

    tx.commit().await?;

    Ok(Json(profile.into()))
}

pub async fn delete_profile(
    State(state): State<AppState>,
    auth: AuthedUser,
) -> ApiResult<StatusCode> {
    let rows = query_delete!(&state.pool, "user_profiles", "user_id", auth.user.id);
    if rows.rows_affected() == 0 {
        return Err(ApiError::not_found("profile not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}
