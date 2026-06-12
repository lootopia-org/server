use axum::{
    routing::{delete, get, patch, post},
    Router,
};

use crate::{
    api::hunts::hunt_steps,
    hunts::handlers::{
        create_hunt, delete_hunt, get_hunt, get_hunt_participants, hunts_in_progrss, join_hunt,
        leave_hunt, list_hunts, update_hunt,
    },
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_hunts))
        .route("/", post(create_hunt))
        .route("/partcipants", get(get_hunt_participants))
        .route("/join", post(join_hunt))
        .route("/leave", post(leave_hunt))
        .route("/joined", get(hunts_in_progrss))
        .route("/{id}", get(get_hunt))
        .route("/{id}", patch(update_hunt))
        .route("/{id}", delete(delete_hunt))
        .nest("/step", hunt_steps::routes::router())
}
