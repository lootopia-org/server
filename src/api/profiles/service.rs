use chrono::{DateTime, Utc};
use sqlx::Postgres;
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    profiles::models::UserProfiles,
    query_get, query_scale, query_update,
    utils::contants::NOW,
};

pub fn level_from_points(points: i32) -> f32 {
    points as f32 / 100.0
}

pub async fn award_step_points(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    user_id: Uuid,
    hunt_id: Uuid,
    step_points: i32,
) -> ApiResult<UserProfiles> {
    let current = query_get!(
        &mut **tx,
        UserProfiles,
        "user_profiles",
        "user_id",
        user_id
    )
    .ok_or_else(|| ApiError::not_found("User profile not found"))?;

    let new_points = current.points + step_points;
    let profile = query_update!(
        &mut **tx,
        UserProfiles,
        "user_profiles",
        "user_id",
        user_id,
        "points"          => Some(new_points),
        "level"           => Some(level_from_points(new_points)),
        "updated_at"      => Some(*NOW)
    );

    sqlx::query(
        "UPDATE hunt_participants
         SET points_awarded = points_awarded + $1
         WHERE user_id = $2 AND hunt_id = $3",
    )
    .bind(step_points)
    .bind(user_id)
    .bind(hunt_id)
    .execute(&mut **tx)
    .await?;

    Ok(profile)
}

pub async fn maybe_mark_hunt_completed(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    user_id: Uuid,
    hunt_id: Uuid,
    profile: UserProfiles,
) -> ApiResult<UserProfiles> {
    let total_steps: i64 = query_scale!(
        &mut **tx,
        "SELECT COUNT(*)::bigint FROM hunt_steps WHERE hunt_id = $1",
        hunt_id
    );
    let completed_steps: i64 = query_scale!(
        &mut **tx,
        "SELECT COUNT(*)::bigint FROM hunt_step_completions WHERE user_id = $1 AND hunt_id = $2",
        user_id,
        hunt_id
    );

    if total_steps == 0 || completed_steps < total_steps {
        return Ok(profile);
    }

    let completed_at: Option<DateTime<Utc>> = query_scale!(
        &mut **tx,
        "SELECT completed_at FROM hunt_participants WHERE user_id = $1 AND hunt_id = $2",
        user_id,
        hunt_id
    );

    if completed_at.is_some() {
        return Ok(profile);
    }

    sqlx::query(
        "UPDATE hunt_participants
         SET completed_at = $1
         WHERE user_id = $2 AND hunt_id = $3 AND completed_at IS NULL",
    )
    .bind(*NOW)
    .bind(user_id)
    .bind(hunt_id)
    .execute(&mut **tx)
    .await?;

    let updated = query_update!(
        &mut **tx,
        UserProfiles,
        "user_profiles",
        "user_id",
        user_id,
        "completed_hunts" => Some(profile.completed_hunts + 1),
        "updated_at"      => Some(*NOW)
    );

    Ok(updated)
}

pub async fn hunt_steps_all_completed(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    hunt_id: Uuid,
) -> ApiResult<bool> {
    let all_done: bool = query_scale!(
        pool,
        "SELECT (SELECT COUNT(*)::bigint FROM hunt_steps WHERE hunt_id = $1)
              = (SELECT COUNT(*)::bigint FROM hunt_step_completions WHERE hunt_id = $1 AND user_id = $2)",
        hunt_id,
        user_id
    );
    Ok(all_done)
}
