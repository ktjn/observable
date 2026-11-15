// Publishes `deployment.markers.v1` so ingest-gateway can maintain its
// deployment-correlation cache without a direct Postgres dependency (Phase 5
// "clean ingest", docs/component-decomposition.md), and `change.markers.v1`
// so the read-only SSE subscription facade (PUSH-003) can invalidate cached
// change-marker views without polling Postgres directly.

use domain::{ChangeMarkerEvent, DeploymentMarkerEvent};
use rdkafka::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::time::Duration;

pub struct DeploymentEventProducer {
    producer: FutureProducer,
    topic: String,
}

impl DeploymentEventProducer {
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

    pub async fn publish(&self, event: &DeploymentMarkerEvent) -> anyhow::Result<()> {
        let payload = serde_json::to_vec(event)?;
        let key = event.tenant_id.to_string();
        self.producer
            .send(
                FutureRecord::to(&self.topic).key(&key).payload(&payload),
                Duration::from_secs(5),
            )
            .await
            .map_err(|(e, _)| anyhow::anyhow!("kafka send error: {e}"))?;
        Ok(())
    }
}

/// Publishes `change.markers.v1`. Mirrors `DeploymentEventProducer`'s shape
/// (same best-effort, commit-then-publish usage from `change_events.rs`) but
/// is kept as a distinct producer/topic since change markers and deployment
/// markers are different domain events with different consumers.
pub struct ChangeEventProducer {
    producer: FutureProducer,
    topic: String,
}

impl ChangeEventProducer {
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

    pub async fn publish(&self, event: &ChangeMarkerEvent) -> anyhow::Result<()> {
        let payload = serde_json::to_vec(event)?;
        let key = event.tenant_id.to_string();
        self.producer
            .send(
                FutureRecord::to(&self.topic).key(&key).payload(&payload),
                Duration::from_secs(5),
            )
            .await
            .map_err(|(e, _)| anyhow::anyhow!("kafka send error: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn event_serializes_for_kafka() {
        let event = DeploymentMarkerEvent {
            deployment_id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            service_name: "checkout".into(),
            environment: "production".into(),
            service_version: "v1.2.3".into(),
            status: "in_progress".into(),
            started_at_unix_nano: 1_700_000_000_000_000_000,
        };
        let bytes = serde_json::to_vec(&event).unwrap();
        let decoded: DeploymentMarkerEvent = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.service_name, "checkout");
        assert_eq!(decoded.status, "in_progress");
    }

    #[test]
    fn change_event_serializes_for_kafka() {
        let event = ChangeMarkerEvent {
            change_event_id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            event_type: "feature_flag".into(),
            service_name: Some("checkout".into()),
            environment: "production".into(),
            title: "Enabled new-checkout-flow".into(),
            occurred_at_unix_nano: 1_700_000_000_000_000_000,
        };
        let bytes = serde_json::to_vec(&event).unwrap();
        let decoded: ChangeMarkerEvent = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.title, "Enabled new-checkout-flow");
        assert_eq!(decoded.event_type, "feature_flag");
    }
}
