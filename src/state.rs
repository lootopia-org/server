use std::sync::Arc;

use sqlx::PgPool;
use webauthn_rs::Webauthn;

use crate::config::Config;
use crate::event::EventHandler;
use crate::infra::s3::S3Storage;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub webauthn: Arc<Webauthn>,
    pub event_handler: Arc<EventHandler>,
    pub s3: S3Storage,
}
