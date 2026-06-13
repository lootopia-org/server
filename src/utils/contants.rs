use chrono::{DateTime, Utc};
use std::sync::LazyLock;

pub const SESSION_MAX_AGE: i64 = 7 * 24 * 60 * 60;
pub const JWS_HEADER: &str = r#"{"alg":"HS256","typ":"JWT"}"#;
pub const SIGN_LABEL: &[u8] = b"lootopia-jwt-sign-v1";
pub const ENC_LABEL: &[u8] = b"lootopia-jwt-enc-v1";
pub const NONCE_LEN: usize = 12;
pub static NOW: LazyLock<DateTime<Utc>> = LazyLock::new(Utc::now);

pub const PROXIMITY_THRESHOLD_METERS: f64 = 100.0;
pub const PROXIMITY_COOLDOWN_SECS: u64 = 300;
