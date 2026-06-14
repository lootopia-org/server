use axum::{
    routing::{delete, get, patch, post, put},
    Router,
};

use crate::{
    api::hunts::{hunt_steps, live_ops, step_photo_sessions},
    hunts::handlers::{
        create_hunt, delete_hunt, get_hunt, get_hunt_analytics, get_hunt_participants,
        hunts_completed, hunts_in_progrss, join_hunt, leave_hunt, list_hunts, pause_hunt, update_hunt,
    },
    hunts::hunt_steps::handlers::sync_hunt_steps,
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_hunts))
        .route("/", post(create_hunt))
        .route("/join", post(join_hunt))
        .route("/leave", post(leave_hunt))
        .route("/joined", get(hunts_in_progrss))
        .route("/completed", get(hunts_completed))
        .route("/{id}/participants", get(get_hunt_participants))
        .route("/{id}/analytics", get(get_hunt_analytics))
        .route("/{id}/pause", post(pause_hunt))
        .route("/{id}/steps/sync", put(sync_hunt_steps))
        .merge(live_ops::routes::router())
        .nest("/step-photo-sessions", step_photo_sessions::routes::router())
        .route("/{id}", get(get_hunt))
        .route("/{id}", patch(update_hunt))
        .route("/{id}", delete(delete_hunt))
        .nest("/step", hunt_steps::routes::router())
}
