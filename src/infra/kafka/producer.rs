use std::time::Duration;

use anyhow::Context;
use rdkafka::config::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};

use super::KafkaConfig;

#[derive(Clone)]
pub struct KafkaProducer {
    producer: FutureProducer,
    topic: String,
}

impl KafkaProducer {
    pub fn connect(config: &KafkaConfig) -> anyhow::Result<Self> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", &config.brokers)
            .set("message.timeout.ms", "5000")
            .set("queue.buffering.max.ms", "0")
            .create()
            .context("creating Kafka producer")?;

        Ok(Self {
            producer,
            topic: config.topic.clone(),
        })
    }

    pub async fn publish(&self, key: &str, payload: &str) -> anyhow::Result<()> {
        let record = FutureRecord::to(&self.topic).key(key).payload(payload);

        self.producer
            .send(record, Duration::from_secs(5))
            .await
            .map_err(|(err, _)| err)
            .context("sending event to Kafka")?;

        Ok(())
    }
}
