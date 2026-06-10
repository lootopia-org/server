use axum::{routing::get, Router};

use crate::{api::ws::ws::live_ws, AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/ws", get(live_ws))
}
