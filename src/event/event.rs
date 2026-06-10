use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::define_topics;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: Uuid,
    pub event_type: String,
    pub topic: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<Uuid>,
    pub payload: Value,
    pub timestamp: DateTime<Utc>,
}

impl Event {
    pub fn new(event_type: impl Into<String>, topic: impl Into<String>, payload: Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type: event_type.into(),
            topic: topic.into(),
            resource_id: None,
            payload,
            timestamp: Utc::now(),
        }
    }

    pub fn with_resource_id(mut self, resource_id: Uuid) -> Self {
        self.resource_id = Some(resource_id);
        self
    }

    pub fn redis_channels(&self) -> Vec<String> {
        let mut channels = vec![global_channel(), topic_channel(&self.topic)];
        if let Some(id) = self.resource_id {
            channels.push(topic_channel(&format!("{}.{}", self.topic, id)));
        }
        channels
    }
}

pub fn global_channel() -> String {
    "events".to_string()
}

pub fn topic_channel(topic: &str) -> String {
    format!("events:topic:{topic}")
}

define_topics! {
    HUNTS:    "hunts"    => [created, updated, deleted, joined, leave],
    HUNT_STEPS:    "hunt_steps"    => [complete, update, delete],
    PROFILE:  "profiles" => [updated]
}
