// HTTP integration test for PUSH-013: alert-evaluator publishes a versioned
// incident-state-change event only after the `incident_events` audit row
// commits, and the published payload never discloses alert-rule internals,
// free-text content or any field beyond what's already query-visible to any
// authorized tenant member via GET /v1/incidents/{id} (PUSH-013-C1/C2).

use alert_evaluator::queue::{AlertEventPublisher, AlertStateChangeEvent};
use alert_evaluator::{AppState, incidents, middleware::auth::TenantContext, observability};
use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::{get, post},
};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::postgres::PgPool;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;
use uuid::Uuid;

async fn start_postgres() -> (PgPool, test_support::postgres::TestDatabase) {
    test_support::postgres::isolated_pool().await
}

async fn response_body_json(body: Body) -> Value {
    let bytes = body.collect().await.expect("body collected").to_bytes();
    serde_json::from_slice(&bytes).expect("valid JSON")
}

async fn insert_tenant(pool: &PgPool, tenant_id: Uuid) {
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(tenant_id)
        .bind(format!("tenant-{tenant_id}"))
        .execute(pool)
        .await
        .expect("tenant inserted");
}

#[derive(Default, Clone)]
struct RecordingPublisher {
    events: Arc<Mutex<Vec<AlertStateChangeEvent>>>,
}

impl RecordingPublisher {
    fn captured(&self) -> Vec<AlertStateChangeEvent> {
        self.events.lock().unwrap().clone()
    }
}

#[async_trait]
impl AlertEventPublisher for RecordingPublisher {
    async fn publish(&self, event: &AlertStateChangeEvent) -> anyhow::Result<()> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }
}

fn build_app(db: PgPool, tenant_id: Uuid, publisher: RecordingPublisher) -> Router {
    let state = AppState {
        db: db.clone(),
        ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        auth_service_url: "http://auth-service:4319".into(),
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AlertEvaluatorMetrics::new()),
        producer: Some(Arc::new(publisher)),
    };
    Router::new()
        .route(
            "/v1/incidents",
            get(incidents::handle_list_incidents).post(incidents::handle_create_incident),
        )
        .route(
            "/v1/incidents/{incident_id}",
            get(incidents::handle_get_incident),
        )
        .route(
            "/v1/incidents/{incident_id}/status",
            post(incidents::handle_update_incident_status),
        )
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id: None,
            role: "tenant_admin".into(),
        }))
        .with_state(state)
}

#[tokio::test]
async fn update_incident_status_commit_publishes_state_change_event_without_restricted_fields() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;

    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'Secret rule name', 'threshold', 'critical', \
                 '{\"metric_name\":\"cpu\",\"operator\":\"gt\",\"threshold\":90}', \
                 '{}', true) \
         RETURNING rule_id",
    )
    .bind(tenant)
    .fetch_one(&db)
    .await
    .expect("rule inserted");

    let incident_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key, triggered_by_rule_id, owner, impact_summary) \
         VALUES ($1, $2, 'Sensitive incident title', 'critical', 'triggered', 'dedup-push-013', $3, 'alice', 'secret impact detail')",
    )
    .bind(incident_id)
    .bind(tenant)
    .bind(rule_id)
    .execute(&db)
    .await
    .expect("incident inserted");

    let publisher = RecordingPublisher::default();
    let app = build_app(db, tenant, publisher.clone());

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/incidents/{incident_id}/status"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"status":"resolved","actor":"alice","message":"Secret resolution note"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = response_body_json(res.into_body()).await;
    assert_eq!(body["status"], "resolved");

    let events = publisher.captured();
    assert_eq!(events.len(), 1, "exactly one event published after commit");
    let event = &events[0];
    assert_eq!(event.tenant_id, tenant);
    assert_eq!(event.incident_id, incident_id);
    assert_eq!(event.status, "resolved");
    assert_eq!(event.severity, "critical");
    assert_eq!(event.event_type, "alert_resolved");

    // PUSH-013-C2: the serialized payload must contain exactly the safe
    // field set -- no rule internals, no free text, no cross-incident data.
    let payload = serde_json::to_value(event).unwrap();
    let fields: std::collections::BTreeSet<String> =
        payload.as_object().unwrap().keys().cloned().collect();
    let expected: std::collections::BTreeSet<String> = [
        "tenant_id",
        "incident_id",
        "status",
        "severity",
        "event_type",
        "occurred_at_unix_nano",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(
        fields, expected,
        "payload must carry only the safe field set"
    );

    let serialized = serde_json::to_string(&payload).unwrap();
    assert!(!serialized.contains("Secret rule name"));
    assert!(!serialized.contains("Sensitive incident title"));
    assert!(!serialized.contains("secret impact detail"));
    assert!(!serialized.contains("Secret resolution note"));
}

#[tokio::test]
async fn update_incident_status_unknown_incident_publishes_no_event() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;
    let unknown_id = Uuid::new_v4();

    let publisher = RecordingPublisher::default();
    let app = build_app(db, tenant, publisher.clone());

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/incidents/{unknown_id}/status"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"status":"resolved","actor":"alice"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert!(
        publisher.captured().is_empty(),
        "no event when the incident-state write never happened"
    );
}

#[tokio::test]
async fn update_incident_status_rejected_invalid_status_publishes_no_event() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;

    let incident_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO incidents (incident_id, tenant_id, title, severity, status, dedup_key) \
         VALUES ($1, $2, 'Incident', 'critical', 'triggered', 'dedup-push-013-invalid')",
    )
    .bind(incident_id)
    .bind(tenant)
    .execute(&db)
    .await
    .expect("incident inserted");

    let publisher = RecordingPublisher::default();
    let app = build_app(db, tenant, publisher.clone());

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/incidents/{incident_id}/status"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"status":"not-a-real-status","actor":"alice"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        publisher.captured().is_empty(),
        "no event when the status write is rejected before commit"
    );
}
