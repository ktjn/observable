//! HTTP integration tests for `trace-transactions` (Phase 7 S1:
//! OBS-31-01, OBS-31-02, OBS-31-03) exercising `GET /v1/transactions/lookup`
//! and `GET /v1/transactions/audit` via `tower::ServiceExt::oneshot`.

use std::sync::Arc;

use axum::{
    Extension, Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use http_body_util::BodyExt;
use query_api::middleware::auth::TenantContext;
use query_api::transactions::{
    InMemoryTransactionAuditSink, InMemoryTransactionSource, TransactionAuditSink,
    TransactionLookupSource, TransactionRecord, TransactionStep, list_transaction_audit,
    lookup_transaction,
};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

fn build_app(
    source: Arc<dyn TransactionLookupSource>,
    audit: Arc<dyn TransactionAuditSink>,
    tenant_id: Uuid,
    role: &str,
    user_id: Option<Uuid>,
) -> Router {
    Router::new()
        .route("/v1/transactions/lookup", get(lookup_transaction))
        .route("/v1/transactions/audit", get(list_transaction_audit))
        .layer(Extension(source))
        .layer(Extension(audit))
        .layer(Extension(TenantContext {
            tenant_id,
            user_id,
            role: role.to_string(),
        }))
}

async fn json_body(body: Body) -> Value {
    let bytes = body.collect().await.expect("body collected").to_bytes();
    serde_json::from_slice(&bytes).expect("valid JSON")
}

#[tokio::test]
async fn obs_31_01_lookup_payment_by_reference_and_refuse_outside_role_or_purpose() {
    let tenant_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let source: Arc<dyn TransactionLookupSource> =
        Arc::new(InMemoryTransactionSource::with_records(vec![
            (
                tenant_id,
                TransactionRecord {
                    reference: "PAY-1001".into(),
                    status: "completed".into(),
                    data_classification: "internal".into(),
                    allowed_purposes: vec!["customer_support".into()],
                    expected_step_count: 2,
                    steps: vec![
                        TransactionStep {
                            sequence_index: 0,
                            service_name: "checkout".into(),
                            operation: "POST /pay".into(),
                            duration_ms: 45,
                            status: "ok".into(),
                        },
                        TransactionStep {
                            sequence_index: 1,
                            service_name: "payments".into(),
                            operation: "charge".into(),
                            duration_ms: 110,
                            status: "ok".into(),
                        },
                    ],
                },
            ),
            (
                tenant_id,
                TransactionRecord {
                    reference: "PAY-RESTRICTED-9".into(),
                    status: "completed".into(),
                    data_classification: "restricted".into(),
                    allowed_purposes: vec!["compliance_audit".into()],
                    expected_step_count: 1,
                    steps: vec![TransactionStep {
                        sequence_index: 0,
                        service_name: "vault".into(),
                        operation: "settle".into(),
                        duration_ms: 20,
                        status: "ok".into(),
                    }],
                },
            ),
        ]));
    let audit: Arc<dyn TransactionAuditSink> = Arc::new(InMemoryTransactionAuditSink::default());
    let app = build_app(
        source,
        audit.clone(),
        tenant_id,
        "support_agent",
        Some(agent_id),
    );

    // Acceptance: support agent finds payment by reference and sees status
    let ok_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/transactions/lookup?reference=PAY-1001&purpose=customer_support")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok_resp.status(), StatusCode::OK);
    let ok_json = json_body(ok_resp.into_body()).await;
    assert_eq!(ok_json["reference"], "PAY-1001");
    assert_eq!(ok_json["status"], "completed");

    // Negative: lookup for restricted reference outside support_agent's role/purpose is refused & recorded
    let denied_resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/transactions/lookup?reference=PAY-RESTRICTED-9&purpose=customer_support")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied_resp.status(), StatusCode::FORBIDDEN);

    let entries = audit.list(tenant_id).await;
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1].reference, "PAY-RESTRICTED-9");
    assert_eq!(entries[1].outcome, "refused_unauthorized");
}

#[tokio::test]
async fn obs_31_02_identifies_delayed_step_and_discloses_incomplete_path_gap() {
    let tenant_id = Uuid::new_v4();
    let source: Arc<dyn TransactionLookupSource> =
        Arc::new(InMemoryTransactionSource::with_records(vec![
            (
                tenant_id,
                TransactionRecord {
                    reference: "PAY-DELAY-2".into(),
                    status: "delayed".into(),
                    data_classification: "internal".into(),
                    allowed_purposes: vec!["incident_investigation".into()],
                    expected_step_count: 3,
                    steps: vec![
                        TransactionStep {
                            sequence_index: 0,
                            service_name: "gateway".into(),
                            operation: "route".into(),
                            duration_ms: 12,
                            status: "ok".into(),
                        },
                        TransactionStep {
                            sequence_index: 1,
                            service_name: "fraud-check".into(),
                            operation: "score".into(),
                            duration_ms: 4800,
                            status: "delayed".into(),
                        },
                        TransactionStep {
                            sequence_index: 2,
                            service_name: "payments".into(),
                            operation: "capture".into(),
                            duration_ms: 95,
                            status: "ok".into(),
                        },
                    ],
                },
            ),
            (
                tenant_id,
                TransactionRecord {
                    reference: "PAY-GAP-3".into(),
                    status: "completed".into(),
                    data_classification: "internal".into(),
                    allowed_purposes: vec!["incident_investigation".into()],
                    expected_step_count: 3,
                    steps: vec![
                        TransactionStep {
                            sequence_index: 0,
                            service_name: "gateway".into(),
                            operation: "route".into(),
                            duration_ms: 10,
                            status: "ok".into(),
                        },
                        // step 1 missing!
                        TransactionStep {
                            sequence_index: 2,
                            service_name: "payments".into(),
                            operation: "capture".into(),
                            duration_ms: 80,
                            status: "ok".into(),
                        },
                    ],
                },
            ),
        ]));
    let audit: Arc<dyn TransactionAuditSink> = Arc::new(InMemoryTransactionAuditSink::default());
    let app = build_app(source, audit, tenant_id, "responder", None);

    // Acceptance: identifies the step where a delayed transaction is held
    let delay_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/transactions/lookup?reference=PAY-DELAY-2&purpose=incident_investigation")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delay_resp.status(), StatusCode::OK);
    let delay_json = json_body(delay_resp.into_body()).await;
    assert_eq!(delay_json["held_at_step"], "fraud-check:score");
    assert_eq!(delay_json["path_complete"], true);

    // Negative: incomplete path shows gap rather than assumed completion
    let gap_resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/transactions/lookup?reference=PAY-GAP-3&purpose=incident_investigation")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(gap_resp.status(), StatusCode::OK);
    let gap_json = json_body(gap_resp.into_body()).await;
    assert_eq!(gap_json["path_complete"], false);
    assert_eq!(gap_json["status"], "incomplete");
    assert!(
        gap_json["gap_disclosure"]
            .as_str()
            .unwrap()
            .contains("2 of 3")
    );
}

#[tokio::test]
async fn obs_31_03_requires_stated_purpose_and_audits_every_lookup() {
    let tenant_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let source: Arc<dyn TransactionLookupSource> =
        Arc::new(InMemoryTransactionSource::with_records(vec![(
            tenant_id,
            TransactionRecord {
                reference: "PAY-AUDIT-4".into(),
                status: "completed".into(),
                data_classification: "internal".into(),
                allowed_purposes: vec!["customer_support".into()],
                expected_step_count: 1,
                steps: vec![TransactionStep {
                    sequence_index: 0,
                    service_name: "payments".into(),
                    operation: "charge".into(),
                    duration_ms: 30,
                    status: "ok".into(),
                }],
            },
        )]));
    let audit: Arc<dyn TransactionAuditSink> = Arc::new(InMemoryTransactionAuditSink::default());
    let app = build_app(
        source,
        audit.clone(),
        tenant_id,
        "support_agent",
        Some(actor_id),
    );

    // Negative: lookup without a stated purpose is refused and audited
    let no_purpose_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/transactions/lookup?reference=PAY-AUDIT-4")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(no_purpose_resp.status(), StatusCode::FORBIDDEN);

    // Acceptance: lookup with stated purpose succeeds and records actor, purpose, reference
    let ok_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/transactions/lookup?reference=PAY-AUDIT-4&purpose=customer_support")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok_resp.status(), StatusCode::OK);

    let audit_resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/transactions/audit")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(audit_resp.status(), StatusCode::OK);
    let audit_json = json_body(audit_resp.into_body()).await;
    let items = audit_json["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["outcome"], "refused_missing_purpose");
    assert_eq!(items[0]["reference"], "PAY-AUDIT-4");
    assert_eq!(items[1]["outcome"], "allowed");
    assert_eq!(items[1]["actor"], format!("user:{actor_id}"));
    assert_eq!(items[1]["purpose"], "customer_support");
    assert_eq!(items[1]["reference"], "PAY-AUDIT-4");
}
