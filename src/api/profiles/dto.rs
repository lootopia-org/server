use serde::{Deserialize, Serialize};

use crate::{impl_from, profiles::models::UserProfiles};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub points: i32,
    pub level: i32,
    pub completed_hunts: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProfile {
    pub hunt_id: String,
}

impl_from!(UserProfiles => Profile {
    points,
    level,
    completed_hunts
});
