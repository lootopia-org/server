use std::future::Future;

use anyhow::Context;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Clone)]
pub struct RedisCache {
    conn: ConnectionManager,
}

impl RedisCache {
    pub fn new(conn: ConnectionManager) -> Self {
        Self { conn }
    }

    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> anyhow::Result<Option<T>> {
        let mut conn = self.conn.clone();
        let value: Option<String> = conn.get(key).await.context("redis GET")?;
        Ok(value.as_deref().and_then(|s| serde_json::from_str(s).ok()))
    }

    pub async fn set<T: Serialize>(&self, key: &str, value: &T) -> anyhow::Result<()> {
        let payload = serde_json::to_string(value).context("serializing value")?;
        let mut conn = self.conn.clone();
        conn.set::<_, _, ()>(key, payload).await?;
        Ok(())
    }

    pub async fn delete(&self, keys: &[&str]) -> anyhow::Result<u64> {
        let mut conn = self.conn.clone();
        let removed: u64 = conn.del(keys).await.context("redis DEL")?;
        Ok(removed)
    }

    pub async fn cached<T, F, Fut>(&self, key: &str, f: F) -> anyhow::Result<T>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce() -> Fut,
        Fut: Future<Output = anyhow::Result<T>>,
    {
        if let Ok(Some(cached)) = self.get::<T>(key).await {
            return Ok(cached);
        }
        let value = f().await?;
        let _ = self.set(key, &value).await;
        Ok(value)
    }
}
