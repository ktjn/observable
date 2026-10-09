// DeploymentRegistry is now fed by deployment.markers.v1 Kafka events, not a
// direct Postgres lookup (Phase 5 "clean ingest",
// docs/component-decomposition.md). Its lib-level unit tests in
// src/deployment_registry.rs cover the exact-match/empty-version/status-filter/
// most-recent-wins semantics directly (no Testcontainers needed for those
// anymore, since there's no longer a database involved). This file instead
// covers the Kafka consumer wiring end-to-end against a real Redpanda broker,
// following the same container setup as
// services/stream-processor/tests/redpanda_integration.rs.

use domain::DeploymentMarkerEvent;
use ingest_gateway::deployment_registry::{DeploymentRegistry, run_consumer};
use rdkafka::{
    ClientConfig,
    admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
    client::DefaultClientContext,
    consumer::{BaseConsumer, Consumer},
    producer::{FutureProducer, FutureRecord},
};
use std::{net::TcpListener, time::Duration};
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

async fn start_redpanda() -> (String, ContainerAsync<GenericImage>) {
    let host_port = pick_free_port();
    let advertise_addr = format!("127.0.0.1:{host_port}");
    let brokers = advertise_addr.clone();

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

    wait_for_kafka_ready(&brokers).await;
    (brokers, container)
}

#[tokio::test]
async fn consumer_applies_published_event_to_registry() {
    let (brokers, _container) = start_redpanda().await;
    let topic = format!("deployment-markers-{}", Uuid::new_v4());
    create_topic(&brokers, &topic).await;

    let registry = DeploymentRegistry::new();
    let tenant_id = Uuid::new_v4();
    let deployment_id = Uuid::new_v4();

    let consumer_registry = registry.clone();
    let consumer_brokers = brokers.clone();
    let consumer_topic = topic.clone();
    let consumer = tokio::spawn(async move {
        run_consumer(
            &consumer_brokers,
            "ingest-gateway-test",
            &consumer_topic,
            consumer_registry,
        )
        .await
    });

    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", &brokers)
        .set("message.timeout.ms", "5000")
        .create()
        .expect("producer created");

    let event = DeploymentMarkerEvent {
        deployment_id,
        tenant_id,
        service_name: "checkout".into(),
        environment: "production".into(),
        service_version: "v1.2.3".into(),
        status: "in_progress".into(),
        started_at_unix_nano: 1,
    };
    let payload = serde_json::to_vec(&event).expect("event serialises");
    producer
        .send(
            FutureRecord::to(&topic)
                .key(&tenant_id.to_string())
                .payload(&payload),
            Duration::from_secs(5),
        )
        .await
        .expect("event delivered");

    let mut result = String::new();
    for _ in 0..40 {
        result = registry
            .lookup(tenant_id, "checkout", "production", "v1.2.3")
            .await;
        if !result.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    consumer.abort();
    assert_eq!(
        result,
        deployment_id.to_string(),
        "consumer must apply the published event to the registry"
    );
}
