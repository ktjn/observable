//! HTTP integration tests for `map-dependencies-and-impact` (Phase 7 S2:
//! OBS-10-01, OBS-10-02, OBS-10-07) exercising `GET /v1/impact/dependencies`
//! and `POST /v1/impact/assess` via `tower::ServiceExt::oneshot`.

use std::sync::Arc;

use axum::{
    Extension, Router,
    body::Body,
    http::{Request, StatusCode},
    routing::{get, post},
};
use http_body_util::BodyExt;
use query_api::impact::{
    DependencyImpactSource, DependentServiceEdge, InMemoryDependencyImpactSource,
    ServiceBusinessExposure, assess_business_impact, get_dependency_exposure,
};
use query_api::middleware::auth::TenantContext;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

fn build_app(source: Arc<dyn DependencyImpactSource>, tenant_id: Uuid) -> Router {
    Router::new()
        .route("/v1/impact/dependencies", get(get_dependency_exposure))
        .route("/v1/impact/assess", post(assess_business_impact))
        .layer(Extension(source))
        .layer(Extension(TenantContext {
            tenant_id,
            user_id: None,
            role: "responder".into(),
        }))
}

async fn json_body(body: Body) -> Value {
    let bytes = body.collect().await.expect("body collected").to_bytes();
    serde_json::from_slice(&bytes).expect("valid JSON")
}

#[tokio::test]
async fn obs_10_01_dependency_exposure_shows_confirmed_and_unconfirmed_mappings() {
    let tenant_id = Uuid::new_v4();
    let source: Arc<dyn DependencyImpactSource> = Arc::new(InMemoryDependencyImpactSource::new(
        vec![
            (
                tenant_id,
                "stripe-api".into(),
                DependentServiceEdge {
                    business_service_name: "Checkout".into(),
                    criticality: "critical".into(),
                    journeys: vec!["Card Payment".into()],
                    mapping_confirmed: true,
                    mapping_source: "catalog-verified".into(),
                },
            ),
            (
                tenant_id,
                "stripe-api".into(),
                DependentServiceEdge {
                    business_service_name: "Subscription Billing".into(),
                    criticality: "high".into(),
                    journeys: vec!["Recurring Renewal".into()],
                    mapping_confirmed: false,
                    mapping_source: "trace-inferred".into(),
                },
            ),
        ],
        vec![],
    ));
    let app = build_app(source, tenant_id);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/impact/dependencies?dependency=stripe-api")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp.into_body()).await;
    assert_eq!(body["confirmed_count"], 1);
    assert_eq!(body["unconfirmed_count"], 1);

    let exposure = body["immediate_exposure"].as_array().unwrap();
    assert_eq!(exposure.len(), 2);
    let confirmed = exposure
        .iter()
        .find(|e| e["business_service_name"] == "Checkout")
        .unwrap();
    assert_eq!(confirmed["mapping_status"], "confirmed");
    let unconfirmed = exposure
        .iter()
        .find(|e| e["business_service_name"] == "Subscription Billing")
        .unwrap();
    assert_eq!(unconfirmed["mapping_status"], "unconfirmed");
}

#[tokio::test]
async fn obs_10_02_identical_symptoms_with_different_business_exposure_receive_different_priorities()
 {
    let tenant_id = Uuid::new_v4();
    let source: Arc<dyn DependencyImpactSource> = Arc::new(InMemoryDependencyImpactSource::new(
        vec![],
        vec![
            (
                tenant_id,
                ServiceBusinessExposure {
                    service_name: "checkout".into(),
                    criticality: "critical".into(),
                    critical_journeys: vec!["Complete Purchase".into()],
                    critical_processes: vec!["Payment Settlement".into()],
                },
            ),
            (
                tenant_id,
                ServiceBusinessExposure {
                    service_name: "marketing-analytics".into(),
                    criticality: "standard".into(),
                    critical_journeys: vec![],
                    critical_processes: vec![],
                },
            ),
        ],
    ));
    let app = build_app(source, tenant_id);

    let checkout_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/impact/assess")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "affected_service": "checkout",
                        "technical_severity": "critical",
                        "error_rate": 0.08,
                        "p95_latency_ms": 600.0
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(checkout_resp.status(), StatusCode::OK);
    let checkout_body = json_body(checkout_resp.into_body()).await;

    let analytics_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/impact/assess")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "affected_service": "marketing-analytics",
                        "technical_severity": "critical",
                        "error_rate": 0.08,
                        "p95_latency_ms": 600.0
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(analytics_resp.status(), StatusCode::OK);
    let analytics_body = json_body(analytics_resp.into_body()).await;

    assert_eq!(checkout_body["impact_priority"], "P1");
    assert_eq!(analytics_body["impact_priority"], "P3");
    assert_ne!(
        checkout_body["impact_priority"],
        analytics_body["impact_priority"]
    );
}

#[tokio::test]
async fn obs_10_07_distinguishes_confirmed_transactions_from_labeled_estimates_and_unknown_not_zero()
 {
    let tenant_id = Uuid::new_v4();
    let source: Arc<dyn DependencyImpactSource> =
        Arc::new(InMemoryDependencyImpactSource::default());
    let app = build_app(source, tenant_id);

    // 1. Confirmed transactions + labeled estimate
    let both_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/impact/assess")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "affected_service": "checkout",
                        "technical_severity": "critical",
                        "error_rate": 0.05,
                        "p95_latency_ms": 300.0,
                        "confirmed_affected_transactions": 42,
                        "estimated_customer_exposure": 350,
                        "estimation_basis": "15m traffic extrapolation"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let both_body = json_body(both_resp.into_body()).await;
    assert_eq!(
        both_body["observed_impact"]["status"],
        "confirmed_by_evidence"
    );
    assert_eq!(both_body["observed_impact"]["count"], 42);
    assert_eq!(both_body["estimated_impact"]["status"], "estimated");
    assert!(
        both_body["estimated_impact"]["label"]
            .as_str()
            .unwrap()
            .contains("Estimate")
    );
    assert_eq!(both_body["estimated_impact"]["count"], 350);

    // 2. Unknown impact -> explicit unknown, count is null (never 0)
    let unknown_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/impact/assess")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "affected_service": "checkout",
                        "technical_severity": "warning",
                        "error_rate": 0.02,
                        "p95_latency_ms": 200.0
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let unknown_body = json_body(unknown_resp.into_body()).await;
    assert_eq!(unknown_body["observed_impact"]["status"], "unknown");
    assert_eq!(unknown_body["observed_impact"]["label"], "Unknown");
    assert!(unknown_body["observed_impact"]["count"].is_null());
    assert_eq!(unknown_body["estimated_impact"]["status"], "unknown");
    assert!(unknown_body["estimated_impact"]["count"].is_null());
}

#[test]
fn obs_10_p1_dependency_and_impact_checks() {
    use query_api::impact::{
        build_incident_business_impact_report, discover_second_order_dependencies,
        flag_undeclared_dependencies, identify_shared_service_consumers,
    };
    let edges = [("checkout", "payments"), ("payments", "ledger-db")];
    assert_eq!(
        discover_second_order_dependencies(&edges, "checkout"),
        vec!["ledger-db"]
    );
    let shared_edges = [("checkout", "auth-core"), ("billing", "auth-core")];
    assert_eq!(
        identify_shared_service_consumers(&shared_edges, "auth-core"),
        vec!["checkout", "billing"]
    );
    let report = build_incident_business_impact_report(vec!["wires".to_string()], 37);
    assert_eq!(report.unsuccessful_transactions, 37);
    let flags = flag_undeclared_dependencies(
        &[("checkout", "legacy-cache"), ("checkout", "payments")],
        &[("checkout", "payments")],
    );
    assert_eq!(flags.len(), 1);
    assert!(flags[0].owner_confirmation_required);
}
