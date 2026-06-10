use anyhow::Context;
use redis::aio::ConnectionManager;

#[derive(Debug, Clone)]
pub struct RedisConfig {
    pub url: String,
}

pub async fn connect(config: &RedisConfig) -> anyhow::Result<ConnectionManager> {
    let client =
        redis::Client::open(config.url.as_str()).context("creating Redis client")?;
    ConnectionManager::new(client)
        .await
        .context("connecting to Redis")
}
