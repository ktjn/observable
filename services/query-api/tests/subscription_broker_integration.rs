//! End-to-end proof that a provider event published to a real Kafka-compatible
//! broker reaches an open SSE subscription (PUSH-018), using the same
//! testcontainers-Redpanda scaffolding as the provider-side publish tests
//! (`health-service/tests/health_transition_publish_integration.rs`).
//!
//! The in-process fan-out itself is covered by the much faster
//! `tests/it/subscriptions_integration.rs`; what is only verifiable here is the
//! consumer wiring: topic subscription, payload parsing against the real wire
//! contract, and hand-off into the per-connection stream.

use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    routing::get,
};
use domain::DeploymentMarkerEvent;
use query_api::{
    middleware::auth::require_tenant,
    planner::QueryPlanner,
    review::ReviewProviders,
    subscriptions::{
        events::ProviderTopic,
        hub::{SubscriptionConfig, SubscriptionHub},
    },
    traces::AppState,
};
use rdkafka::ClientConfig;
use rdkafka::admin::{AdminClient, AdminOptions, NewTopic, TopicReplication};
use rdkafka::client::DefaultClientContext;
use rdkafka::consumer::{BaseConsumer, Consumer};
use rdkafka::producer::{FutureProducer, FutureRecord};
use serde_json::json;
use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};
use tokio_stream::StreamExt;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

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
    admin
        .create_topics(
            &[NewTopic::new(topic, 1, TopicReplication::Fixed(1))],
            &AdminOptions::default(),
        )
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
        let topic = format!("deployment-markers-sse-test-{}", Uuid::new_v4());
        create_topic(&brokers, &topic).await;
        Self {
            brokers,
            topic,
            _container: container,
        }
    }

    fn producer(&self) -> FutureProducer {
        ClientConfig::new()
            .set("bootstrap.servers", &self.brokers)
            .set("message.timeout.ms", "5000")
            .create()
            .expect("producer created")
    }
}

/// Publishes a `deployment.markers.v1` message exactly as admin-service's
/// `DeploymentEventProducer` does: the shared `domain` contract as JSON, keyed by
/// tenant so a tenant's events stay ordered on one partition.
async fn publish(producer: &FutureProducer, topic: &str, event: &DeploymentMarkerEvent) {
    let payload = serde_json::to_vec(event).expect("event serializes");
    let key = event.tenant_id.to_string();
    producer
        .send(
            FutureRecord::to(topic).key(&key).payload(&payload),
            Duration::from_secs(5),
        )
        .await
        .expect("message delivered to broker");
}

#[tokio::test]
async fn a_deployment_marker_published_to_the_broker_reaches_an_open_subscription() {
    let broker = Broker::start().await;
    let tenant = Uuid::new_v4();

    let auth = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user_id": Uuid::new_v4().to_string(),
            "tenant_id": tenant.to_string(),
            "role": "viewer",
        })))
        .mount(&auth)
        .await;

    let providers = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/internal/alerting-enrichment"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{ "service_name": "checkout", "active_alert_count": 0, "slo_breaching": false }],
        })))
        .mount(&providers)
        .await;
    Mock::given(method("GET"))
        .and(path("/internal/deployment-enrichment"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{ "service_name": "checkout", "latest_deployment": "v1.0.0" }],
        })))
        .mount(&providers)
        .await;

    let hub = SubscriptionHub::new(SubscriptionConfig::default());
    hub.spawn_consumer(
        &broker.brokers,
        ProviderTopic::DeploymentMarkers,
        &broker.topic,
    )
    .expect("consumer subscribed to the provider topic");

    let auth_url = auth.uri();
    let app = Router::new()
        .route(
            "/v1/ui/subscriptions/events",
            get(query_api::subscriptions::stream::subscribe_events),
        )
        .layer(axum::Extension(hub))
        .layer(axum::Extension(ReviewProviders::disabled()))
        .layer(axum::middleware::from_fn(require_tenant))
        .layer(axum::Extension(Arc::new(auth_url.clone())))
        .layer(axum::Extension(reqwest::Client::new()))
        .with_state(AppState {
            ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
            planner: Arc::new(QueryPlanner),
            llm: None,
            auth_service_url: auth_url,
            metrics: Arc::new(query_api::observability::QueryApiMetrics::new()),
            sessions: query_api::nlq_session::NlqSessionStore::default(),
            admin_service_url: providers.uri(),
            alert_evaluator_url: providers.uri(),
            internal_service_token: "test-internal-token".into(),
            http_client: reqwest::Client::new(),
        });

    let views = r#"[{"viewId":"status","query":{"kind":"service-status"}}]"#;
    let encoded: String = views
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect();

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/ui/subscriptions/events?views={encoded}"))
                .header(header::COOKIE, "session=valid-token")
                .header(header::ACCEPT, "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let mut stream = response.into_body().into_data_stream();
    let mut buffer = String::new();

    // Read the authorized snapshot first.
    let snapshot = read_frame(&mut stream, &mut buffer)
        .await
        .expect("a snapshot frame");
    assert!(
        snapshot.contains("event: snapshot") || snapshot.contains("event:snapshot"),
        "expected a snapshot first, got: {snapshot}"
    );

    // Consumers start at the log's end, so republish until the running consumer
    // has joined the group and observed one. Each attempt carries a newer
    // `started_at`, so the view's watermark accepts whichever arrives first.
    let producer = broker.producer();
    let delta = {
        let mut found = None;
        for attempt in 0..20u64 {
            publish(
                &producer,
                &broker.topic,
                &DeploymentMarkerEvent {
                    deployment_id: Uuid::new_v4(),
                    tenant_id: tenant,
                    service_name: "checkout".into(),
                    environment: "production".into(),
                    service_version: "v2.0.0".into(),
                    status: "success".into(),
                    started_at_unix_nano: 1_000 + attempt,
                },
            )
            .await;
            if let Ok(Some(frame)) =
                tokio::time::timeout(Duration::from_secs(2), read_frame(&mut stream, &mut buffer))
                    .await
            {
                found = Some(frame);
                break;
            }
        }
        found.expect("a broker-published deployment marker must reach the open subscription")
    };

    assert!(
        delta.contains("event: delta"),
        "expected a pass-through delta, got: {delta}"
    );
    assert!(
        delta.contains("\"latest_deployment\":\"v2.0.0\""),
        "the delta must carry the published version: {delta}"
    );
    assert!(
        delta.contains("\"viewId\":\"status\""),
        "the delta must be tagged with its view: {delta}"
    );
}

/// Reads bytes until one complete SSE frame (terminated by a blank line) is
/// available, returning it. Heartbeat comments are skipped.
async fn read_frame(
    stream: &mut axum::body::BodyDataStream,
    buffer: &mut String,
) -> Option<String> {
    loop {
        if let Some(split) = buffer.find("\n\n") {
            let raw = buffer[..split].to_string();
            *buffer = buffer[split + 2..].to_string();
            if raw.lines().any(|l| l.starts_with("event:")) {
                return Some(raw);
            }
            continue;
        }
        match stream.next().await {
            None | Some(Err(_)) => return None,
            Some(Ok(chunk)) => buffer.push_str(&String::from_utf8_lossy(&chunk)),
        }
    }
}
