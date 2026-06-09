use chrono::{DateTime, Utc};
use sqlx::prelude::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct UserProfiles {
    pub id: Uuid,
    pub user_id: Uuid,
    pub points: i32,
    pub level: f32,
    pub completed_hunts: i32,
    pub updated_at: DateTime<Utc>,
}
