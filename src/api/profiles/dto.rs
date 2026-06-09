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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProfile {
    pub hunt_id: Uuid,
}

impl_from!(UserProfiles => Profile {
    points,
    level,
    completed_hunts
});
