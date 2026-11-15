//! Integration tests for `GET /v1/ui/subscriptions/events` against the real axum
//! router, the real auth middleware and real provider HTTP endpoints (wiremock).
//!
//! Covers PUSH-002 (single-call register-and-stream, cookie auth), PUSH-006
//! (replay/reset), PUSH-007 (per-batch session revalidation, fail closed),
//! PUSH-017 (ephemeral per-connection context), PUSH-018 (snapshot/delta/replace)
//! and PUSH-020 (fixed-size queue, disconnect on overflow).

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
        events::{AlertStateChangeEvent, HealthTransitionEvent, ProviderEvent},
        hub::{SubscriptionConfig, SubscriptionHub},
    },
    traces::AppState,
};
use serde_json::{Value, json};
use tokio_stream::StreamExt;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, path_regex},
};

/// One parsed SSE frame.
#[derive(Debug, Clone, PartialEq)]
struct Frame {
    id: Option<String>,
    event: String,
    data: Value,
}

/// Incrementally reads SSE frames off a streaming response body. A long-lived SSE
/// body never ends, so every read is bounded by a timeout and the stream's
/// termination is itself an assertable outcome.
struct SseReader {
    stream: axum::body::BodyDataStream,
    buffer: String,
    ended: bool,
}

impl SseReader {
    fn new(body: Body) -> Self {
        Self {
            stream: body.into_data_stream(),
            buffer: String::new(),
            ended: false,
        }
    }

    fn take_frame(&mut self) -> Option<Frame> {
        loop {
            let split = self.buffer.find("\n\n")?;
            let raw = self.buffer[..split].to_string();
            self.buffer = self.buffer[split + 2..].to_string();

            let mut id = None;
            let mut event = None;
            let mut data = String::new();
            for line in raw.lines() {
                if let Some(rest) = line.strip_prefix("id:") {
                    id = Some(rest.trim().to_string());
                } else if let Some(rest) = line.strip_prefix("event:") {
                    event = Some(rest.trim().to_string());
                } else if let Some(rest) = line.strip_prefix("data:") {
                    data.push_str(rest.trim());
                }
                // A leading ':' is a heartbeat comment; it carries no frame.
            }
            if let Some(event) = event {
                return Some(Frame {
                    id,
                    event,
                    data: serde_json::from_str(&data).unwrap_or(Value::Null),
                });
            }
        }
    }

    /// The next frame, or `None` if the stream ended or went quiet.
    async fn next_frame(&mut self) -> Option<Frame> {
        if let Some(frame) = self.take_frame() {
            return Some(frame);
        }
        loop {
            match tokio::time::timeout(Duration::from_secs(10), self.stream.next()).await {
                Err(_) => return None,
                Ok(None) => {
                    self.ended = true;
                    return self.take_frame();
                }
                Ok(Some(Err(_))) => {
                    self.ended = true;
                    return self.take_frame();
                }
                Ok(Some(Ok(chunk))) => {
                    self.buffer.push_str(&String::from_utf8_lossy(&chunk));
                    if let Some(frame) = self.take_frame() {
                        return Some(frame);
                    }
                }
            }
        }
    }

    /// Drives the stream until it terminates, returning the frames seen. Fails the
    /// calling test if the stream is still open after `within`.
    async fn drain_until_closed(&mut self, within: Duration) -> Vec<Frame> {
        let mut frames = Vec::new();
        let deadline = tokio::time::Instant::now() + within;
        loop {
            if let Some(frame) = self.take_frame() {
                frames.push(frame);
                continue;
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                panic!("stream did not close within {within:?}; frames so far: {frames:?}");
            }
            match tokio::time::timeout(remaining, self.stream.next()).await {
                Err(_) => {
                    panic!("stream did not close within {within:?}; frames so far: {frames:?}")
                }
                Ok(None) | Ok(Some(Err(_))) => {
                    while let Some(frame) = self.take_frame() {
                        frames.push(frame);
                    }
                    return frames;
                }
                Ok(Some(Ok(chunk))) => self.buffer.push_str(&String::from_utf8_lossy(&chunk)),
            }
        }
    }
}

/// The providers a view's query path reads, each a real HTTP endpoint.
struct Harness {
    app: Router,
    hub: SubscriptionHub,
    tenant: Uuid,
    _auth: MockServer,
    _providers: MockServer,
}

/// Session validation that always succeeds for `tenant`.
async fn auth_server(tenant: Uuid) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user_id": Uuid::new_v4().to_string(),
            "tenant_id": tenant.to_string(),
            "role": "viewer",
        })))
        .mount(&server)
        .await;
    server
}

/// Alert, deployment and health reads, as the real services expose them.
async fn provider_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/internal/alerting-enrichment"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{ "service_name": "checkout", "active_alert_count": 2, "slo_breaching": true }],
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/internal/deployment-enrichment"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{ "service_name": "checkout", "latest_deployment": "v1.0.0" }],
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/v1/health/services/[0-9a-f-]+$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "state": "normal",
            "evidence_status": "current",
            "evaluated_at": "2026-11-15T00:00:00Z",
            "last_evidence_at": "2026-11-15T00:00:00Z",
            "freshness_seconds": 300,
        })))
        .mount(&server)
        .await;
    server
}

async fn harness_with(config: SubscriptionConfig, auth: MockServer) -> Harness {
    let tenant = Uuid::new_v4();
    let providers = provider_server().await;
    let auth_url = auth.uri();

    let state = AppState {
        ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        planner: Arc::new(QueryPlanner),
        llm: None,
        auth_service_url: auth_url.clone(),
        metrics: Arc::new(query_api::observability::QueryApiMetrics::new()),
        sessions: query_api::nlq_session::NlqSessionStore::default(),
        admin_service_url: providers.uri(),
        alert_evaluator_url: providers.uri(),
        internal_service_token: "test-internal-token".into(),
        http_client: reqwest::Client::new(),
    };
    let hub = SubscriptionHub::new(config);
    let review = ReviewProviders::http(
        providers.uri(),
        providers.uri(),
        providers.uri(),
        reqwest::Client::new(),
    );

    let app = Router::new()
        .route(
            "/v1/ui/subscriptions/events",
            get(query_api::subscriptions::stream::subscribe_events),
        )
        .layer(axum::Extension(hub.clone()))
        .layer(axum::Extension(review))
        .layer(axum::middleware::from_fn(require_tenant))
        .layer(axum::Extension(Arc::new(auth_url)))
        .layer(axum::Extension(reqwest::Client::new()))
        .with_state(state);

    Harness {
        app,
        hub,
        tenant,
        _auth: auth,
        _providers: providers,
    }
}

async fn harness() -> Harness {
    let tenant = Uuid::new_v4();
    let auth = auth_server(tenant).await;
    let mut h = harness_with(SubscriptionConfig::default(), auth).await;
    h.tenant = tenant;
    h
}

impl Harness {
    /// Opens the stream with a session cookie and the given `views` parameter.
    async fn open(&self, views: &str) -> axum::response::Response {
        self.request(views, None).await
    }

    async fn request(&self, views: &str, last_event_id: Option<&str>) -> axum::response::Response {
        let uri = format!("/v1/ui/subscriptions/events?views={}", urlencode(views));
        let mut builder = Request::builder()
            .uri(uri)
            .header(header::COOKIE, "session=valid-token")
            .header(header::ACCEPT, "text/event-stream");
        if let Some(id) = last_event_id {
            builder = builder.header("last-event-id", id);
        }
        self.app
            .clone()
            .oneshot(builder.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    fn deployment(&self, service: &str, version: &str, started_at: u64) -> ProviderEvent {
        ProviderEvent::Deployment(DeploymentMarkerEvent {
            deployment_id: Uuid::new_v4(),
            tenant_id: self.tenant,
            service_name: service.into(),
            environment: "production".into(),
            service_version: version.into(),
            status: "success".into(),
            started_at_unix_nano: started_at,
        })
    }

    fn alert(&self, tenant: Uuid) -> ProviderEvent {
        ProviderEvent::AlertState(AlertStateChangeEvent {
            tenant_id: tenant,
            incident_id: Uuid::new_v4(),
            status: "firing".into(),
            severity: "critical".into(),
            event_type: "alert_fired".into(),
            occurred_at_unix_nano: 1,
        })
    }
}

fn urlencode(raw: &str) -> String {
    raw.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

const STATUS_VIEW: &str =
    r#"[{"viewId":"status","query":{"kind":"service-status","environment":"production"}}]"#;

fn health_view(service_id: Uuid) -> String {
    format!(
        r#"[{{"viewId":"health","query":{{"kind":"service-health","serviceId":"{service_id}","environmentClass":"production"}}}}]"#
    )
}

// ---------------------------------------------------------------- registration

#[tokio::test]
async fn a_request_without_a_session_never_establishes_a_stream() {
    let h = harness().await;
    let response = h
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/ui/subscriptions/events?views={}",
                    urlencode(STATUS_VIEW)
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_invalid_session_is_rejected_before_the_stream_opens() {
    let auth = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&auth)
        .await;
    let h = harness_with(SubscriptionConfig::default(), auth).await;
    assert_eq!(h.open(STATUS_VIEW).await.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_identity_outage_fails_closed_with_503() {
    // An auth-service that cannot be reached at all: the stream must never open.
    // Port 1 is never a listening auth-service, so the call fails to connect
    // rather than returning a status.
    let uri = "http://127.0.0.1:1".to_string();
    let hub = SubscriptionHub::new(SubscriptionConfig::default());
    let app = Router::new()
        .route(
            "/v1/ui/subscriptions/events",
            get(query_api::subscriptions::stream::subscribe_events),
        )
        .layer(axum::Extension(hub))
        .layer(axum::Extension(ReviewProviders::disabled()))
        .layer(axum::middleware::from_fn(require_tenant))
        .layer(axum::Extension(Arc::new(uri)))
        .layer(axum::Extension(reqwest::Client::new()))
        .with_state(
            // State is irrelevant: the middleware rejects first.
            AppState {
                ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
                planner: Arc::new(QueryPlanner),
                llm: None,
                auth_service_url: "http://127.0.0.1:9".into(),
                metrics: Arc::new(query_api::observability::QueryApiMetrics::new()),
                sessions: query_api::nlq_session::NlqSessionStore::default(),
                admin_service_url: "http://127.0.0.1:9".into(),
                alert_evaluator_url: "http://127.0.0.1:9".into(),
                internal_service_token: "t".into(),
                http_client: reqwest::Client::new(),
            },
        );
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/ui/subscriptions/events?views={}",
                    urlencode(STATUS_VIEW)
                ))
                .header(header::COOKIE, "session=valid-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn a_missing_views_parameter_is_a_bad_request() {
    let h = harness().await;
    let response = h
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/ui/subscriptions/events")
                .header(header::COOKIE, "session=valid-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_disallowed_query_kind_is_a_bad_request() {
    let h = harness().await;
    let response = h
        .open(r#"[{"viewId":"v","query":{"kind":"raw-sql","sql":"SELECT 1"}}]"#)
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_json(response).await;
    assert_eq!(body["error"], json!("invalid_request"));
}

#[tokio::test]
async fn a_pinned_historical_view_is_refused_rather_than_streamed() {
    let h = harness().await;
    let response = h
        .open(
            r#"[{"viewId":"v","query":{"kind":"service-status"},"timeRange":{"mode":"fixed","from":"2026-01-01T00:00:00Z"}}]"#,
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_view_whose_provider_is_not_deployed_is_unavailable_not_empty() {
    let tenant = Uuid::new_v4();
    let auth = auth_server(tenant).await;
    let auth_url = auth.uri();
    let hub = SubscriptionHub::new(SubscriptionConfig::default());
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
            admin_service_url: "http://127.0.0.1:9".into(),
            alert_evaluator_url: "http://127.0.0.1:9".into(),
            internal_service_token: "t".into(),
            http_client: reqwest::Client::new(),
        });

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/ui/subscriptions/events?views={}",
                    urlencode(&health_view(Uuid::new_v4()))
                ))
                .header(header::COOKIE, "session=valid-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn the_process_connection_ceiling_refuses_further_streams() {
    let tenant = Uuid::new_v4();
    let auth = auth_server(tenant).await;
    let h = harness_with(
        SubscriptionConfig {
            max_connections: 1,
            ..SubscriptionConfig::default()
        },
        auth,
    )
    .await;

    let first = h.open(STATUS_VIEW).await;
    assert_eq!(first.status(), StatusCode::OK);
    let second = h.open(STATUS_VIEW).await;
    assert_eq!(second.status(), StatusCode::SERVICE_UNAVAILABLE);
}

// -------------------------------------------------------------------- snapshot

#[tokio::test]
async fn an_authorized_view_receives_a_snapshot_with_streaming_headers() {
    let h = harness().await;
    let response = h.open(STATUS_VIEW).await;
    assert_eq!(response.status(), StatusCode::OK);

    let headers = response.headers();
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "text/event-stream"
    );
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
    assert_eq!(headers.get("x-accel-buffering").unwrap(), "no");

    let mut reader = SseReader::new(response.into_body());
    let frame = reader.next_frame().await.expect("a snapshot frame");
    assert_eq!(frame.event, "snapshot");
    assert!(
        frame.id.is_some(),
        "a data event must carry an id for resume"
    );
    assert_eq!(frame.data["v"], json!(1));
    assert_eq!(frame.data["viewId"], json!("status"));
    assert_eq!(frame.data["revision"], json!("1"));
    assert!(
        frame.data["queryRevision"]
            .as_str()
            .unwrap()
            .starts_with('r')
    );
    // The snapshot is the authorized merged read of both providers.
    assert_eq!(
        frame.data["data"]["checkout"]["active_alert_count"],
        json!(2)
    );
    assert_eq!(frame.data["data"]["checkout"]["slo_breaching"], json!(true));
    assert_eq!(
        frame.data["data"]["checkout"]["latest_deployment"],
        json!("v1.0.0")
    );
}

#[tokio::test]
async fn a_service_health_view_snapshots_the_authorized_provider_read() {
    let h = harness().await;
    let response = h.open(&health_view(Uuid::new_v4())).await;
    assert_eq!(response.status(), StatusCode::OK);

    let mut reader = SseReader::new(response.into_body());
    let frame = reader.next_frame().await.expect("a snapshot frame");
    assert_eq!(frame.event, "snapshot");
    assert_eq!(frame.data["viewId"], json!("health"));
    assert_eq!(frame.data["data"]["state"], json!("normal"));
    assert_eq!(frame.data["data"]["evidence_status"], json!("current"));
}

#[tokio::test]
async fn each_registered_view_gets_its_own_snapshot() {
    let h = harness().await;
    let id = Uuid::new_v4();
    let views = format!(
        r#"[{{"viewId":"status","query":{{"kind":"service-status"}}}},{{"viewId":"health","query":{{"kind":"service-health","serviceId":"{id}","environmentClass":"production"}}}}]"#
    );
    let response = h.open(&views).await;
    assert_eq!(response.status(), StatusCode::OK);

    let mut reader = SseReader::new(response.into_body());
    let first = reader.next_frame().await.expect("first snapshot");
    let second = reader.next_frame().await.expect("second snapshot");
    let mut ids = vec![
        first.data["viewId"].as_str().unwrap().to_string(),
        second.data["viewId"].as_str().unwrap().to_string(),
    ];
    ids.sort();
    assert_eq!(ids, vec!["health", "status"]);
    assert_eq!(first.event, "snapshot");
    assert_eq!(second.event, "snapshot");
}

// ------------------------------------------------------------ delta / replace

#[tokio::test]
async fn a_deployment_marker_arrives_as_a_passed_through_delta() {
    let h = harness().await;
    let response = h.open(STATUS_VIEW).await;
    let mut reader = SseReader::new(response.into_body());
    assert_eq!(reader.next_frame().await.unwrap().event, "snapshot");

    h.hub.publish(h.deployment("checkout", "v2.0.0", 100));

    let frame = reader.next_frame().await.expect("a delta frame");
    assert_eq!(frame.event, "delta");
    assert_eq!(frame.data["viewId"], json!("status"));
    // Monotonic per-view result revision, after the snapshot's "1".
    assert_eq!(frame.data["revision"], json!("2"));
    assert_eq!(frame.data["changes"][0]["op"], json!("upsert"));
    assert_eq!(frame.data["changes"][0]["key"], json!("checkout"));
    assert_eq!(
        frame.data["changes"][0]["value"]["latest_deployment"],
        json!("v2.0.0")
    );
}

#[tokio::test]
async fn an_alert_state_change_arrives_as_an_authoritative_replace() {
    let h = harness().await;
    let response = h.open(STATUS_VIEW).await;
    let mut reader = SseReader::new(response.into_body());
    assert_eq!(reader.next_frame().await.unwrap().event, "snapshot");

    h.hub.publish(h.alert(h.tenant));

    let frame = reader.next_frame().await.expect("a replace frame");
    assert_eq!(frame.event, "replace");
    assert_eq!(frame.data["viewId"], json!("status"));
    assert_eq!(frame.data["revision"], json!("2"));
    // A replace carries the whole re-read result, not a change list.
    assert_eq!(
        frame.data["data"]["checkout"]["active_alert_count"],
        json!(2)
    );
    assert!(frame.data.get("changes").is_none());
}

#[tokio::test]
async fn a_health_transition_replaces_the_health_view() {
    let h = harness().await;
    let service_id = Uuid::new_v4();
    let response = h.open(&health_view(service_id)).await;
    let mut reader = SseReader::new(response.into_body());
    assert_eq!(reader.next_frame().await.unwrap().event, "snapshot");

    h.hub.publish(ProviderEvent::Health(HealthTransitionEvent {
        tenant_id: h.tenant,
        service_id,
        environment_class: "production".into(),
        state: "degraded".into(),
        evidence_status: "current".into(),
        evaluated_at_unix_nano: 1,
    }));

    let frame = reader.next_frame().await.expect("a replace frame");
    assert_eq!(frame.event, "replace");
    assert_eq!(frame.data["viewId"], json!("health"));
    // The value comes from the re-read authorized provider call, not the event.
    assert_eq!(frame.data["data"]["state"], json!("normal"));
}

#[tokio::test]
async fn a_duplicate_deployment_marker_does_not_emit_a_second_delta() {
    let h = harness().await;
    let response = h.open(STATUS_VIEW).await;
    let mut reader = SseReader::new(response.into_body());
    assert_eq!(reader.next_frame().await.unwrap().event, "snapshot");

    let event = h.deployment("checkout", "v2.0.0", 100);
    h.hub.publish(event.clone());
    assert_eq!(
        reader.next_frame().await.unwrap().data["revision"],
        json!("2")
    );

    // At-least-once delivery means the same marker can arrive twice.
    h.hub.publish(event);
    // Followed by a genuinely newer one, which must still be delivered.
    h.hub.publish(h.deployment("checkout", "v3.0.0", 200));

    let frame = reader.next_frame().await.expect("the newer delta");
    assert_eq!(
        frame.data["changes"][0]["value"]["latest_deployment"],
        json!("v3.0.0")
    );
    assert_eq!(
        frame.data["revision"],
        json!("3"),
        "the duplicate must not have consumed a revision"
    );
}

#[tokio::test]
async fn an_out_of_order_deployment_marker_never_regresses_the_view() {
    let h = harness().await;
    let response = h.open(STATUS_VIEW).await;
    let mut reader = SseReader::new(response.into_body());
    reader.next_frame().await.unwrap();

    h.hub.publish(h.deployment("checkout", "v3.0.0", 200));
    assert_eq!(
        reader.next_frame().await.unwrap().data["changes"][0]["value"]["latest_deployment"],
        json!("v3.0.0")
    );

    // An older marker arriving late must not overwrite the newer version.
    h.hub.publish(h.deployment("checkout", "v1.0.0", 50));
    h.hub.publish(h.deployment("payments", "v9.0.0", 300));

    let frame = reader.next_frame().await.expect("the next accepted delta");
    assert_eq!(frame.data["changes"][0]["key"], json!("payments"));
}

#[tokio::test]
async fn an_event_for_another_tenant_delivers_nothing() {
    let h = harness().await;
    let response = h.open(STATUS_VIEW).await;
    let mut reader = SseReader::new(response.into_body());
    assert_eq!(reader.next_frame().await.unwrap().event, "snapshot");

    h.hub.publish(h.alert(Uuid::new_v4()));
    h.hub
        .publish(ProviderEvent::Deployment(DeploymentMarkerEvent {
            deployment_id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            service_name: "checkout".into(),
            environment: "production".into(),
            service_version: "leaked".into(),
            status: "success".into(),
            started_at_unix_nano: 500,
        }));

    // Then an event that *is* ours, proving the stream is alive and that only the
    // cross-tenant events were dropped.
    h.hub.publish(h.deployment("checkout", "v2.0.0", 600));
    let frame = reader.next_frame().await.expect("our own delta");
    assert_eq!(
        frame.data["changes"][0]["value"]["latest_deployment"],
        json!("v2.0.0")
    );
    assert_eq!(
        frame.data["revision"],
        json!("2"),
        "cross-tenant events must not advance this view's revision"
    );
}

#[tokio::test]
async fn an_unrelated_service_event_does_not_touch_a_health_view() {
    let h = harness().await;
    let subscribed = Uuid::new_v4();
    let response = h.open(&health_view(subscribed)).await;
    let mut reader = SseReader::new(response.into_body());
    reader.next_frame().await.unwrap();

    // A different service, and the right service in a different class.
    h.hub.publish(ProviderEvent::Health(HealthTransitionEvent {
        tenant_id: h.tenant,
        service_id: Uuid::new_v4(),
        environment_class: "production".into(),
        state: "unavailable".into(),
        evidence_status: "current".into(),
        evaluated_at_unix_nano: 1,
    }));
    h.hub.publish(ProviderEvent::Health(HealthTransitionEvent {
        tenant_id: h.tenant,
        service_id: subscribed,
        environment_class: "non_production".into(),
        state: "unavailable".into(),
        evidence_status: "current".into(),
        evaluated_at_unix_nano: 2,
    }));
    // Now the matching one.
    h.hub.publish(ProviderEvent::Health(HealthTransitionEvent {
        tenant_id: h.tenant,
        service_id: subscribed,
        environment_class: "production".into(),
        state: "degraded".into(),
        evidence_status: "current".into(),
        evaluated_at_unix_nano: 3,
    }));

    let frame = reader.next_frame().await.expect("the matching replace");
    assert_eq!(frame.event, "replace");
    assert_eq!(frame.data["revision"], json!("2"));
}

// ------------------------------------------------------------------ fail-closed

#[tokio::test]
async fn a_revoked_session_stops_delivery_and_closes_the_stream() {
    let tenant = Uuid::new_v4();
    let auth = MockServer::start().await;
    // The registering request validates; the per-batch recheck then fails, the way
    // a revoked membership behaves once T0's fail-closed fix is in place.
    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user_id": Uuid::new_v4().to_string(),
            "tenant_id": tenant.to_string(),
            "role": "viewer",
        })))
        .up_to_n_times(1)
        .mount(&auth)
        .await;
    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&auth)
        .await;

    let mut h = harness_with(SubscriptionConfig::default(), auth).await;
    h.tenant = tenant;

    let response = h.open(STATUS_VIEW).await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut reader = SseReader::new(response.into_body());
    assert_eq!(reader.next_frame().await.unwrap().event, "snapshot");

    h.hub.publish(h.alert(tenant));

    let frames = reader.drain_until_closed(Duration::from_secs(10)).await;
    assert_eq!(
        frames.last().map(|f| f.event.as_str()),
        Some("reset"),
        "a revoked session must end in an explicit reset: {frames:?}"
    );
    assert_eq!(
        frames.last().unwrap().data["reason"],
        json!("authorization_changed")
    );
    assert!(
        frames
            .iter()
            .all(|f| f.event != "replace" && f.event != "delta"),
        "no data event may follow revocation: {frames:?}"
    );
}

#[tokio::test]
async fn a_tenant_switch_stops_the_old_streams_delivery() {
    let tenant = Uuid::new_v4();
    let switched_to = Uuid::new_v4();
    let auth = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user_id": Uuid::new_v4().to_string(),
            "tenant_id": tenant.to_string(),
            "role": "viewer",
        })))
        .up_to_n_times(1)
        .mount(&auth)
        .await;
    // The session's authoritative tenant is now a different one -- a switch made
    // from another tab. No client cooperation is involved.
    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user_id": Uuid::new_v4().to_string(),
            "tenant_id": switched_to.to_string(),
            "role": "viewer",
        })))
        .mount(&auth)
        .await;

    let mut h = harness_with(SubscriptionConfig::default(), auth).await;
    h.tenant = tenant;

    let response = h.open(STATUS_VIEW).await;
    let mut reader = SseReader::new(response.into_body());
    assert_eq!(reader.next_frame().await.unwrap().event, "snapshot");

    h.hub.publish(h.alert(tenant));

    let frames = reader.drain_until_closed(Duration::from_secs(10)).await;
    assert_eq!(
        frames.last().unwrap().data["reason"],
        json!("authorization_changed")
    );
    assert!(
        frames.iter().all(|f| f.event != "replace"),
        "the old tenant's stream must emit no further data: {frames:?}"
    );
}

// -------------------------------------------------------------- backpressure

#[tokio::test]
async fn a_stalled_consumer_overflows_its_fixed_queue_and_is_disconnected() {
    let tenant = Uuid::new_v4();
    let auth = auth_server(tenant).await;
    // A queue of one: the snapshot fills it, and the next event cannot be queued.
    let mut h = harness_with(
        SubscriptionConfig {
            queue_capacity: 1,
            ..SubscriptionConfig::default()
        },
        auth,
    )
    .await;
    h.tenant = tenant;

    let response = h.open(STATUS_VIEW).await;
    assert_eq!(response.status(), StatusCode::OK);

    // Deliberately do not read the body: this is the stalled browser. The driver
    // must hit the bound and disconnect on its own, before anything is drained.
    for i in 0..50 {
        h.hub
            .publish(h.deployment("checkout", &format!("v{i}"), 100 + i));
    }
    tokio::time::sleep(Duration::from_secs(1)).await;

    let mut reader = SseReader::new(response.into_body());
    let frames = reader.drain_until_closed(Duration::from_secs(15)).await;
    assert!(
        frames.len() < 50,
        "the queue must stay bounded rather than buffering every event: {}",
        frames.len()
    );
    // Disconnect is the overflow signal; there is no conflation and no ACK.
    assert!(
        !frames.iter().any(|f| f.event == "reset"),
        "overflow disconnects rather than negotiating: {frames:?}"
    );
}

#[tokio::test]
async fn an_overflow_sends_a_best_effort_reset_once_a_slot_frees() {
    let tenant = Uuid::new_v4();
    let auth = auth_server(tenant).await;
    let mut h = harness_with(
        SubscriptionConfig {
            queue_capacity: 1,
            ..SubscriptionConfig::default()
        },
        auth,
    )
    .await;
    h.tenant = tenant;

    let response = h.open(STATUS_VIEW).await;
    // The snapshot already fills the only slot. Publish while deliberately
    // not reading yet, so the driver's dispatch of this event discovers the
    // slot full and starts its best-effort, timed reset write.
    h.hub.publish(h.deployment("checkout", "v1", 100));
    // Give the driver a real but short window to run: enough to overflow and
    // enter its bounded reset write, well under that write's own timeout.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut reader = SseReader::new(response.into_body());
    // Draining the snapshot frees the slot the pending reset write is
    // waiting on, so it lands instead of timing out.
    let snapshot = reader.next_frame().await.expect("the snapshot");
    assert_eq!(snapshot.event, "snapshot");

    let reset = reader.next_frame().await.expect("a best-effort reset");
    assert_eq!(reset.event, "reset");
    assert_eq!(reset.data["reason"], json!("overflow"));

    assert!(
        reader.next_frame().await.is_none(),
        "no event may follow the overflow reset"
    );
}

#[tokio::test]
async fn an_overflow_reset_never_blocks_shutdown_when_the_socket_stays_stalled() {
    let tenant = Uuid::new_v4();
    let auth = auth_server(tenant).await;
    let mut h = harness_with(
        SubscriptionConfig {
            queue_capacity: 1,
            ..SubscriptionConfig::default()
        },
        auth,
    )
    .await;
    h.tenant = tenant;

    let response = h.open(STATUS_VIEW).await;
    // Deliberately never read: the snapshot already fills the only slot, so
    // the first published event overflows immediately and its best-effort
    // reset write can never find a free slot.
    h.hub.publish(h.deployment("checkout", "v1", 100));
    // Outlast the reset write's own timeout with nobody draining, so it must
    // have given up and the connection must already be closing.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut reader = SseReader::new(response.into_body());
    // Comfortably bounded: proves the overflow path gives up instead of
    // blocking shutdown, rather than actually needing this much time.
    let frames = reader.drain_until_closed(Duration::from_millis(500)).await;
    assert_eq!(
        frames.iter().map(|f| f.event.as_str()).collect::<Vec<_>>(),
        vec!["snapshot"],
        "a reset that could not be written in time must not appear, and the \
         connection must still close: {frames:?}"
    );
}

// ------------------------------------------------------------- replay / reset

#[tokio::test]
async fn an_unknown_last_event_id_resets_before_the_fresh_snapshot() {
    let h = harness().await;
    let response = h
        .request(STATUS_VIEW, Some("ffffffffffffffffffffffffffffffff.9"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let mut reader = SseReader::new(response.into_body());
    let first = reader.next_frame().await.expect("a reset frame");
    assert_eq!(first.event, "reset");
    assert_eq!(first.data["reason"], json!("cursor_unavailable"));
    // The client is then authoritatively resynchronized.
    let second = reader.next_frame().await.expect("a snapshot frame");
    assert_eq!(second.event, "snapshot");
    assert_eq!(second.data["revision"], json!("1"));
}

#[tokio::test]
async fn a_malformed_last_event_id_resets_rather_than_failing_the_request() {
    let h = harness().await;
    let response = h.request(STATUS_VIEW, Some("garbage")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut reader = SseReader::new(response.into_body());
    // An unparseable cursor is simply no cursor: a fresh authorized snapshot.
    let frame = reader.next_frame().await.expect("a snapshot frame");
    assert_eq!(frame.event, "snapshot");
}

#[tokio::test]
async fn a_reconnect_within_the_replay_window_resumes_instead_of_resnapshotting() {
    let h = harness().await;
    let response = h.open(STATUS_VIEW).await;
    let mut reader = SseReader::new(response.into_body());
    let snapshot = reader.next_frame().await.expect("a snapshot");
    let snapshot_id = snapshot.id.clone().unwrap();

    // Produce one more buffered data event, then end the connection.
    h.hub.publish(h.deployment("checkout", "v2.0.0", 100));
    let delta = reader.next_frame().await.expect("a delta");
    assert_eq!(delta.event, "delta");
    drop(reader);
    // Let the driver task observe the closed queue and retain its buffer.
    tokio::time::sleep(Duration::from_millis(250)).await;

    // Reconnect from the snapshot's id: the delta after it must be replayed.
    let resumed = h.request(STATUS_VIEW, Some(&snapshot_id)).await;
    assert_eq!(resumed.status(), StatusCode::OK);
    let mut reader = SseReader::new(resumed.into_body());
    let frame = reader.next_frame().await.expect("a replayed frame");
    assert_eq!(
        frame.event, "delta",
        "a covered cursor replays buffered events rather than re-snapshotting: {frame:?}"
    );
    assert_eq!(frame.id, delta.id, "replay preserves the original event id");
}

#[tokio::test]
async fn a_reconnect_with_changed_view_definitions_resets_rather_than_replaying() {
    let h = harness().await;
    let response = h.open(STATUS_VIEW).await;
    let mut reader = SseReader::new(response.into_body());
    let snapshot_id = reader.next_frame().await.unwrap().id.unwrap();
    h.hub.publish(h.deployment("checkout", "v2.0.0", 100));
    reader.next_frame().await.expect("a delta");
    drop(reader);
    tokio::time::sleep(Duration::from_millis(250)).await;

    // Same cursor, different view definition: the retained buffer is not valid for
    // this query revision.
    let changed =
        r#"[{"viewId":"status","query":{"kind":"service-status","environment":"staging"}}]"#;
    let resumed = h.request(changed, Some(&snapshot_id)).await;
    let mut reader = SseReader::new(resumed.into_body());
    let frame = reader.next_frame().await.expect("a frame");
    assert_eq!(frame.event, "reset");
    assert_eq!(frame.data["reason"], json!("cursor_unavailable"));
}

// -------------------------------------------------------------------- helpers

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}
