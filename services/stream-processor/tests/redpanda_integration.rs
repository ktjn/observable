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

#[tokio::test]
async fn queue_consumer_survives_broker_transport_loss_and_resumes_delivery() {
    let host_port = pick_free_port();
    let brokers = format!("127.0.0.1:{host_port}");
    let broker = GenericImage::new("redpandadata/redpanda", "v26.2.4")
        .with_wait_for(WaitFor::message_on_stderr("Successfully started Redpanda!"))
        .with_cmd(vec![
            "redpanda".to_string(),
            "start".to_string(),
            "--smp=1".to_string(),
            "--memory=512M".to_string(),
            "--overprovisioned".to_string(),
            "--kafka-addr=0.0.0.0:9092".to_string(),
            format!("--advertise-kafka-addr={brokers}"),
        ])
        .with_mapped_port(host_port, 9092_u16.tcp())
        .start()
        .await
        .expect("broker started");
    wait_for_kafka_ready(&brokers).await;
    let topic = format!("consumer-reconnect-{}", Uuid::new_v4());
    create_topic(&brokers, &topic).await;
    let consumer =
        stream_processor::consumer::QueueConsumer::new(&brokers, "reconnect-test", &topic)
            .expect("consumer created");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut tasks = tokio::task::JoinSet::new();
    tasks.spawn(async move {
        consumer
            .run_batch(1, Duration::from_millis(100), move |batch| {
                let tx = tx.clone();
                async move {
                    tx.send(batch).expect("test receiver alive");
                    Ok(())
                }
            })
            .await
    });
    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", &brokers)
        .set("message.timeout.ms", "5000")
        .create()
        .expect("producer created");
    let first = TelemetryEnvelope {
        envelope_id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        environment: "reconnect".into(),
        received_at_unix_nano: 0,
        payload: EnvelopePayload::Spans(vec![]),
    };
    let bytes = serde_json::to_vec(&first).expect("serialize first envelope");
    producer
        .send(
            FutureRecord::to(&topic).payload(&bytes).key("before"),
            Duration::from_secs(5),
        )
        .await
        .expect("baseline delivered to broker");
    let received = tokio::time::timeout(Duration::from_secs(15), rx.recv())
        .await
        .expect("baseline consumed within timeout")
        .expect("consumer channel open");
    assert_eq!(received[0].envelope_id, first.envelope_id);
    broker
        .stop_with_timeout(Some(2))
        .await
        .expect("broker stopped");
    let exited = tokio::time::timeout(Duration::from_secs(5), tasks.join_next()).await;
    assert!(
        exited.is_err(),
        "consumer must remain alive across broker transport loss: {exited:?}"
    );
    broker.start().await.expect("same broker restarted");
    wait_for_kafka_ready(&brokers).await;
    let second = TelemetryEnvelope {
        envelope_id: Uuid::new_v4(),
        ..first
    };
    let bytes = serde_json::to_vec(&second).expect("serialize second envelope");
    producer
        .send(
            FutureRecord::to(&topic).payload(&bytes).key("after"),
            Duration::from_secs(5),
        )
        .await
        .expect("recovery delivered to broker");
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let received = rx.recv().await.expect("consumer channel open");
            assert_eq!(received.len(), 1);
            assert_eq!(received[0].tenant_id, second.tenant_id);
            if received[0].envelope_id == second.envelope_id {
                break;
            }
            // Async offset commit may be lost during the outage; at-least-once replay is valid.
            assert_eq!(received[0].envelope_id, first.envelope_id);
        }
    })
    .await
    .expect("same consumer resumes after broker restart");
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
}
