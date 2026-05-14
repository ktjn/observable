use domain::{EnvelopePayload, TelemetryEnvelope};
use rdkafka::{
    ClientConfig, Message,
    admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
    client::DefaultClientContext,
    consumer::{BaseConsumer, Consumer, StreamConsumer},
    producer::{FutureProducer, FutureRecord},
};
use std::{net::TcpListener, time::Duration};
use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};
use uuid::Uuid;

/// Bind to port 0 to let the OS pick a free port, then release it and return the port number.
/// There is a small TOCTOU window, but it is acceptable for local integration tests.
fn pick_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind to 0 to get free port");
    listener.local_addr().unwrap().port()
}

/// Poll the Kafka metadata API until the broker is accepting connections (up to 15 s).
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

/// Create a topic via the Kafka admin API.
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

#[tokio::test]
async fn redpanda_container_preserves_tenant_id_and_payload_across_queue_boundary() {
    // Pick a free host port before starting the container so we can tell Redpanda
    // to advertise exactly that address. This keeps metadata discovery working for
    // the single-node setup: rdkafka will follow the advertised broker address from
    // the metadata response, which must resolve back to the container's Kafka port.
    let host_port = pick_free_port();
    let advertise_addr = format!("127.0.0.1:{host_port}");
    let brokers = advertise_addr.clone();

    let _container: ContainerAsync<GenericImage> =
        GenericImage::new("redpandadata/redpanda", "v23.3.1")
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

    let topic = format!("test-{}", Uuid::new_v4());
    create_topic(&brokers, &topic).await;

    // --- Build and serialise a TelemetryEnvelope ---
    let tenant_id = Uuid::new_v4();
    let envelope_id = Uuid::new_v4();
    let envelope = TelemetryEnvelope {
        envelope_id,
        tenant_id,
        environment: "testbench".to_string(),
        received_at_unix_nano: 1_700_000_000_000_000_000_u64,
        payload: EnvelopePayload::Spans(vec![]),
    };
    let payload_bytes = serde_json::to_vec(&envelope).expect("envelope serialised");

    // --- Produce ---
    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", &brokers)
        .set("message.timeout.ms", "5000")
        .create()
        .expect("producer created");

    producer
        .send(
            FutureRecord::to(&topic).payload(&payload_bytes).key("key"),
            Duration::from_secs(5),
        )
        .await
        .expect("message delivered");

    // --- Consume using the same ClientConfig as QueueConsumer::new ---
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", &brokers)
        .set("group.id", "test-group")
        .set("auto.offset.reset", "earliest")
        .create()
        .expect("consumer created");
    consumer.subscribe(&[topic.as_str()]).expect("subscribed");

    let msg = tokio::time::timeout(Duration::from_secs(15), consumer.recv())
        .await
        .expect("message received within timeout")
        .expect("no kafka error");

    let received_bytes = msg.payload().expect("message has payload");
    let received: TelemetryEnvelope =
        serde_json::from_slice(received_bytes).expect("envelope deserialised");

    // Assertions: tenant_id and envelope_id must survive the queue round-trip
    assert_eq!(
        received.tenant_id, tenant_id,
        "tenant_id must survive the queue round-trip"
    );
    assert_eq!(
        received.envelope_id, envelope_id,
        "envelope_id must be preserved across the queue boundary"
    );
    assert_eq!(
        received_bytes.len(),
        payload_bytes.len(),
        "serialised payload length must be identical"
    );
}
