use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{api::hunts::hunt_steps::dto::HuntStepResp, hunts::models::Hunt, impl_from};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HuntResp {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub image: Option<String>,
    pub partner_id: Uuid,
    pub difficulty: Option<String>,
    pub estimated_duration: i32,
    pub status: Option<String>,
    pub rating: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HuntDetail {
    #[serde(flatten)]
    pub hunt: HuntResp,
    pub steps: Vec<HuntStepResp>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateHunt {
    pub title: String,
    pub description: Option<String>,
    pub image: Option<String>,
    pub partner_id: String,
    pub difficulty: Option<String>,
    pub estimated_duration: i32,
    pub status: Option<String>,
    pub steps: Vec<CreateHuntStep>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateHuntStep {
    pub step_order: i32,
    pub title: String,
    pub description: Option<String>,
    pub r#type: Option<String>,
    pub latitude: Option<String>,
    pub longitude: Option<String>,
    pub awnser: Option<String>,
    pub points: Option<f32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateHunt {
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub difficulty: Option<String>,
    pub estimated_duration: Option<i32>,
    pub latitude: Option<String>,
    pub longitude: Option<String>,
    pub status: Option<String>,
    pub rating: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinHunt {
    pub hunt_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct HuntParticipantResp {
    pub user_id: Uuid,
    pub email: String,
    pub points: Option<i32>,
    pub level: Option<f32>,
    pub completed_hunts: Option<i32>,
    pub points_awarded: i32,
    pub joined_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct StepAnalyticsResp {
    pub step_id: Uuid,
    pub step_order: i32,
    pub title: String,
    pub latitude: Option<String>,
    pub longitude: Option<String>,
    pub completion_count: i64,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct UserLocationResp {
    pub latitude: String,
    pub longitude: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HuntAnalyticsResp {
    pub hunt_id: Uuid,
    pub participant_count: i64,
    pub completed_hunt_count: i64,
    pub steps: Vec<StepAnalyticsResp>,
    pub user_locations: Vec<UserLocationResp>,
}

#[derive(Deserialize)]
pub struct HuntFilters {
    pub status: Option<String>,
    pub all: Option<bool>,
}

impl_from!(Hunt => HuntResp {
    id,
    title,
    description,
    image,
    partner_id,
    difficulty,
    estimated_duration,
    status,
    rating,
});
