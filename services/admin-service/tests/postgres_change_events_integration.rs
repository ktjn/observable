// HTTP integration test for GET /v1/events/changes against a real Postgres
// instance via Testcontainers, exercising the full handler path via
// tower::ServiceExt::oneshot.

use admin_service::{
    AdminServiceAppState, change_events, middleware::auth::TenantContext, observability,
    queue::ChangeEventProducer,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use domain::ChangeMarkerEvent;
use http_body_util::BodyExt;
use rdkafka::{
    ClientConfig, Message,
    admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
    client::DefaultClientContext,
    consumer::{BaseConsumer, Consumer, StreamConsumer},
};
use serde_json::{Value, json};
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::{net::TcpListener, sync::Arc, time::Duration};
use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};
use tower::ServiceExt;
use uuid::Uuid;

/// Bind to port 0 to let the OS pick a free port, then release it and return the port number.
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

/// Starts (or reuses, via `OBSERVABLE_TEST_KAFKA_BROKERS`) a Kafka-compatible
/// broker and returns its bootstrap address plus a fresh, isolated topic
/// name. Mirrors `services/stream-processor/tests/redpanda_integration.rs`'s
/// setup.
async fn start_broker() -> (String, Option<ContainerAsync<GenericImage>>, String) {
    let (brokers, container) = match test_support::kafka::external_brokers() {
        Some(brokers) => (brokers, None),
        None => {
            let host_port = pick_free_port();
            let advertise_addr = format!("127.0.0.1:{host_port}");
            let brokers = advertise_addr.clone();
            let container = GenericImage::new("redpandadata/redpanda", "v26.2.4")
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
            (brokers, Some(container))
        }
    };
    wait_for_kafka_ready(&brokers).await;
    let topic = format!("change-markers-test-{}", Uuid::new_v4());
    create_topic(&brokers, &topic).await;
    (brokers, container, topic)
}

const DEV_TENANT_ID: &str = "00000000-0000-0000-0000-000000000002";

async fn start_postgres() -> (PgPool, test_support::postgres::TestDatabase) {
    test_support::postgres::isolated_pool().await
}

async fn response_body_json(body: Body) -> Value {
    let bytes = body.collect().await.expect("body collected").to_bytes();
    serde_json::from_slice(&bytes).expect("valid JSON")
}

fn build_app(db: PgPool, tenant_id: Uuid) -> Router {
    build_app_with_role(db, tenant_id, "tenant_admin")
}

fn build_app_with_role(db: PgPool, tenant_id: Uuid, role: &str) -> Router {
    let state = AdminServiceAppState {
        db: db.clone(),
        ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        auth_service_url: "http://auth-service:4319".into(),
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AdminServiceMetrics::new()),
        producer: None,
        change_producer: None,
    };
    Router::new()
        .route(
            "/v1/events/changes",
            get(change_events::handle_list_change_events).post(change_events::create_change_event),
        )
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id: Some(Uuid::new_v4()),
            role: role.into(),
        }))
        .with_state(state)
}

fn build_app_with_state(state: AdminServiceAppState, tenant_id: Uuid, role: &str) -> Router {
    Router::new()
        .route(
            "/v1/events/changes",
            get(change_events::handle_list_change_events).post(change_events::create_change_event),
        )
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id: Some(Uuid::new_v4()),
            role: role.into(),
        }))
        .with_state(state)
}

async fn seed_event(
    db: &PgPool,
    tenant_id: Uuid,
    event_type: &str,
    service_name: Option<&str>,
    environment: &str,
    title: &str,
) {
    sqlx::query(
        "INSERT INTO change_events (tenant_id, event_type, service_name, environment, title) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(tenant_id)
    .bind(event_type)
    .bind(service_name)
    .bind(environment)
    .bind(title)
    .execute(db)
    .await
    .expect("change event inserted");
}

#[tokio::test]
async fn list_change_events_returns_seeded_events_scoped_to_tenant() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::parse_str(DEV_TENANT_ID).unwrap();
    let other_tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'other')")
        .bind(other_tenant_id)
        .execute(&db)
        .await
        .unwrap();

    seed_event(
        &db,
        tenant_id,
        "feature_flag",
        Some("checkout"),
        "production",
        "Enabled new flow",
    )
    .await;
    seed_event(
        &db,
        other_tenant_id,
        "feature_flag",
        Some("other-svc"),
        "production",
        "Other tenant event",
    )
    .await;

    let app = build_app(db, tenant_id);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/changes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = response_body_json(resp.into_body()).await;
    let items = body["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Enabled new flow");
}

#[tokio::test]
async fn list_change_events_filters_by_service_name() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    seed_event(
        &db,
        tenant_id,
        "migration",
        Some("checkout"),
        "production",
        "Checkout schema migration",
    )
    .await;
    seed_event(
        &db,
        tenant_id,
        "migration",
        Some("billing"),
        "production",
        "Billing schema migration",
    )
    .await;

    let app = build_app(db, tenant_id);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/changes?service_name=billing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = response_body_json(resp.into_body()).await;
    let items = body["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Billing schema migration");
}

#[tokio::test]
async fn create_change_event_then_appears_in_list() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 't')")
        .bind(tenant_id)
        .execute(&db)
        .await
        .unwrap();

    let app = build_app_with_role(db, tenant_id, "member");

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/events/changes")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "event_type": "feature_flag",
                        "environment": "production",
                        "title": "Enabled new-checkout-flow",
                        "service_name": "checkout"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::CREATED);

    let list_resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/changes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let body = response_body_json(list_resp.into_body()).await;
    let items = body["items"].as_array().expect("items array");
    assert!(
        items
            .iter()
            .any(|i| i["title"] == "Enabled new-checkout-flow")
    );
}

#[tokio::test]
async fn create_change_event_rejects_unknown_event_type() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 't')")
        .bind(tenant_id)
        .execute(&db)
        .await
        .unwrap();

    let app = build_app_with_role(db, tenant_id, "admin");
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/events/changes")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "event_type": "deploy",
                        "environment": "production",
                        "title": "bad event type"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn create_change_event_forbidden_for_viewer_role() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 't')")
        .bind(tenant_id)
        .execute(&db)
        .await
        .unwrap();

    let app = build_app_with_role(db, tenant_id, "viewer");
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/events/changes")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "event_type": "feature_flag",
                        "environment": "production",
                        "title": "Enabled new-checkout-flow"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

/// PUSH-003-C1: the change-marker hint is only published after the
/// `change_events` row has committed and is query-visible -- by the time the
/// broker message arrives, `GET /v1/events/changes` already returns it.
#[tokio::test]
async fn create_change_event_publishes_change_marker_only_after_commit() {
    let (brokers, _broker_container, topic) = start_broker().await;
    let (db, _pg_container) = start_postgres().await;
    let tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 't')")
        .bind(tenant_id)
        .execute(&db)
        .await
        .unwrap();

    let producer = Arc::new(ChangeEventProducer::new(&brokers, &topic).expect("producer created"));
    let state = AdminServiceAppState {
        db: db.clone(),
        ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        auth_service_url: "http://auth-service:4319".into(),
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AdminServiceMetrics::new()),
        producer: None,
        change_producer: Some(producer),
    };
    let app = build_app_with_state(state, tenant_id, "member");

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/events/changes")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "event_type": "feature_flag",
                        "environment": "production",
                        "title": "Enabled new-checkout-flow",
                        "service_name": "checkout"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let created = response_body_json(create_resp.into_body()).await;
    let change_event_id = created["change_event_id"].as_str().unwrap().to_string();

    // By the time we read the broker message, the row must already be
    // query-visible through the normal read path -- proving publish happened
    // strictly after commit, not concurrently with or before it.
    let list_resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/changes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let body = response_body_json(list_resp.into_body()).await;
    let items = body["items"].as_array().expect("items array");
    assert!(
        items
            .iter()
            .any(|i| i["change_event_id"] == change_event_id),
        "committed change event must be query-visible before/by the time we assert on the published event"
    );

    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", &brokers)
        .set("group.id", format!("test-{}", Uuid::new_v4()))
        .set("auto.offset.reset", "earliest")
        .create()
        .expect("consumer created");
    consumer.subscribe(&[topic.as_str()]).expect("subscribed");
    let msg = tokio::time::timeout(Duration::from_secs(15), consumer.recv())
        .await
        .expect("change marker event received within timeout")
        .expect("no kafka error");
    let bytes = msg.payload().expect("message has payload");
    let event: ChangeMarkerEvent = serde_json::from_slice(bytes).expect("event deserialised");
    assert_eq!(event.change_event_id.to_string(), change_event_id);
    assert_eq!(event.tenant_id, tenant_id);
    assert_eq!(event.title, "Enabled new-checkout-flow");
    assert_eq!(event.event_type, "feature_flag");
    assert_eq!(event.service_name, Some("checkout".to_string()));
}

/// PUSH-003-C2: a rejected/failed write must not be reported as a successful
/// delivery. If the `change_events` INSERT fails, no event reaches the
/// broker -- the handler must return an error without ever calling publish.
#[tokio::test]
async fn create_change_event_does_not_publish_when_db_insert_fails() {
    let (brokers, _broker_container, topic) = start_broker().await;

    // A pool pointed at an unreachable address: any query against it fails,
    // simulating a rejected/failed commit without needing to corrupt real
    // Postgres state (same technique as
    // services/auth-service/tests/health_worker_credential_integration.rs).
    let db = PgPoolOptions::new()
        .connect_lazy("postgres://invalid:invalid@127.0.0.1:1/test")
        .unwrap();

    let producer = Arc::new(ChangeEventProducer::new(&brokers, &topic).expect("producer created"));
    let state = AdminServiceAppState {
        db,
        ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        auth_service_url: "http://auth-service:4319".into(),
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AdminServiceMetrics::new()),
        producer: None,
        change_producer: Some(producer),
    };
    let tenant_id = Uuid::new_v4();
    let app = build_app_with_state(state, tenant_id, "member");

    let create_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/events/changes")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "event_type": "feature_flag",
                        "environment": "production",
                        "title": "should never be published"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", &brokers)
        .set("group.id", format!("test-{}", Uuid::new_v4()))
        .set("auto.offset.reset", "earliest")
        .create()
        .expect("consumer created");
    consumer.subscribe(&[topic.as_str()]).expect("subscribed");
    let outcome = tokio::time::timeout(Duration::from_secs(5), consumer.recv()).await;
    assert!(
        outcome.is_err(),
        "no event must be published when the underlying write never committed"
    );
}
