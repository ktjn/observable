//! Publishes `health.transitions.v1` so the SSE subscription facade (observable-query) can
//! re-query and push an authorized update without client polling (PUSH-012). Mirrors
//! admin-service's `queue::DeploymentEventProducer`/`publish_deployment_event` pattern: the
//! Postgres row in `health.transitions` (written by `history::record` inside `persist`'s
//! transaction) is the source of truth, and this event is a best-effort freshness signal for
//! the subscription manager, which re-authorizes and re-queries before any browser delivery.
//! The payload therefore excludes rule internals and per-condition evidence (gated under
//! `health.rule`/`health.evidence`) -- only the tenant/service/class-scoped state and
//! evidence status that `GET /v1/health/services/{id}` already discloses once authorized.

use rdkafka::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HealthTransitionEvent {
    pub tenant_id: Uuid,
    pub service_id: Uuid,
    pub environment_class: String,
    /// `normal` | `degraded` | `unavailable` | `unknown`.
    pub state: String,
    /// `current` | `stale` | `missing` | `insufficient`.
    pub evidence_status: String,
    pub evaluated_at_unix_nano: u64,
}

pub struct HealthEventProducer {
    producer: FutureProducer,
    topic: String,
}

impl HealthEventProducer {
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

    pub async fn publish(&self, event: &HealthTransitionEvent) -> anyhow::Result<()> {
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
    fn event_serializes_without_rule_or_condition_internals() {
        let event = HealthTransitionEvent {
            tenant_id: Uuid::new_v4(),
            service_id: Uuid::new_v4(),
            environment_class: "production".into(),
            state: "degraded".into(),
            evidence_status: "current".into(),
            evaluated_at_unix_nano: 1_700_000_000_000_000_000,
        };
        let v = serde_json::to_value(&event).unwrap();
        assert_eq!(v["state"], "degraded");
        assert_eq!(v["evidence_status"], "current");
        for restricted in ["conditions", "rule_id", "rule_revision", "identities"] {
            assert!(
                v.get(restricted).is_none(),
                "event must not carry {restricted}"
            );
        }
    }
}
