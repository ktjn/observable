// Publishes `alerting.state.v1` so the read-only SSE subscription facade
// (observable-query, PUSH-001/PUSH-013) can treat a committed incident
// state change as a trigger to re-run its own authorized query -- this
// event is a freshness signal, not the data delivered to a browser.
// Mirrors admin-service's `DeploymentEventProducer` (`queue.rs`).
//
// PUSH-013-C2 requires that a denied/unauthorized incident or alert event
// never discloses its identifier, existence, count or label to an
// unauthorized subscriber. `AlertStateChangeEvent` is deliberately minimal:
// it carries only fields already visible to any authorized tenant member
// via `GET /v1/incidents/{id}` -- no alert-rule internals (condition,
// notification_channels), no free text (title, actor, message,
// impact_summary), no cross-incident data. `incidents.rs` only calls
// `publish` after the `incident_events` row's commit succeeds, so a
// rejected or failed write never reaches the producer.

use async_trait::async_trait;
use rdkafka::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlertStateChangeEvent {
    pub tenant_id: Uuid,
    pub incident_id: Uuid,
    pub status: String,
    pub severity: String,
    pub event_type: String,
    pub occurred_at_unix_nano: u64,
}

#[async_trait]
pub trait AlertEventPublisher: Send + Sync {
    async fn publish(&self, event: &AlertStateChangeEvent) -> anyhow::Result<()>;
}

pub struct AlertStateEventProducer {
    producer: FutureProducer,
    topic: String,
}

impl AlertStateEventProducer {
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
}

#[async_trait]
impl AlertEventPublisher for AlertStateEventProducer {
    async fn publish(&self, event: &AlertStateChangeEvent) -> anyhow::Result<()> {
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

    #[test]
    fn event_serializes_with_only_the_safe_field_set() {
        let event = AlertStateChangeEvent {
            tenant_id: Uuid::new_v4(),
            incident_id: Uuid::new_v4(),
            status: "resolved".into(),
            severity: "critical".into(),
            event_type: "alert_resolved".into(),
            occurred_at_unix_nano: 1_700_000_000_000_000_000,
        };
        let value = serde_json::to_value(&event).unwrap();
        let mut fields: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort();
        assert_eq!(
            fields,
            vec![
                "event_type",
                "incident_id",
                "occurred_at_unix_nano",
                "severity",
                "status",
                "tenant_id",
            ]
        );
        let decoded: AlertStateChangeEvent = serde_json::from_value(value).unwrap();
        assert_eq!(decoded, event);
    }
}
