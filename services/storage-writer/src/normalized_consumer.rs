use domain::NormalizedTelemetryBatch;
use rdkafka::{
    ClientConfig, Message, Offset, TopicPartitionList,
    consumer::{CommitMode, Consumer, StreamConsumer},
};

/// Consumes `telemetry.normalized.v1` and forwards each batch into the
/// existing `WriteBuffer` via its durable ("awaits actual ClickHouse
/// persistence") methods, committing the Kafka offset only once that
/// succeeds. This is the target Phase 2 ingress for storage (docs/component-
/// decomposition.md): "It consumes telemetry.normalized.v1 directly from
/// Redpanda." Runs unconditionally -- harmless if stream-processor is in HTTP
/// mode and nothing is ever published here.
///
/// `WriteBuffer`'s flush loops retry indefinitely on a ClickHouse write
/// failure rather than dropping (see buffer.rs), so a storage outage here
/// backpressures: `send_*_durable` blocks until the write succeeds, this loop
/// doesn't call `recv()` again until it returns, and the offset is never
/// committed for a message that hasn't actually landed in ClickHouse.
pub struct NormalizedConsumer {
    consumer: StreamConsumer,
    topic: String,
}

impl NormalizedConsumer {
    pub fn new(brokers: &str, group_id: &str, topic: &str) -> anyhow::Result<Self> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", group_id)
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.commit", "false")
            .create()?;
        consumer.subscribe(&[topic])?;
        Ok(Self {
            consumer,
            topic: topic.to_string(),
        })
    }

    /// See `QueueConsumer::commit` (stream-processor) for why `CommitMode::Async`.
    fn commit(&self, partition: i32, offset: i64) -> anyhow::Result<()> {
        let mut tpl = TopicPartitionList::new();
        tpl.add_partition_offset(&self.topic, partition, Offset::Offset(offset + 1))?;
        self.consumer.commit(&tpl, CommitMode::Async)?;
        Ok(())
    }

    pub async fn run(&self, buffer: &crate::buffer::WriteBuffer) -> anyhow::Result<()> {
        loop {
            let msg = self.consumer.recv().await?;
            let partition = msg.partition();
            let offset = msg.offset();
            let Some(payload) = msg.payload() else {
                self.commit(partition, offset)?;
                continue;
            };
            match serde_json::from_slice::<NormalizedTelemetryBatch>(payload) {
                Ok(batch) => {
                    if !batch.spans.is_empty() {
                        buffer.send_spans_durable(batch.spans).await?;
                    }
                    if !batch.logs.is_empty() {
                        buffer.send_logs_durable(batch.logs).await?;
                    }
                    if !batch.series.is_empty() || !batch.points.is_empty() {
                        buffer
                            .send_metrics_durable(batch.series, batch.points)
                            .await?;
                    }
                }
                // Not retryable -- this payload can never parse -- so still
                // commit past it rather than getting stuck on it forever.
                Err(e) => tracing::warn!(error = %e, "normalized batch deserialise failed"),
            }
            self.commit(partition, offset)?;
        }
    }
}
