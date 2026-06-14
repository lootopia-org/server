use axum::{
    routing::{delete, get, patch},
    Router,
};

use crate::{
    api::hunts::live_ops::handlers::{clear_step_live_ops, get_hunt_live_ops, update_step_live_ops},
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{id}/live-ops", get(get_hunt_live_ops))
        .route("/{id}/live-ops/{stepId}", patch(update_step_live_ops))
        .route("/{id}/live-ops/{stepId}", delete(clear_step_live_ops))
}
