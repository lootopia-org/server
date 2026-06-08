use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct MessageResp {
    message: String,
}

pub fn message(text: impl Into<String>) -> Json<MessageResp> {
    Json(MessageResp {
        message: text.into(),
    })
}
