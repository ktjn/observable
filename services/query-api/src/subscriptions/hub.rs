//! Process-wide provider-event fan-out for read-only view subscriptions.
//!
//! ## Why this is not `stream-processor`'s `QueueConsumer`
//!
//! The only existing consumer in this repo (`services/stream-processor/src/consumer.rs`)
//! is a batching, manually offset-committing consumer in a shared consumer group,
//! built for ingestion throughput. That shape is wrong here on three counts:
//!
//! 1. **Consumer group.** A shared `group.id` partitions a topic *across* replicas,
//!    so an event handed to replica A would never reach an SSE connection parked on
//!    replica B. Each query-api process therefore joins under its own unique
//!    group, which makes every process a full fan-out subscriber of every topic and
//!    removes any need for sticky-session routing (ADR-046 decision 2).
//! 2. **Offsets.** Nothing durable is tracked, so there is nothing to commit. A
//!    connection's correctness comes from its authorized snapshot plus subsequent
//!    events; a gap is repaired by `reset` and a fresh snapshot, never by replaying
//!    committed offsets. Consumers start at `latest` because history is already
//!    covered by the snapshot.
//! 3. **Batching.** Batching trades latency for throughput. These are low-volume
//!    control-plane state changes feeding a p95 ≤ 2 s budget, so each message is
//!    broadcast as it arrives.
//!
//! One `StreamConsumer` per topic runs in its own background task and publishes
//! parsed events into a bounded `tokio::sync::broadcast`. Each SSE connection holds
//! a receiver and filters by its own tenant and registered views. A connection that
//! cannot keep up with the broadcast gets `RecvError::Lagged`, which is exactly the
//! "continuity cannot be proven" signal that produces a `reset` (PUSH-006).

use super::events::{ProviderEvent, ProviderTopic};
use super::replay::ReplayStore;
use rdkafka::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::message::Message;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Semaphore, broadcast};
use uuid::Uuid;

/// Tunables for the subscription facade. Defaults are the MVP floor; tests narrow
/// them to force overflow and lag deterministically.
#[derive(Debug, Clone)]
pub struct SubscriptionConfig {
    /// Fixed size of each connection's outgoing event queue. On overflow the
    /// connection is disconnected rather than the queue growing (PUSH-020).
    pub queue_capacity: usize,
    /// Size of the process-wide provider-event fan-out buffer.
    pub fanout_capacity: usize,
    /// Ceiling on concurrent subscription connections per process (PUSH-008).
    pub max_connections: usize,
    /// SSE heartbeat comment interval.
    pub heartbeat: Duration,
}

impl Default for SubscriptionConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 64,
            fanout_capacity: 1024,
            max_connections: 512,
            heartbeat: Duration::from_secs(15),
        }
    }
}

/// Shared state backing `GET /v1/ui/subscriptions/events`.
#[derive(Clone)]
pub struct SubscriptionHub {
    events: broadcast::Sender<ProviderEvent>,
    replay: ReplayStore,
    connections: Arc<Semaphore>,
    config: SubscriptionConfig,
}

impl SubscriptionHub {
    /// A hub with no broker consumers. The endpoint still serves authorized
    /// snapshots; it simply never receives provider events. This is also how
    /// tests inject events directly, and mirrors the provider services'
    /// `Option<producer>` "broker not configured" behaviour.
    pub fn new(config: SubscriptionConfig) -> Self {
        let (events, _) = broadcast::channel(config.fanout_capacity);
        Self {
            events,
            replay: ReplayStore::new(),
            connections: Arc::new(Semaphore::new(config.max_connections)),
            config,
        }
    }

    /// Builds a hub from the environment, spawning one consumer task per
    /// configured topic. Topic variables are read individually, so a deployment
    /// may subscribe to only some providers. Absent `REDPANDA_BROKERS` yields a
    /// consumer-less hub rather than a startup failure.
    pub fn from_env(config: SubscriptionConfig) -> Self {
        let hub = Self::new(config);
        let Some(brokers) = std::env::var("REDPANDA_BROKERS")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        else {
            tracing::info!(
                "REDPANDA_BROKERS not set -- read-only view subscriptions will serve snapshots \
                 without provider push updates"
            );
            return hub;
        };

        let topics = [
            (ProviderTopic::DeploymentMarkers, "DEPLOYMENT_MARKERS_TOPIC"),
            (ProviderTopic::ChangeMarkers, "CHANGE_MARKERS_TOPIC"),
            (ProviderTopic::HealthTransitions, "HEALTH_TRANSITIONS_TOPIC"),
            (ProviderTopic::AlertingState, "ALERTING_STATE_TOPIC"),
        ];
        for (topic, env_var) in topics {
            let Some(name) = std::env::var(env_var)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
            else {
                tracing::info!(%env_var, "topic not configured -- skipping provider subscription");
                continue;
            };
            match hub.spawn_consumer(&brokers, topic, &name) {
                Ok(()) => {
                    tracing::info!(topic = %name, kind = topic.as_str(), "subscribed to provider topic")
                }
                Err(e) => tracing::error!(
                    error = %e,
                    topic = %name,
                    "failed to subscribe to provider topic -- view subscriptions will miss its updates"
                ),
            }
        }
        hub
    }

    /// Spawns a background task consuming one provider topic into the fan-out.
    pub fn spawn_consumer(
        &self,
        brokers: &str,
        topic: ProviderTopic,
        topic_name: &str,
    ) -> anyhow::Result<()> {
        // A unique group per process: every replica must observe every event,
        // because any replica may be holding the connection that cares about it.
        let group_id = format!("query-api-sse-{}", Uuid::new_v4());
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", &group_id)
            // History is already covered by each connection's authorized snapshot.
            .set("auto.offset.reset", "latest")
            // Nothing durable is tracked, so there is no offset to commit.
            .set("enable.auto.commit", "false")
            .create()?;
        consumer.subscribe(&[topic_name])?;

        let events = self.events.clone();
        let topic_name = topic_name.to_string();
        tokio::spawn(async move {
            loop {
                match consumer.recv().await {
                    Ok(message) => {
                        let Some(payload) = message.payload() else {
                            continue;
                        };
                        match topic.parse_payload(payload) {
                            // A send error only means no connection is currently
                            // subscribed, which is the common idle case.
                            Ok(event) => {
                                let _ = events.send(event);
                            }
                            Err(e) => tracing::warn!(
                                error = %e,
                                topic = %topic_name,
                                "discarding unparseable provider event"
                            ),
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, topic = %topic_name, "provider topic receive error");
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            }
        });
        Ok(())
    }

    /// Publishes an event into the fan-out. Used by consumer tasks and by tests
    /// that exercise delivery without a broker.
    pub fn publish(&self, event: ProviderEvent) {
        let _ = self.events.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ProviderEvent> {
        self.events.subscribe()
    }

    pub fn replay(&self) -> &ReplayStore {
        &self.replay
    }

    pub fn config(&self) -> &SubscriptionConfig {
        &self.config
    }

    /// Admits one connection, or `None` when the process is at its connection
    /// ceiling. The permit is held for the connection's lifetime.
    pub fn admit(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        self.connections.clone().try_acquire_owned().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subscriptions::events::HealthTransitionEvent;

    fn health_event(tenant_id: Uuid) -> ProviderEvent {
        ProviderEvent::Health(HealthTransitionEvent {
            tenant_id,
            service_id: Uuid::new_v4(),
            environment_class: "production".into(),
            state: "degraded".into(),
            evidence_status: "current".into(),
            evaluated_at_unix_nano: 1,
        })
    }

    #[tokio::test]
    async fn a_published_event_reaches_every_subscriber() {
        let hub = SubscriptionHub::new(SubscriptionConfig::default());
        let mut first = hub.subscribe();
        let mut second = hub.subscribe();
        let tenant = Uuid::new_v4();

        hub.publish(health_event(tenant));

        assert_eq!(first.recv().await.unwrap().tenant_id(), tenant);
        assert_eq!(second.recv().await.unwrap().tenant_id(), tenant);
    }

    #[tokio::test]
    async fn publishing_with_no_subscribers_is_not_an_error() {
        let hub = SubscriptionHub::new(SubscriptionConfig::default());
        hub.publish(health_event(Uuid::new_v4()));
    }

    #[tokio::test]
    async fn a_subscriber_that_falls_behind_the_fanout_observes_lag() {
        let hub = SubscriptionHub::new(SubscriptionConfig {
            fanout_capacity: 2,
            ..SubscriptionConfig::default()
        });
        let mut rx = hub.subscribe();
        for _ in 0..5 {
            hub.publish(health_event(Uuid::new_v4()));
        }
        // Lag is reported rather than silently dropping events, which is what lets
        // a connection emit `reset` instead of implying complete delivery.
        assert!(matches!(
            rx.recv().await,
            Err(broadcast::error::RecvError::Lagged(_))
        ));
    }

    #[tokio::test]
    async fn connections_are_admitted_up_to_the_configured_ceiling() {
        let hub = SubscriptionHub::new(SubscriptionConfig {
            max_connections: 2,
            ..SubscriptionConfig::default()
        });
        let first = hub.admit();
        let second = hub.admit();
        assert!(first.is_some() && second.is_some());
        assert!(hub.admit().is_none(), "third connection must be refused");

        drop(first);
        assert!(hub.admit().is_some(), "a closed connection frees its slot");
    }

    #[test]
    fn a_hub_without_brokers_configured_still_serves() {
        // No REDPANDA_BROKERS in the test environment: the hub is consumer-less
        // but fully usable for authorized snapshots.
        let hub = SubscriptionHub::new(SubscriptionConfig::default());
        assert_eq!(hub.config().queue_capacity, 64);
    }
}
