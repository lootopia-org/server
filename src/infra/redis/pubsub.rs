use anyhow::Context;
use futures::StreamExt;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use tracing::{info, warn};

#[derive(Clone)]
pub struct RedisPubSub {
    url: String,
}

impl RedisPubSub {
    pub fn new(url: String) -> Self {
        Self { url }
    }

    pub async fn publish(
        &self,
        conn: &ConnectionManager,
        channels: &[String],
        payload: &str,
    ) -> anyhow::Result<()> {
        let mut conn = conn.clone();
        for channel in channels {
            conn.publish::<_, _, ()>(channel, payload)
                .await
                .context("publishing event to Redis")?;
        }
        Ok(())
    }

    pub async fn listen<T, F>(self, channel: &str, on_message: F) -> anyhow::Result<()>
    where
        T: Send + 'static,
        F: Fn(T) + Send + Sync + 'static,
        T: for<'de> serde::Deserialize<'de>,
    {
        let client = redis::Client::open(self.url.as_str())
            .context("creating Redis pub/sub client")?;
        let mut pubsub = client
            .get_async_pubsub()
            .await
            .context("opening Redis pub/sub")?;

        pubsub
            .subscribe(channel)
            .await
            .context("subscribing to Redis channel")?;

        info!(channel = %channel, "Redis live-event listener started");

        let mut stream = pubsub.on_message();
        while let Some(message) = stream.next().await {
            let payload: String = message.get_payload().context("reading Redis payload")?;
            match serde_json::from_str::<T>(&payload) {
                Ok(event) => on_message(event),
                Err(err) => {
                    warn!(error = %err, "skipping malformed Redis event");
                }
            }
        }

        Ok(())
    }
}
