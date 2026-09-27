use domain::NormalizedTelemetryBatch;
use rdkafka::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::time::Duration;

/// Publishes normalized telemetry batches to `telemetry.normalized.v1`. This is
/// the target Phase 2 boundary (docs/component-decomposition.md): storage
/// consumes this topic directly instead of stream-processor pushing over HTTP.
/// Selected at runtime via `STORAGE_WRITE_MODE=queue` (default remains `http`)
/// so the two paths can be dual-run during verification without code changes.
pub struct NormalizedProducer {
    producer: FutureProducer,
    topic: String,
}

impl NormalizedProducer {
    pub fn new(brokers: &str, topic: &str) -> anyhow::Result<Self> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .create()?;
        Ok(Self {
            producer,
            topic: topic.into(),
        })
    }

    pub async fn publish(&self, batch: &NormalizedTelemetryBatch, key: &str) -> anyhow::Result<()> {
        let payload = serde_json::to_vec(batch)?;
        self.producer
            .send(
                FutureRecord::to(&self.topic).key(key).payload(&payload),
                Duration::from_secs(5),
            )
            .await
            .map_err(|(e, _)| anyhow::anyhow!("kafka send error: {e}"))?;
        Ok(())
    }
}
