use chrono::{DateTime, Utc};
use sqlx::prelude::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct HuntStepCompletion {
    pub id: Uuid,
    pub hunt_id: Uuid,
    pub user_id: Uuid,
    pub step_id: Uuid,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
pub struct HuntStep {
    pub id: Uuid,
    pub hunt_id: Uuid,
    pub step_order: i32,
    pub title: String,
    pub awnser: Option<String>,
    pub description: Option<String>,
    pub r#type: Option<String>,
    pub latitude: Option<String>,
    pub longitude: Option<String>,
    pub points: Option<i32>,
    pub created_at: DateTime<Utc>,
}
