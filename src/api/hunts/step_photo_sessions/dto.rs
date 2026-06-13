use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateStepPhotoSessionReq {
    pub step_key: String,
    pub hunt_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitStepPhotoReq {
    pub photo_url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StepPhotoSessionResp {
    pub session_id: Uuid,
    pub step_key: String,
    pub hunt_id: Option<Uuid>,
    pub photo_url: Option<String>,
    pub status: String,
}
