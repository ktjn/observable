//! Testcontainers-backed integration test for `AlertStateEventProducer`
//! (`queue.rs`) against a real Redpanda broker. Mirrors the scaffolding in
//! `services/health-service/tests/health_transition_publish_integration.rs`.
//!
//! The fake-based `RecordingPublisher` test in
//! `postgres_incident_state_events_integration.rs` already proves the HTTP
//! handler only calls `publish` after commit and that the payload carries
//! only the safe field set (PUSH-013-C2). This test proves the producer
//! itself round-trips a real event through a real broker with that same
//! safe field set intact -- no alert-rule internals, no free text.

use alert_evaluator::queue::{AlertEventPublisher, AlertStateChangeEvent, AlertStateEventProducer};
use rdkafka::consumer::{BaseConsumer, Consumer, StreamConsumer};
use rdkafka::{
    ClientConfig, Message,
    admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
    client::DefaultClientContext,
};
use std::net::TcpListener;
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
/// subscribed consumer, ready to observe whatever `alert-evaluator` publishes to it.
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
        let topic = format!("alert-state-events-test-{}", Uuid::new_v4());
        create_topic(&brokers, &topic).await;
        Self {
            brokers,
            topic,
            _container: container,
        }
    }

    fn producer(&self) -> AlertStateEventProducer {
        AlertStateEventProducer::new(&self.brokers, &self.topic).expect("producer created")
    }

    fn consumer(&self) -> StreamConsumer {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &self.brokers)
            .set("group.id", format!("test-{}", Uuid::new_v4()))
            .set("auto.offset.reset", "earliest")
            .create()
            .expect("consumer created");
        consumer
            .subscribe(&[self.topic.as_str()])
            .expect("subscribed");
        consumer
    }
}

async fn recv_event(consumer: &StreamConsumer, timeout: Duration) -> Option<AlertStateChangeEvent> {
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
async fn a_published_event_round_trips_through_a_real_broker_with_only_the_safe_field_set() {
    let broker = Broker::start().await;
    let producer = broker.producer();
    let consumer = broker.consumer();

    let event = AlertStateChangeEvent {
        tenant_id: Uuid::new_v4(),
        incident_id: Uuid::new_v4(),
        status: "resolved".into(),
        severity: "critical".into(),
        event_type: "alert_resolved".into(),
        occurred_at_unix_nano: 1_700_000_000_000_000_000,
    };

    producer.publish(&event).await.expect("event published");

    let received = recv_event(&consumer, Duration::from_secs(10))
        .await
        .expect("published event received from the real broker");
    assert_eq!(received, event);

    // PUSH-013-C2: only the safe field set may ever cross the broker -- no
    // alert-rule internals (condition, notification_channels), no free text
    // (title, actor, message, impact_summary), no cross-incident data. The
    // deserialized event is exactly what crossed the wire, so re-serializing
    // it enumerates the fields the broker actually carried.
    let raw = serde_json::to_value(&received).unwrap();
    let mut fields: Vec<&str> = raw
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
}
