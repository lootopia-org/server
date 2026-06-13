use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    auth::session::{AuthedAdmin, AuthedUser},
    error::{ApiError, ApiResult},
    event::{event::Event, event_types, topics},
    profiles::{
        dto::{AdminProfileResp, AdminUpdateProfile, Profile, UpdateProfile},
        models::UserProfiles,
    },
    query_create, query_delete, query_get, query_join, query_scale, query_update,
    utils::contants::NOW,
    AppState,
};

pub async fn list_profiles(
    State(state): State<AppState>,
    _auth: AuthedAdmin,
) -> ApiResult<Json<Vec<AdminProfileResp>>> {
    let profiles: Vec<AdminProfileResp> = query_join!(
        &state.pool,
        AdminProfileResp,
        r#"
        SELECT
            up.id,
            up.user_id,
            u.username,
            u.email,
            up.points,
            up.level,
            up.completed_hunts,
            up.updated_at
        FROM user_profiles up
        JOIN users u ON u.id = up.user_id
        ORDER BY up.updated_at DESC
        "#,
    );
    Ok(Json(profiles))
}

pub async fn admin_update_profile(
    State(state): State<AppState>,
    _auth: AuthedAdmin,
    Path(user_id): Path<Uuid>,
    Json(req): Json<AdminUpdateProfile>,
) -> ApiResult<Json<AdminProfileResp>> {
    let exists: bool = query_scale!(
        &state.pool,
        "SELECT EXISTS(SELECT 1 FROM user_profiles WHERE user_id = $1)",
        user_id
    );
    if !exists {
        return Err(ApiError::not_found("profile not found"));
    }

    let profile = query_update!(
        &state.pool,
        UserProfiles,
        "user_profiles",
        "user_id",
        user_id,
        "points"          => req.points,
        "level"           => req.level,
        "completed_hunts" => req.completed_hunts,
        "updated_at"      => Some(*NOW)
    );

    let resp: AdminProfileResp = query_join!(
        &state.pool,
        AdminProfileResp,
        r#"
        SELECT
            up.id,
            up.user_id,
            u.username,
            u.email,
            up.points,
            up.level,
            up.completed_hunts,
            up.updated_at
        FROM user_profiles up
        JOIN users u ON u.id = up.user_id
        WHERE up.user_id = $1
        "#,
        user_id
    )
    .into_iter()
    .next()
    .ok_or_else(|| ApiError::not_found("profile not found"))?;

    state.event_handler.publish(Event::new(
        event_types::PROFILE_UPDATED,
        topics::PROFILE,
        serde_json::to_value(&Profile::from(profile)).unwrap_or(serde_json::Value::Null),
    ));

    Ok(Json(resp))
}

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

pub async fn update_profile(
    State(state): State<AppState>,
    auth: AuthedUser,
    Json(req): Json<UpdateProfile>,
) -> ApiResult<impl IntoResponse> {
    let hunt_valid: bool = query_scale!(
        &state.pool,
        "SELECT EXISTS(SELECT 1 FROM hunts WHERE id = $1 AND status = 'active')",
        &req.hunt_id
    );
    if !hunt_valid {
        return Err(ApiError::not_found("hunt not found or not available"));
    }

    let hunt_points: i32 = query_scale!(
        &state.pool,
        "SELECT COALESCE(SUM(points), 0)::int FROM hunt_steps WHERE hunt_id = $1",
        &req.hunt_id
    );
    if hunt_points == 0 {
        return Err(ApiError::not_found("hunt not found or has no steps"));
    }

    let hunt_belongs: bool = query_scale!(
        &state.pool,
        "SELECT EXISTS(SELECT 1 FROM hunt_participants WHERE hunt_id = $1 AND user_id = $2)",
        &req.hunt_id,
        auth.user.id
    );
    if !hunt_belongs {
        return Err(ApiError::bad_request("hunt does not belong to this user"));
    }

    let mut tx = state.pool.begin().await?;

    let completed_at: Option<DateTime<Utc>> = query_scale!(
        &mut *tx,
        "SELECT completed_at FROM hunt_participants WHERE user_id=$1 AND hunt_id=$2",
        auth.user.id,
        &req.hunt_id
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

    query_create!(&mut *tx, "hunt_participants",
        "user_id" => auth.user.id,
        "hunt_id" => &req.hunt_id,
        "points_awarded" => hunt_points,
        "completed_at" => *NOW
    );

    tx.commit().await?;

    let resp = Profile::from(profile);
    state.event_handler.publish(Event::new(
        event_types::PROFILE_UPDATED,
        topics::PROFILE,
        serde_json::to_value(&resp).unwrap_or(serde_json::Value::Null),
    ));

    Ok((StatusCode::ACCEPTED, Json(resp)))
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
