use std::sync::Arc;

use sqlx::PgPool;
use webauthn_rs::Webauthn;

use crate::config::Config;
use crate::event::EventHandler;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub webauthn: Arc<Webauthn>,
    pub event_handler: Arc<EventHandler>,
}
