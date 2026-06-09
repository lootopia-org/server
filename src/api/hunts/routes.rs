use axum::{
    routing::{delete, get, patch, post},
    Router,
};

use crate::{
    hunts::handlers::{
        create_hunt, delete_hunt, get_hunt, join_hunt, list_hunts, update_hunt,
    },
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_hunts))
        .route("/", post(create_hunt))
        .route("/join", post(join_hunt))
        .route("/{id}", get(get_hunt))
        .route("/{id}", patch(update_hunt))
        .route("/{id}", delete(delete_hunt))
}
