use domain::NormalizedTelemetryBatch;
use rdkafka::{
    ClientConfig, Message,
    consumer::{Consumer, StreamConsumer},
};

/// Consumes `telemetry.normalized.v1` and forwards each batch straight into the
/// existing `WriteBuffer`, the same path the (transitional) HTTP handlers use.
/// This is the target Phase 2 ingress for storage (docs/component-
/// decomposition.md): "It consumes telemetry.normalized.v1 directly from
/// Redpanda." Runs unconditionally -- harmless if stream-processor is in HTTP
/// mode and nothing is ever published here.
pub struct NormalizedConsumer {
    consumer: StreamConsumer,
}

impl NormalizedConsumer {
    pub fn new(brokers: &str, group_id: &str, topic: &str) -> anyhow::Result<Self> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", group_id)
            .set("auto.offset.reset", "earliest")
            .create()?;
        consumer.subscribe(&[topic])?;
        Ok(Self { consumer })
    }

    pub async fn run(&self, buffer: &crate::buffer::WriteBuffer) -> anyhow::Result<()> {
        loop {
            let msg = self.consumer.recv().await?;
            let Some(payload) = msg.payload() else {
                continue;
            };
            match serde_json::from_slice::<NormalizedTelemetryBatch>(payload) {
                Ok(batch) => {
                    if !batch.spans.is_empty() {
                        buffer.send_spans(batch.spans);
                    }
                    if !batch.logs.is_empty() {
                        buffer.send_logs(batch.logs);
                    }
                    if !batch.series.is_empty() || !batch.points.is_empty() {
                        buffer.send_metrics(batch.series, batch.points);
                    }
                }
                Err(e) => tracing::warn!(error = %e, "normalized batch deserialise failed"),
            }
        }
    }
}
