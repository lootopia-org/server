use chrono::{DateTime, Utc};
use sqlx::prelude::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct Hunt {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub image: Option<String>,
    pub partner_id: Uuid,
    pub difficulty: Option<String>,
    pub estimated_duration: i32,
    pub status: Option<String>,
    pub rating: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct HuntStep {
    pub id: Uuid,
    pub hunt_id: Uuid,
    pub step_order: i32,
    pub title: String,
    pub description: Option<String>,
    pub r#type: Option<String>,
    pub latitude: Option<String>,
    pub longitude: Option<String>,
    pub points: Option<f32>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct HuntParticipant {
    pub id: Uuid,
    pub user_id: Uuid,
    pub hunt_id: Uuid,
    pub points_awarded: i32,
    pub joined_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}
