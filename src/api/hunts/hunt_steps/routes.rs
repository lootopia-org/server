use axum::{
    routing::{delete, get, patch, post},
    Router,
};

use crate::{
    api::hunts::hunt_steps::handlers::{
        complete_step, completed_steps, delete_step, get_step, update_step,
    },
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{id}", get(get_step))
        .route("/complete/{id}", post(complete_step))
        .route("/{id}", patch(update_step))
        .route("/{id}", delete(delete_step))
        .route("/{id}", patch(completed_steps))
}
