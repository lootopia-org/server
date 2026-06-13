use axum::{
    routing::{delete, get, patch},
    Router,
};

use crate::{
    profiles::handlers::{admin_update_profile, delete_profile, get_profile, list_profiles, update_profile},
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/list", get(list_profiles))
        .route("/{userId}", patch(admin_update_profile))
        .route("/", get(get_profile))
        .route("/", patch(update_profile))
        .route("/", delete(delete_profile))
}
