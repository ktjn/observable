//! PUSH-012: the health provider must publish a `health.transitions.v1` change event only
//! after a health-state transition is query-visible (committed to `health.transitions` by
//! `assessment::persist`), and the event must never carry data gated behind a per-condition
//! `health.evidence`/`health.rule` authorization check that the subscription manager has not
//! itself re-checked.

mod common;

use chrono::Duration as ChronoDuration;
use common::{Harness, inputs_for, ns_ago, view};
use health_service::ports::{EvidenceTimes, TelemetryIdentity};
use health_service::queue::{HealthEventProducer, HealthTransitionEvent};
use rdkafka::consumer::{BaseConsumer, Consumer, StreamConsumer};
use rdkafka::{
    ClientConfig, Message,
    admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
    client::DefaultClientContext,
};
use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;
use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};
use uuid::Uuid;

fn pick_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind to 0 to get free port");
    listener.local_addr().unwrap().port()
}

async fn wait_for_kafka_ready(brokers: &str) {
    let checker: BaseConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .create()
        .expect("readiness checker created");
    for _ in 0..30 {
        if checker
            .fetch_metadata(None, Duration::from_millis(1_000))
            .is_ok()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    panic!("Redpanda Kafka API did not become ready within 15 s");
}

async fn create_topic(brokers: &str, topic: &str) {
    let admin: AdminClient<DefaultClientContext> = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .create()
        .expect("admin client created");
    let new_topic = NewTopic::new(topic, 1, TopicReplication::Fixed(1));
    admin
        .create_topics(&[new_topic], &AdminOptions::default())
        .await
        .expect("topic creation request sent");
}

/// A broker (external or a fresh container) plus a uniquely named, pre-created topic and a
/// subscribed consumer, ready to observe whatever `health-service` publishes to it.
struct Broker {
    brokers: String,
    topic: String,
    _container: Option<ContainerAsync<GenericImage>>,
}

impl Broker {
    async fn start() -> Self {
        let (brokers, container) = match test_support::kafka::external_brokers() {
            Some(brokers) => (brokers, None),
            None => {
                let host_port = pick_free_port();
                let advertise_addr = format!("127.0.0.1:{host_port}");
                let container: ContainerAsync<GenericImage> =
                    GenericImage::new("redpandadata/redpanda", "v26.2.4")
                        .with_wait_for(WaitFor::message_on_stderr("Successfully started Redpanda!"))
                        .with_cmd(vec![
                            "redpanda".to_string(),
                            "start".to_string(),
                            "--smp=1".to_string(),
                            "--memory=512M".to_string(),
                            "--overprovisioned".to_string(),
                            "--kafka-addr=0.0.0.0:9092".to_string(),
                            format!("--advertise-kafka-addr={advertise_addr}"),
                        ])
                        .with_mapped_port(host_port, 9092_u16.tcp())
                        .start()
                        .await
                        .expect("redpanda container started");
                (advertise_addr, Some(container))
            }
        };
        wait_for_kafka_ready(&brokers).await;
        let topic = format!("health-transitions-test-{}", Uuid::new_v4());
        create_topic(&brokers, &topic).await;
        Self {
            brokers,
            topic,
            _container: container,
        }
    }

    fn producer(&self) -> HealthEventProducer {
        HealthEventProducer::new(&self.brokers, &self.topic).expect("producer created")
    }

    fn consumer(&self) -> StreamConsumer {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &self.brokers)
            .set("group.id", format!("test-{}", Uuid::new_v4()))
            .set("auto.offset.reset", "earliest")
            .create()
            .expect("consumer created");
        consumer.subscribe(&[self.topic.as_str()]).expect("subscribed");
        consumer
    }
}

fn identity(name: &str, env: &str) -> TelemetryIdentity {
    TelemetryIdentity {
        service_name: name.into(),
        environment: env.into(),
    }
}

async fn recv_event(consumer: &StreamConsumer, timeout: Duration) -> Option<HealthTransitionEvent> {
    match tokio::time::timeout(timeout, consumer.recv()).await {
        Err(_) => None,
        Ok(msg) => {
            let msg = msg.expect("no kafka error");
            let bytes = msg.payload().expect("message has payload");
            Some(serde_json::from_slice(bytes).expect("event deserializes"))
        }
    }
}

#[tokio::test]
async fn a_superseded_evaluation_never_publishes_an_event() {
    let broker = Broker::start().await;
    let mut h = Harness::start().await;
    h.state.producer = Some(Arc::new(broker.producer()));
    let consumer = broker.consumer();

    // Seed one successful transition first so the second, older evaluation is superseded
    // (not stored), which must not publish anything either.
    let id = Uuid::new_v4();
    h.services.put(h.tenant, view(id, "live", "internal"));
    h.evidence.record(
        h.tenant,
        &identity("checkout", "prod"),
        EvidenceTimes {
            traces: Some(ns_ago(10)),
            ..Default::default()
        },
    );
    h.inputs.set(h.tenant, vec![inputs_for(id, "checkout", "live")]);
    let first = health_service::assessment::evaluate_all(
        &h.state.db,
        &h.inputs,
        &h.evidence,
        h.state.producer.as_deref(),
    )
    .await;
    assert_eq!(first.stored, 2, "both environment classes evaluate");
    // Drain the two events the successful pass legitimately published, so the superseded
    // pass below is observed against an empty topic.
    for _ in 0..2 {
        assert!(
            recv_event(&consumer, Duration::from_secs(10)).await.is_some(),
            "expected the seeding pass's own events to arrive"
        );
    }

    // Re-run with an evaluation timestamp from the past: `persist` rejects it as stale
    // (`evaluated_at` older than what is stored), so nothing commits and nothing publishes.
    let rule = health_service::assessment::current_rule(&h.state.db, h.tenant)
        .await
        .unwrap()
        .unwrap();
    let ids = vec![identity("checkout", "prod")];
    let stale_snapshot = health_service::assessment::Snapshot {
        tenant_id: h.tenant,
        service_id: id,
        environment_class: health_service::ports::EnvironmentClass::Production,
        rule_id: rule.rule_id.clone(),
        rule_revision: rule.revision,
        scope_key: health_service::assessment::scope_key(&ids),
        identities: ids,
        conditions: vec![health_service::assessment::ConditionEvidence::presence(
            "any",
            "internal",
            Some(chrono::Utc::now() - ChronoDuration::seconds(600)),
        )],
        freshness_seconds: rule.freshness_seconds,
        evaluated_at: chrono::Utc::now() - ChronoDuration::seconds(600),
    };
    let stored = health_service::assessment::persist(&h.state.db, &stale_snapshot)
        .await
        .unwrap();
    assert!(!stored, "an older evaluated_at must not overwrite a newer one");

    assert!(
        recv_event(&consumer, Duration::from_secs(3)).await.is_none(),
        "a write that never commits (superseded) must never publish an event"
    );
}

#[tokio::test]
async fn a_committed_transition_publishes_the_query_visible_state_and_nothing_restricted() {
    let broker = Broker::start().await;
    let mut h = Harness::start().await;
    h.state.producer = Some(Arc::new(broker.producer()));
    let consumer = broker.consumer();

    let id = Uuid::new_v4();
    h.services.put(h.tenant, view(id, "live", "internal"));
    h.evidence.record(
        h.tenant,
        &identity("checkout", "prod"),
        EvidenceTimes {
            traces: Some(ns_ago(10)),
            ..Default::default()
        },
    );
    h.inputs.set(h.tenant, vec![inputs_for(id, "checkout", "live")]);

    let summary = h.evaluate().await;
    assert_eq!(summary.stored, 2);

    let mut seen_production = false;
    for _ in 0..2 {
        let event = recv_event(&consumer, Duration::from_secs(10))
            .await
            .expect("committed transition publishes an event");
        assert_eq!(event.tenant_id, h.tenant);
        assert_eq!(event.service_id, id);

        // The event's (tenant, service, class, state) must match exactly what the
        // already-committed, query-visible read discloses -- never a pre-commit hint.
        let (_, body, raw) = h.health(id, &event.environment_class, "member").await;
        assert_eq!(body["state"], serde_json::json!(event.state), "{raw}");

        if event.environment_class == "production" {
            seen_production = true;
            assert_eq!(event.state, "normal");
            assert_eq!(event.evidence_status, "current");
        } else {
            assert_eq!(event.environment_class, "non_production");
            assert_eq!(event.state, "unknown");
            assert_eq!(event.evidence_status, "missing");
        }

        // The payload must not leak anything gated behind a separate `health.evidence` or
        // `health.rule` authorization check (conditions, rule identity, raw service
        // bindings) -- only the tenant/service/class-scoped state the caller already sees
        // once authorized for `health.assessment`.
        let v = serde_json::to_value(&event).unwrap();
        for restricted in ["conditions", "rule_id", "rule_revision", "identities", "scope_key"] {
            assert!(v.get(restricted).is_none(), "leaked restricted field {restricted}");
        }
    }
    assert!(seen_production, "expected a production-class event");
}
