use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{api::hunts::hunt_steps::models::HuntStep, impl_from};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HuntStepResp {
    pub id: Uuid,
    pub step_order: i32,
    pub title: String,
    pub description: Option<String>,
    pub r#type: Option<String>,
    pub latitude: Option<String>,
    pub longitude: Option<String>,
    pub points: Option<i32>,
    pub awnser: Option<String>,
    pub scan_in_ar: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateStepReq {
    pub hunt_id: Uuid,
    pub step_order: i32,
    pub title: String,
    pub description: Option<String>,
    pub r#type: Option<String>,
    pub latitude: Option<String>,
    pub longitude: Option<String>,
    pub awnser: Option<String>,
    pub points: Option<i32>,
    pub scan_in_ar: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateHuntStep {
    pub step_order: Option<i32>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub r#type: Option<String>,
    pub latitude: Option<String>,
    pub longitude: Option<String>,
    pub awnser: Option<String>,
    pub points: Option<i32>,
    pub scan_in_ar: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncHuntStepItem {
    pub id: Option<Uuid>,
    pub step_order: i32,
    pub title: String,
    pub description: Option<String>,
    pub r#type: Option<String>,
    pub latitude: Option<String>,
    pub longitude: Option<String>,
    pub awnser: Option<String>,
    pub points: Option<i32>,
    pub scan_in_ar: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncHuntStepsReq {
    pub steps: Vec<SyncHuntStepItem>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteStepReq {
    pub answer: Option<String>,
}

impl_from!(HuntStep => HuntStepResp {
    id,
    step_order,
    title,
    description,
    r#type,
    latitude,
    longitude,
    points,
    awnser,
    scan_in_ar,
});
