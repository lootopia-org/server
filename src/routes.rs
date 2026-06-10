use crate::{
    api::{self, ws},
    auth, hunts, profiles, AppState,
};
use axum::{middleware, Router};

pub fn router(state: AppState) -> Router {
    Router::new()
        .nest("/auth", auth::router::public_routes())
        .nest("/auth", auth::router::protected_routes())
        .nest("/profile", profiles::routes::router())
        .nest("/hunt", hunts::routes::router())
        .merge(ws::routes::router())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            api::middleware::caching::cache_middleware,
        ))
        .with_state(state)
}
