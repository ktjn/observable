//! Integration tests for operational context hand-off (`OBS-16-01`) and
//! authorized exit-exercise export (`OBS-16-02`).

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use admin_service::{
    AdminServiceAppState,
    context_exchange::{
        ExitExportPackage, ExportableRecord, HandoffResponse, IncidentHandoffPayload, router,
    },
    observability::AdminServiceMetrics,
};
use axum::{Json, Router, http::StatusCode, routing::post};
use observable_auth::authz::{
    Classification, DecideRequest, FilterRequest, FilterResponse, ResourceKey,
};
use serde_json::json;
use sqlx::{PgPool, Row};
use tokio::sync::Mutex;
use uuid::Uuid;

fn app_state(db: PgPool, auth_service_url: String) -> AdminServiceAppState {
    AdminServiceAppState {
        db,
        ch: clickhouse::Client::default(),
        auth_service_url,
        http_client: reqwest::Client::new(),
        metrics: Arc::new(AdminServiceMetrics::new()),
        producer: None,
    }
}

async fn serve_admin(state: AdminServiceAppState) -> String {
    let app = router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    format!("http://{addr}")
}

#[tokio::test]
async fn incident_handoff_preserves_service_severity_status_and_context() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let base = serve_admin(app_state(db.clone(), "http://127.0.0.1:1".into())).await;

    let received: Arc<Mutex<Vec<IncidentHandoffPayload>>> = Arc::new(Mutex::new(Vec::new()));
    let received_clone = Arc::clone(&received);
    let receiver_app = Router::new().route(
        "/webhook/incident",
        post(move |Json(payload): Json<IncidentHandoffPayload>| {
            let received = Arc::clone(&received_clone);
            async move {
                received.lock().await.push(payload);
                StatusCode::ACCEPTED
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind receiver");
    let receiver_addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, receiver_app).await.expect("serve") });

    let tenant_id = Uuid::new_v4();
    let payload = IncidentHandoffPayload {
        incident_id: "INC-2026-101".into(),
        tenant_id,
        service_name: "checkout-api".into(),
        severity: "critical".into(),
        status: "investigating".into(),
        summary: "Elevated 503 rate on POST /checkout".into(),
        context: json!({
            "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
            "deployment": "v2.18.0",
            "runbook": "https://runbooks.internal/checkout-503"
        }),
    };

    let resp = reqwest::Client::new()
        .post(format!("{base}/v1/context-exchange/handoff"))
        .json(&json!({
            "target_url": format!("http://{receiver_addr}/webhook/incident"),
            "max_retries": 2,
            "incident": payload
        }))
        .send()
        .await
        .expect("handoff");

    assert_eq!(resp.status(), 200);
    let body: HandoffResponse = resp.json().await.expect("json");
    assert_eq!(body.status, "delivered");
    assert_eq!(body.attempts, 1);

    let captured = received.lock().await;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0], payload);

    let outbox_status: String = sqlx::query_scalar(
        "SELECT status FROM control.context_handoff_outbox WHERE tenant_id = $1 AND incident_id = $2",
    )
    .bind(tenant_id)
    .bind("INC-2026-101")
    .fetch_one(&db)
    .await
    .expect("outbox");
    assert_eq!(outbox_status, "delivered");
}

#[tokio::test]
async fn handoff_failure_is_detected_retried_and_reported_without_silent_loss() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let base = serve_admin(app_state(db.clone(), "http://127.0.0.1:1".into())).await;

    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_clone = Arc::clone(&attempts);
    let failing_app = Router::new().route(
        "/webhook/down",
        post(move || {
            let attempts = Arc::clone(&attempts_clone);
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                StatusCode::SERVICE_UNAVAILABLE
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind failing");
    let failing_addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, failing_app).await.expect("serve") });

    let tenant_id = Uuid::new_v4();
    let payload = IncidentHandoffPayload {
        incident_id: "INC-2026-102".into(),
        tenant_id,
        service_name: "billing-worker".into(),
        severity: "critical".into(),
        status: "triggered".into(),
        summary: "Queue consumer stalled".into(),
        context: json!({ "lag": 18400 }),
    };

    let resp = reqwest::Client::new()
        .post(format!("{base}/v1/context-exchange/handoff"))
        .json(&json!({
            "target_url": format!("http://{failing_addr}/webhook/down"),
            "max_retries": 2,
            "incident": payload
        }))
        .send()
        .await
        .expect("handoff");

    assert_eq!(resp.status(), 502);
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    let body: HandoffResponse = resp.json().await.expect("json");
    assert_eq!(body.status, "failed_pending_retry");
    assert_eq!(body.attempts, 3);
    assert!(body.last_error.as_deref().unwrap().contains("503"));
    assert_eq!(body.preserved_context, payload);

    // Outbox retains the failed hand-off and full incident context for retry.
    let row = sqlx::query(
        "SELECT status, attempts, payload FROM control.context_handoff_outbox \
         WHERE tenant_id = $1 AND incident_id = $2",
    )
    .bind(tenant_id)
    .bind("INC-2026-102")
    .fetch_one(&db)
    .await
    .expect("outbox row");
    assert_eq!(row.get::<String, _>("status"), "failed_pending_retry");
    assert_eq!(row.get::<i32, _>("attempts"), 3);
    let stored_payload: serde_json::Value = row.get("payload");
    assert_eq!(stored_payload["service_name"], "billing-worker");
    assert_eq!(stored_payload["context"]["lag"], 18400);
}

async fn serve_authz(max_classification: Classification) -> String {
    let app = Router::new()
        .route(
            "/internal/authz/decide",
            post(move |Json(req): Json<DecideRequest>| async move {
                let allowed = req
                    .resource
                    .as_ref()
                    .and_then(|r| r.classification)
                    .is_some_and(|c| c <= max_classification);
                if allowed {
                    (StatusCode::OK, Json(json!({ "decision": "permit" })))
                } else {
                    (StatusCode::FORBIDDEN, Json(json!({ "decision": "deny" })))
                }
            }),
        )
        .route(
            "/internal/authz/filter",
            post(move |Json(req): Json<FilterRequest>| async move {
                let permitted = req
                    .resources
                    .into_iter()
                    .filter(|r| r.classification.is_some_and(|c| c <= max_classification))
                    .map(|r| ResourceKey {
                        resource_type: r.resource_type,
                        id: r.id,
                    })
                    .collect();
                (StatusCode::OK, Json(FilterResponse { permitted }))
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind authz");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    format!("http://{addr}")
}

#[tokio::test]
async fn exit_exercise_exports_agreed_business_records_for_independent_review() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let tenant_id = Uuid::new_v4();
    let admin_user = Uuid::new_v4();

    let auth_url = serve_authz(Classification::Restricted).await;
    let base = serve_admin(app_state(db, auth_url)).await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/v1/context-exchange/export"))
        .json(&json!({
            "tenant_id": tenant_id,
            "user_id": admin_user,
            "purpose": "compliance_exit_exercise",
            "format": "json",
            "records": [
                {
                    "record_id": "svc-checkout",
                    "record_type": "catalog.service",
                    "classification": "internal",
                    "payload": { "owner": "payments", "slo": "99.9%" }
                },
                {
                    "record_id": "svc-ledger",
                    "record_type": "catalog.service",
                    "classification": "confidential",
                    "payload": { "owner": "finance", "slo": "99.99%" }
                }
            ]
        }))
        .send()
        .await
        .expect("export");

    assert_eq!(resp.status(), 200);
    let pkg: ExitExportPackage = resp.json().await.expect("package");
    assert_eq!(pkg.schema_version, "1.0");
    assert_eq!(pkg.format, "json");
    assert_eq!(pkg.record_count, 2);
    assert_eq!(pkg.records[0].record_id, "svc-checkout");
    assert_eq!(pkg.records[1].record_id, "svc-ledger");
}

#[tokio::test]
async fn restricted_scope_export_withholds_unauthorized_records() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let tenant_id = Uuid::new_v4();
    let viewer_user = Uuid::new_v4();

    let auth_url = serve_authz(Classification::Internal).await;
    let base = serve_admin(app_state(db, auth_url)).await;

    let records = vec![
        ExportableRecord {
            record_id: "svc-public-docs".into(),
            record_type: "catalog.service".into(),
            classification: Classification::Public,
            payload: json!({ "name": "docs" }),
        },
        ExportableRecord {
            record_id: "svc-internal-web".into(),
            record_type: "catalog.service".into(),
            classification: Classification::Internal,
            payload: json!({ "name": "web" }),
        },
        ExportableRecord {
            record_id: "svc-restricted-vault".into(),
            record_type: "catalog.service".into(),
            classification: Classification::Restricted,
            payload: json!({ "secret_key": "must-not-export" }),
        },
    ];

    let resp = reqwest::Client::new()
        .post(format!("{base}/v1/context-exchange/export"))
        .json(&json!({
            "tenant_id": tenant_id,
            "user_id": viewer_user,
            "purpose": "audit_export",
            "format": "json",
            "records": records
        }))
        .send()
        .await
        .expect("export");

    assert_eq!(resp.status(), 200);
    let pkg: ExitExportPackage = resp.json().await.expect("package");
    assert_eq!(pkg.record_count, 2);
    assert!(
        pkg.records
            .iter()
            .all(|r| r.record_id != "svc-restricted-vault")
    );
    let encoded = serde_json::to_string(&pkg).unwrap();
    assert!(!encoded.contains("must-not-export"));
}
