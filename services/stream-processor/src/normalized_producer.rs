use domain::NormalizedTelemetryBatch;
use domain::chunk::{MAX_QUEUE_MESSAGE_BYTES, serialize_chunked};
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

    /// Publishes `batch`, split into as many records as needed to stay under
    /// the broker's message size limit. A merged batch of up to
    /// `STREAM_PROCESSOR_BATCH_SIZE` envelopes routinely exceeds 1 MiB.
    pub async fn publish(&self, batch: &NormalizedTelemetryBatch, key: &str) -> anyhow::Result<()> {
        let chunks = serialize_chunked(batch.clone(), MAX_QUEUE_MESSAGE_BYTES).inspect_err(
            |e| tracing::error!(error = %e, topic = %self.topic, "normalized batch chunking failed"),
        )?;
        for payload in &chunks {
            self.producer
                .send(
                    FutureRecord::to(&self.topic).key(key).payload(payload),
                    Duration::from_secs(5),
                )
                .await
                .map_err(|(e, _)| {
                    tracing::error!(error = %e, topic = %self.topic, bytes = payload.len(), "kafka send failed");
                    anyhow::anyhow!("kafka send error: {e}")
                })?;
        }
        Ok(())
    }
}
