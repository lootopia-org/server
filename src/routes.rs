use crate::{auth, profiles, AppState};
use axum::Router;

pub fn router(state: AppState) -> Router {
    Router::new()
        .nest("/auth", auth::router::public_routes())
        .nest("/auth", auth::router::protected_routes())
        .nest("/profile", profiles::routes::router())
        .with_state(state)
}
