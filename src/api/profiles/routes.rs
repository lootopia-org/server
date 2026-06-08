use axum::{
    routing::{delete, get, patch, post},
    Router,
};

use crate::{
    profiles::handlers::{create_profile, delete_profile, get_profile, update_profile},
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/get", get(get_profile))
        .route("/create", post(create_profile))
        .route("/update", patch(update_profile))
        .route("/delete", delete(delete_profile))
}
