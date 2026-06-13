use std::env;
use std::fs;
use std::path::Path;

use crate::auth::crypto::password::PasswordParams;
use crate::infra::kafka::KafkaConfig;
use crate::infra::redis::RedisConfig;

#[derive(Debug, Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
    pub from: String,
}

#[derive(Debug, Clone)]
pub struct S3Config {
    pub bucket: String,
    pub region: String,
    pub key_prefix: String,
    pub endpoint: Option<String>,
    pub public_base_url: Option<String>,
    pub access_key_id: String,
    pub secret_access_key: String,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub kafka: KafkaConfig,
    pub redis: RedisConfig,
    pub password_pepper: String,
    pub pbkdf2_iterations: u32,
    pub rp_name: String,
    pub rp_id: String,
    pub origin: String,
    pub public_base_url: String,
    pub s3: S3Config,
    pub jwt_secret: String,
    pub email_verify_ttl_seconds: i64,
    pub ceremony_ttl_seconds: i64,
    pub require_verified_email: bool,
    pub smtp: Option<SmtpConfig>,
}

impl Config {
    pub fn password_params(&self) -> PasswordParams {
        PasswordParams {
            pepper: self.password_pepper.clone().into_bytes(),
            iterations: self.pbkdf2_iterations,
        }
    }
}

pub fn load_dotenv<P: AsRef<Path>>(path: P) {
    let Ok(contents) = fs::read_to_string(path) else {
        return;
    };
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = unquote(value.trim());
        if key.is_empty() {
            continue;
        }
        if env::var_os(key).is_none() {
            env::set_var(key, value);
        }
    }
}

fn unquote(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &s[1..s.len() - 1];
        }
    }
    s
}

pub fn load_config() -> Config {
    let smtp = load_smtp();
    Config {
        port: read_env("PORT", 8080),
        database_url: env_str(
            "DATABASE_URL",
            "host=localhost port=5432 dbname=authdb user=postgres password=postgres",
        ),
        kafka: KafkaConfig {
            brokers: env_str("KAFKA_BROKERS", "localhost:9092"),
            topic: env_str("KAFKA_TOPIC", "events"),
        },
        redis: RedisConfig {
            url: env_str("REDIS_URL", "redis://localhost:6379"),
        },
        password_pepper: env_str("PASSWORD_PEPPER", "dev-only-insecure-pepper-change-me"),
        pbkdf2_iterations: read_env("PBKDF2_ITERATIONS", 200_000),
        rp_name: env_str("RP_NAME", "Rust Auth Server"),
        rp_id: env_str("RP_ID", "localhost"),
        origin: env_str("ORIGIN", "http://localhost:8080"),
        public_base_url: env_str("PUBLIC_BASE_URL", "http://localhost:8080"),
        s3: S3Config {
            bucket: env_str("S3_BUCKET", "lootopia"),
            region: env::var("AWS_REGION")
                .or_else(|_| env::var("S3_REGION"))
                .unwrap_or_else(|_| "us-east-1".to_string()),
            key_prefix: env_str("S3_KEY_PREFIX", "lootopia"),
            endpoint: Some(
                env::var("S3_ENDPOINT")
                    .unwrap_or_else(|_| "http://127.0.0.1:9000".to_string()),
            ),
            public_base_url: env::var("S3_PUBLIC_BASE_URL")
                .ok()
                .filter(|value| !value.is_empty()),
            access_key_id: env_str("AWS_ACCESS_KEY_ID", "rustfsadmin"),
            secret_access_key: env_str("AWS_SECRET_ACCESS_KEY", "rustfsadmin"),
        },
        jwt_secret: env_str("JWT_SECRET", "dev-only-insecure-jwt-secret-change-me"),
        email_verify_ttl_seconds: read_env("EMAIL_VERIFY_TTL_SECONDS", 60 * 60 * 24),
        ceremony_ttl_seconds: read_env("CEREMONY_TTL_SECONDS", 300),
        require_verified_email: read_env("REQUIRE_VERIFIED_EMAIL", true),
        smtp,
    }
}

fn load_smtp() -> Option<SmtpConfig> {
    let host = env::var("SMTP_HOST").ok().filter(|h| !h.is_empty())?;
    Some(SmtpConfig {
        host,
        port: read_env("SMTP_PORT", 587),
        user: env_str("SMTP_USER", ""),
        pass: env_str("SMTP_PASS", ""),
        from: env_str("SMTP_FROM", "no-reply@localhost"),
    })
}

fn env_str(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn read_env<T: EnvParse>(key: &str, default: T) -> T {
    env::var(key)
        .ok()
        .and_then(|v| T::parse_env(&v))
        .unwrap_or(default)
}

trait EnvParse: Sized {
    fn parse_env(value: &str) -> Option<Self>;
}

macro_rules! impl_env_parse_fromstr {
    ($($t:ty),*) => {$(
        impl EnvParse for $t {
            fn parse_env(value: &str) -> Option<Self> {
                value.trim().parse().ok()
            }
        }
    )*};
}
impl_env_parse_fromstr!(u16, u32, i64, usize);

impl EnvParse for bool {
    fn parse_env(value: &str) -> Option<Self> {
        match value.trim() {
            "true" | "True" | "1" => Some(true),
            "false" | "False" | "0" => Some(false),
            _ => None,
        }
    }
}
