use axum::{
    routing::{get, post},
    Router,
};

use crate::{
    api::hunts::step_photo_sessions::handlers::{
        create_step_photo_session, get_step_photo_session, submit_step_photo,
    },
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_step_photo_session))
        .route("/{id}", get(get_step_photo_session))
        .route("/{id}/photo", post(submit_step_photo))
}
