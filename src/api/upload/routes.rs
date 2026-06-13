use axum::{routing::post, Router};

use crate::{
    api::upload::handlers::{upload_avatar, upload_image},
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/image", post(upload_image))
        .route("/avatar", post(upload_avatar))
}
