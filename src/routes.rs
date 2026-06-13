use crate::{
    api::{self, ws},
    auth, hunts, profiles, upload, AppState,
};
use axum::{middleware, Router};

pub fn router(state: AppState) -> Router {
    Router::new()
        .nest("/hunt", hunts::routes::router())
        .nest("/upload", upload::routes::router())
        .merge(ws::routes::router())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            api::middleware::caching::cache_middleware,
        ))
        .nest("/profile", profiles::routes::router())
        .nest("/auth", auth::router::public_routes())
        .nest("/auth", auth::router::protected_routes())
        .with_state(state)
}
