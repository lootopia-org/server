use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{impl_from, profiles::models::UserProfiles};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub points: i32,
    pub level: f32,
    pub completed_hunts: i32,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AdminProfileResp {
    pub id: Uuid,
    pub user_id: Uuid,
    pub username: Option<String>,
    pub email: String,
    pub points: i32,
    pub level: f32,
    pub completed_hunts: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProfile {
    pub hunt_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminUpdateProfile {
    pub points: Option<i32>,
    pub level: Option<f32>,
    pub completed_hunts: Option<i32>,
}

impl_from!(UserProfiles => Profile {
    points,
    level,
    completed_hunts
});
