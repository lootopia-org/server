use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProximityNotification {
    pub user_id: Uuid,
    pub hunt_id: Uuid,
    pub step_id: Uuid,
    pub step_title: String,
    pub distance_meters: f64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HuntPausedNotification {
    pub user_id: Uuid,
    pub hunt_id: Uuid,
    pub hunt_title: String,
    pub message: String,
}
