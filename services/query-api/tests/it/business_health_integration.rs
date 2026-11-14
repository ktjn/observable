//! HTTP integration tests for `assess-customer-experience` and
//! `track-business-processes` (Phase 7 S3: OBS-05-01, OBS-05-02,
//! OBS-12-01, OBS-12-02, OBS-32-01, OBS-32-02) via `tower::ServiceExt::oneshot`.

use std::sync::Arc;

use axum::{
    Extension, Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use chrono::{Duration, Utc};
use http_body_util::BodyExt;
use query_api::business_health::{
    BusinessHealthSource, BusinessProcessMeasure, CustomerJourneyTelemetry,
    InMemoryBusinessHealthSource, ScheduledProcessSpec, get_journey_health, get_process_health,
    get_scheduled_process_status, register_scheduled_process,
};
use query_api::middleware::auth::TenantContext;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

fn build_app(source: Arc<dyn BusinessHealthSource>, tenant_id: Uuid) -> Router {
    Router::new()
        .route("/v1/customer-experience/journeys", get(get_journey_health))
        .route("/v1/processes/health", get(get_process_health))
        .route(
            "/v1/processes/scheduled",
            get(get_scheduled_process_status).post(register_scheduled_process),
        )
        .layer(Extension(source))
        .layer(Extension(TenantContext {
            tenant_id,
            user_id: None,
            role: "operator".into(),
        }))
}

async fn json_body(body: Body) -> Value {
    let bytes = body.collect().await.expect("body collected").to_bytes();
    serde_json::from_slice(&bytes).expect("valid JSON")
}

#[tokio::test]
async fn obs_05_01_journey_health_reflects_failures_and_shows_unknown_when_evidence_stops() {
    let tenant_id = Uuid::new_v4();
    let now = Utc::now();
    let source: Arc<dyn BusinessHealthSource> = Arc::new(InMemoryBusinessHealthSource::new(
        vec![
            (
                tenant_id,
                CustomerJourneyTelemetry {
                    journey_id: "checkout-flow".into(),
                    name: "Checkout Flow".into(),
                    success_count: 0,
                    failure_count: 15,
                    client_error_count: 0,
                    p95_latency_ms: 850.0,
                    backend_healthy: false,
                    last_evidence_at: Some(now - Duration::seconds(10)),
                    freshness_seconds: 120,
                },
            ),
            (
                tenant_id,
                CustomerJourneyTelemetry {
                    journey_id: "signup-flow".into(),
                    name: "Signup Flow".into(),
                    success_count: 50,
                    failure_count: 0,
                    client_error_count: 0,
                    p95_latency_ms: 120.0,
                    backend_healthy: true,
                    last_evidence_at: Some(now - Duration::minutes(30)), // stale!
                    freshness_seconds: 120,
                },
            ),
        ],
        vec![],
        vec![],
    ));
    let app = build_app(source, tenant_id);

    // Acceptance: failed journey reflected in customer-facing health view
    let fail_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/customer-experience/journeys?journey_id=checkout-flow")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(fail_resp.status(), StatusCode::OK);
    let fail_json = json_body(fail_resp.into_body()).await;
    assert_eq!(fail_json["status"], "failed");
    assert_eq!(fail_json["customer_impact"], true);

    // Negative: stale/stopped evidence shows unknown, not success
    let stale_resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/customer-experience/journeys?journey_id=signup-flow")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let stale_json = json_body(stale_resp.into_body()).await;
    assert_eq!(stale_json["status"], "unknown");
    assert_eq!(stale_json["evidence_status"], "stale");
}

#[tokio::test]
async fn obs_05_02_client_side_failure_reported_as_customer_impact_even_when_backend_healthy() {
    let tenant_id = Uuid::new_v4();
    let now = Utc::now();
    let source: Arc<dyn BusinessHealthSource> = Arc::new(InMemoryBusinessHealthSource::new(
        vec![(
            tenant_id,
            CustomerJourneyTelemetry {
                journey_id: "web-cart".into(),
                name: "Web Cart".into(),
                success_count: 100,
                failure_count: 0,
                client_error_count: 24,
                p95_latency_ms: 180.0,
                backend_healthy: true,
                last_evidence_at: Some(now - Duration::seconds(5)),
                freshness_seconds: 120,
            },
        )],
        vec![],
        vec![],
    ));
    let app = build_app(source, tenant_id);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/customer-experience/journeys?journey_id=web-cart")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp.into_body()).await;
    assert_eq!(body["backend_healthy"], true);
    assert_eq!(body["status"], "degraded");
    assert_eq!(body["customer_impact"], true);
}

#[tokio::test]
async fn obs_12_01_and_obs_12_02_process_outcomes_unknown_when_silent_and_stalled_when_services_up()
{
    let tenant_id = Uuid::new_v4();
    let now = Utc::now();
    let source: Arc<dyn BusinessHealthSource> = Arc::new(InMemoryBusinessHealthSource::new(
        vec![],
        vec![
            (
                tenant_id,
                BusinessProcessMeasure {
                    process_id: "order-fulfillment".into(),
                    name: "Order Fulfillment".into(),
                    owner: "ops-lead".into(),
                    completed_count: 250,
                    failed_count: 0,
                    stalled_count: 0,
                    involved_services_available: true,
                    last_reported_at: Some(now - Duration::seconds(10)),
                    freshness_seconds: 120,
                },
            ),
            (
                tenant_id,
                BusinessProcessMeasure {
                    process_id: "invoice-sync".into(),
                    name: "Invoice Sync".into(),
                    owner: "finance-ops".into(),
                    completed_count: 40,
                    failed_count: 0,
                    stalled_count: 0,
                    involved_services_available: true,
                    last_reported_at: Some(now - Duration::minutes(20)), // stopped reporting!
                    freshness_seconds: 120,
                },
            ),
            (
                tenant_id,
                BusinessProcessMeasure {
                    process_id: "kyc-onboarding".into(),
                    name: "KYC Onboarding".into(),
                    owner: "compliance-ops".into(),
                    completed_count: 10,
                    failed_count: 0,
                    stalled_count: 7,
                    involved_services_available: true, // services available, process stalled!
                    last_reported_at: Some(now - Duration::seconds(5)),
                    freshness_seconds: 120,
                },
            ),
        ],
        vec![],
    ));
    let app = build_app(source, tenant_id);

    // OBS-12-01 acceptance: owner views current outcome
    let ok_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/processes/health?process_id=order-fulfillment")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let ok_json = json_body(ok_resp.into_body()).await;
    assert_eq!(ok_json["outcome"], "normal");

    // OBS-12-01 negative: stopped reporting -> unknown, not normal
    let silent_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/processes/health?process_id=invoice-sync")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let silent_json = json_body(silent_resp.into_body()).await;
    assert_eq!(silent_json["outcome"], "unknown");
    assert_eq!(silent_json["source_status"], "stopped_reporting");

    // OBS-12-02 acceptance & negative: stalled process reported even while services available
    let stalled_resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/processes/health?process_id=kyc-onboarding")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let stalled_json = json_body(stalled_resp.into_body()).await;
    assert_eq!(stalled_json["involved_services_available"], true);
    assert_eq!(stalled_json["outcome"], "stalled");
}

#[tokio::test]
async fn obs_32_01_and_obs_32_02_scheduled_process_registration_and_pre_cutoff_detection() {
    let tenant_id = Uuid::new_v4();
    let base = Utc::now();
    let expected_start = base - Duration::minutes(30);
    let deadline = base + Duration::minutes(30); // cut-off is STILL in the future!

    let source: Arc<dyn BusinessHealthSource> = Arc::new(InMemoryBusinessHealthSource::new(
        vec![],
        vec![],
        vec![
            (
                tenant_id,
                ScheduledProcessSpec {
                    process_id: "eod-delayed".into(),
                    name: "EOD Settlement (Delayed)".into(),
                    criticality: "critical".into(),
                    expected_start: Some(expected_start),
                    completion_deadline: Some(deadline),
                    start_grace_seconds: 300,
                    owner: Some("batch-ops".into()),
                    dependencies: vec!["ledger".into()],
                    run_started_at: Some(expected_start + Duration::minutes(2)),
                    run_completed_at: None,
                    estimated_finish_at: Some(deadline + Duration::minutes(20)),
                },
            ),
            (
                tenant_id,
                ScheduledProcessSpec {
                    process_id: "eod-never-started".into(),
                    name: "EOD Reconciliation (Never Started)".into(),
                    criticality: "critical".into(),
                    expected_start: Some(expected_start),
                    completion_deadline: Some(deadline),
                    start_grace_seconds: 300,
                    owner: Some("batch-ops".into()),
                    dependencies: vec![],
                    run_started_at: None,
                    run_completed_at: None,
                    estimated_finish_at: None,
                },
            ),
        ],
    ));
    let app = build_app(source, tenant_id);

    // OBS-32-01 negative: critical process registered without deadline or owner is flagged incomplete
    let incomplete_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/processes/scheduled")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "process_id": "eod-incomplete",
                        "name": "EOD Unowned",
                        "criticality": "critical",
                        "expected_start": expected_start.to_rfc3339(),
                        "completion_deadline": null,
                        "start_grace_seconds": 300,
                        "owner": null,
                        "dependencies": [],
                        "run_started_at": null,
                        "run_completed_at": null,
                        "estimated_finish_at": null
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(incomplete_resp.status(), StatusCode::CREATED);
    let incomplete_json = json_body(incomplete_resp.into_body()).await;
    assert_eq!(incomplete_json["registration_status"], "incomplete");
    assert!(
        incomplete_json["incomplete_reasons"]
            .as_array()
            .unwrap()
            .contains(&json!("missing_owner"))
    );
    assert!(
        incomplete_json["incomplete_reasons"]
            .as_array()
            .unwrap()
            .contains(&json!("missing_deadline"))
    );

    // OBS-32-01 acceptance + OBS-32-02 acceptance: delayed run triggers at_risk BEFORE cut-off
    let now_str = base.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let delayed_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/processes/scheduled?process_id=eod-delayed&now={now_str}"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let delayed_json = json_body(delayed_resp.into_body()).await;
    assert_eq!(delayed_json["registration_status"], "complete");
    assert_eq!(delayed_json["owner"], "batch-ops");
    assert_eq!(delayed_json["run_status"], "at_risk");
    assert!(delayed_json["warning"].is_string());

    // OBS-32-02 negative: process that never started is detected BEFORE cut-off
    let never_started_resp = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/processes/scheduled?process_id=eod-never-started&now={now_str}"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let never_started_json = json_body(never_started_resp.into_body()).await;
    assert_eq!(never_started_json["run_status"], "missed_start");
    assert!(never_started_json["warning"].is_string());
}
