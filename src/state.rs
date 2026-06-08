use std::sync::Arc;

use sqlx::PgPool;
use webauthn_rs::Webauthn;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub webauthn: Arc<Webauthn>,
}
