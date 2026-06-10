pub mod consumer;
pub mod producer;

pub use consumer::KafkaConsumer;
pub use producer::KafkaProducer;

#[derive(Debug, Clone)]
pub struct KafkaConfig {
    pub brokers: String,
    pub topic: String,
}
