//! Testcontainers-backed integration tests for `PUSH-016`
//! (`publish-telemetry-readonly-changes`): a burst of raw telemetry records
//! flushed by `storage-writer` is coalesced into at most one
//! `TelemetryWatermarkEvent` per `(tenant_id, signal)` and published through the
//! real Redpanda broker contract with no raw telemetry or PII disclosure
//! (PUSH-016-C1, PUSH-016-C2).

use domain::{LogRecord, MetricPoint, MetricSeries, Span, TelemetryWatermarkEvent};
use rdkafka::consumer::{BaseConsumer, Consumer, StreamConsumer};
use rdkafka::{
    ClientConfig, Message,
    admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
    client::DefaultClientContext,
};
use std::net::TcpListener;
use std::time::Duration;
use storage_writer::watermark::{
    TelemetryWatermarkProducer, WatermarkPublisher, coalesce_log_watermarks,
    coalesce_metric_watermarks, coalesce_span_watermarks, publish_watermarks_best_effort,
};
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
        let topic = format!("telemetry-watermarks-test-{}", Uuid::new_v4());
        create_topic(&brokers, &topic).await;
        Self {
            brokers,
            topic,
            _container: container,
        }
    }

    fn producer(&self) -> TelemetryWatermarkProducer {
        TelemetryWatermarkProducer::new(&self.brokers, &self.topic).expect("producer created")
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

async fn recv_event(
    consumer: &StreamConsumer,
    timeout: Duration,
) -> Option<TelemetryWatermarkEvent> {
    match tokio::time::timeout(timeout, consumer.recv()).await {
        Err(_) => None,
        Ok(msg) => {
            let msg = msg.expect("no kafka error");
            let bytes = msg.payload().expect("message has payload");
            Some(serde_json::from_slice(bytes).expect("event deserializes"))
        }
    }
}

/// PUSH-016-C1 & PUSH-016-C2: a burst of 500 spans, 300 logs, and 400 metric
/// points for a tenant coalesces into exactly 1 watermark event per signal
/// (3 events total across 1,200 raw records) and round-trips through a real
/// Redpanda broker carrying only tenant, signal, bounded service names, max
/// timestamp and coalesced count -- never raw span attributes or log bodies.
#[tokio::test]
async fn burst_of_telemetry_coalesces_to_single_watermark_per_signal_on_real_broker() {
    let broker = Broker::start().await;
    let producer = broker.producer();
    let consumer = broker.consumer();
    let tenant = Uuid::new_v4();

    let spans: Vec<Span> = (0..500u64)
        .map(|i| Span {
            tenant_id: tenant,
            service_name: if i % 2 == 0 {
                "checkout".into()
            } else {
                "payments".into()
            },
            start_time_unix_nano: 1_700_000_000_000_000_000 + i,
            end_time_unix_nano: 1_700_000_000_005_000_000 + i,
            ..Default::default()
        })
        .collect();

    let logs: Vec<LogRecord> = (0..300u64)
        .map(|i| LogRecord {
            tenant_id: tenant,
            service_name: "checkout".into(),
            timestamp_unix_nano: 1_700_000_000_010_000_000 + i,
            body: serde_json::json!({"secret": "never-disclosed-in-watermark"}),
            ..Default::default()
        })
        .collect();

    let series = vec![MetricSeries {
        tenant_id: tenant,
        service_name: "payments".into(),
        metric_name: "http.server.duration".into(),
        ..Default::default()
    }];
    let points: Vec<MetricPoint> = (0..400u64)
        .map(|i| MetricPoint {
            tenant_id: tenant,
            metric_name: "http.server.duration".into(),
            time_unix_nano: 1_700_000_000_020_000_000 + i,
            ..Default::default()
        })
        .collect();

    let span_wm = coalesce_span_watermarks(&spans);
    let log_wm = coalesce_log_watermarks(&logs);
    let metric_wm = coalesce_metric_watermarks(&series, &points);

    assert_eq!(span_wm.len(), 1);
    assert_eq!(log_wm.len(), 1);
    assert_eq!(metric_wm.len(), 1);

    let dyn_pub: &dyn WatermarkPublisher = &producer;
    publish_watermarks_best_effort(Some(dyn_pub), span_wm).await;
    publish_watermarks_best_effort(Some(dyn_pub), log_wm).await;
    publish_watermarks_best_effort(Some(dyn_pub), metric_wm).await;

    let first = recv_event(&consumer, Duration::from_secs(10))
        .await
        .expect("spans watermark received");
    assert_eq!(first.tenant_id, tenant);
    assert_eq!(first.signal, "spans");
    assert_eq!(first.coalesced_records, 500);
    assert_eq!(first.watermark_unix_nano, 1_700_000_000_005_000_499);
    assert_eq!(
        first.services,
        vec!["checkout".to_string(), "payments".to_string()]
    );

    let second = recv_event(&consumer, Duration::from_secs(10))
        .await
        .expect("logs watermark received");
    assert_eq!(second.tenant_id, tenant);
    assert_eq!(second.signal, "logs");
    assert_eq!(second.coalesced_records, 300);
    assert_eq!(second.watermark_unix_nano, 1_700_000_000_010_000_299);
    let raw_log_wm = serde_json::to_string(&second).unwrap();
    assert!(!raw_log_wm.contains("never-disclosed-in-watermark"));

    let third = recv_event(&consumer, Duration::from_secs(10))
        .await
        .expect("metrics watermark received");
    assert_eq!(third.tenant_id, tenant);
    assert_eq!(third.signal, "metrics");
    assert_eq!(third.coalesced_records, 400);
    assert_eq!(third.watermark_unix_nano, 1_700_000_000_020_000_399);
    assert_eq!(third.services, vec!["payments".to_string()]);

    // No fourth event: 1,200 raw records coalesced to 3 signal watermarks.
    assert!(
        recv_event(&consumer, Duration::from_secs(1))
            .await
            .is_none()
    );
}
