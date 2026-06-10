use std::time::Duration;

use anyhow::Context;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::message::Message;
use tracing::{info, warn};

use super::KafkaConfig;

pub struct KafkaConsumer;

impl KafkaConsumer {
    pub async fn relay<F, Fut>(config: &KafkaConfig, mut on_message: F) -> anyhow::Result<()>
    where
        F: FnMut(&[u8]) -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<()>>,
    {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &config.brokers)
            .set("group.id", "lootopia-relay")
            .set("enable.auto.commit", "true")
            .set("auto.offset.reset", "latest")
            .create()
            .context("creating Kafka consumer")?;

        consumer
            .subscribe(&[&config.topic])
            .context("subscribing to Kafka topic")?;

        info!(topic = %config.topic, "Kafka live-event relay started");

        loop {
            match consumer.recv().await {
                Ok(message) => {
                    let Some(payload) = message.payload() else {
                        continue;
                    };
                    if let Err(err) = on_message(payload).await {
                        warn!(error = %err, "failed to process Kafka event");
                    }
                }
                Err(err) => {
                    warn!(error = %err, "Kafka consumer error");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }
}
