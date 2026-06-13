use std::future::Future;
use std::sync::Arc;

use anyhow::Context;
use redis::aio::ConnectionManager;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::broadcast;
use tracing::{error, warn};

use crate::event::event::{global_channel, Event};
use crate::infra::kafka::{KafkaConfig, KafkaConsumer, KafkaProducer};
use crate::infra::redis::{connect as connect_redis, RedisCache, RedisConfig, RedisPubSub};

const BROADCAST_CAPACITY: usize = 1024;

#[derive(Clone)]
pub struct EventHandler {
    kafka: KafkaProducer,
    cache: RedisCache,
    redis_conn: ConnectionManager,
    redis_pubsub: RedisPubSub,
    kafka_config: KafkaConfig,
    broadcaster: broadcast::Sender<Event>,
}

impl EventHandler {
    pub async fn connect_and_spawn(
        kafka: &KafkaConfig,
        redis: &RedisConfig,
    ) -> anyhow::Result<Arc<Self>> {
        let handler = Arc::new(Self::connect(kafka, redis).await?);
        handler.clone().spawn_relay();
        Ok(handler)
    }

    async fn connect(kafka: &KafkaConfig, redis: &RedisConfig) -> anyhow::Result<Self> {
        let kafka_producer = KafkaProducer::connect(kafka)?;
        let redis_conn = connect_redis(redis).await?;
        let cache = RedisCache::new(redis_conn.clone());
        let redis_pubsub = RedisPubSub::new(redis.url.clone());

        let (broadcaster, _) = broadcast::channel(BROADCAST_CAPACITY);

        Ok(Self {
            kafka: kafka_producer,
            cache,
            redis_conn,
            redis_pubsub,
            kafka_config: kafka.clone(),
            broadcaster,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.broadcaster.subscribe()
    }

    pub fn publish(&self, event: Event) {
        let hub = self.clone();
        tokio::spawn(async move {
            if let Err(err) = hub.publish_inner(event).await {
                warn!(error = %err, "failed to publish event");
            }
        });
    }

    async fn publish_inner(&self, event: Event) -> anyhow::Result<()> {
        let payload = serde_json::to_string(&event).context("serializing live event")?;
        self.kafka.publish(&event.event_type, &payload).await
    }

    async fn fanout(&self, event: Event) -> anyhow::Result<()> {
        let payload = serde_json::to_string(&event).context("serializing live event")?;
        self.redis_pubsub
            .publish(&self.redis_conn, &event.redis_channels(), &payload)
            .await
    }

    fn spawn_relay(self: Arc<Self>) {
        let kafka_hub = Arc::clone(&self);
        let kafka_config = self.kafka_config.clone();
        tokio::spawn(async move {
            if let Err(err) = kafka_hub.run_kafka_relay(&kafka_config).await {
                error!(error = %err, "Kafka relay stopped");
            }
        });

        let redis_hub = Arc::clone(&self);
        tokio::spawn(async move {
            if let Err(err) = redis_hub.run_redis_listener().await {
                error!(error = %err, "Redis listener stopped");
            }
        });
    }

    async fn run_kafka_relay(self: Arc<Self>, config: &KafkaConfig) -> anyhow::Result<()> {
        KafkaConsumer::relay(config, |payload| {
            let hub = Arc::clone(&self);
            let payload = payload.to_vec();
            async move {
                let event: Event = match serde_json::from_slice(&payload) {
                    Ok(event) => event,
                    Err(err) => {
                        warn!(error = %err, "skipping malformed Kafka event");
                        return Ok(());
                    }
                };
                hub.fanout(event).await
            }
        })
        .await
    }

    async fn run_redis_listener(self: Arc<Self>) -> anyhow::Result<()> {
        let broadcaster = self.broadcaster.clone();
        self.redis_pubsub
            .clone()
            .listen(&global_channel(), move |event: Event| {
                let _ = broadcaster.send(event);
            })
            .await
    }

    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> anyhow::Result<Option<T>> {
        self.cache.get(key).await
    }

    pub async fn set<T: Serialize>(&self, key: &str, value: &T) -> anyhow::Result<()> {
        self.cache.set(key, value).await
    }

    pub async fn set_with_ttl<T: Serialize>(
        &self,
        key: &str,
        value: &T,
        ttl_secs: u64,
    ) -> anyhow::Result<()> {
        self.cache.set_with_ttl(key, value, ttl_secs).await
    }

    pub async fn exists(&self, key: &str) -> anyhow::Result<bool> {
        self.cache.exists(key).await
    }

    pub async fn delete(&self, keys: &[&str]) -> anyhow::Result<u64> {
        self.cache.delete(keys).await
    }

    pub async fn cached<T, F, Fut>(&self, key: &str, f: F) -> anyhow::Result<T>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce() -> Fut,
        Fut: Future<Output = anyhow::Result<T>>,
    {
        self.cache.cached(key, f).await
    }
}
