// HTTP integration test for GET/POST /v1/slos against a real Postgres
// instance via Testcontainers, exercising the full handler path via
// tower::ServiceExt::oneshot.

use alert_evaluator::{AppState, middleware::auth::TenantContext, observability, slos};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::{get, post},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::postgres::PgPool;
use std::sync::Arc;
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

fn build_app(db: PgPool, tenant_id: Uuid) -> Router {
    let state = AppState {
        db: db.clone(),
        ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        auth_service_url: "http://auth-service:4319".into(),
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AlertEvaluatorMetrics::new()),
    };
    Router::new()
        .route(
            "/v1/slos",
            get(slos::handle_list_slos).post(slos::handle_create_slo),
        )
        .route(
            "/v1/slos/evaluate-risk",
            post(slos::handle_evaluate_budget_risk),
        )
        .route(
            "/v1/slos/monthly-review",
            post(slos::handle_monthly_slo_review),
        )
        .route(
            "/v1/slos/effective-target",
            post(slos::handle_resolve_effective_target),
        )
        .route(
            "/v1/slos/evaluate-journey",
            post(slos::handle_evaluate_journey_slo),
        )
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id: None,
            role: "tenant_admin".into(),
        }))
        .with_state(state)
}

#[tokio::test]
async fn post_slo_creates_tenant_scoped_definition() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;
    let app = build_app(db, tenant);

    let body = json!({
        "service_name": "payments",
        "environment": "prod",
        "target": 0.999,
        "window_days": 30,
        "burn_rate_fast_threshold": 14.4,
        "burn_rate_slow_threshold": 1.0,
        "description": "Payments availability SLO"
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/slos")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(body["service_name"], "payments");
    assert_eq!(body["environment"], "prod");
    assert_eq!(body["sli_type"], "availability");
    assert_eq!(body["target"], 0.999);
    assert_eq!(body["firing"], false);
    assert!(body["last_fired_at"].is_null());
}

#[tokio::test]
async fn post_slo_rejects_invalid_target() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;
    let app = build_app(db, tenant);

    let body = json!({
        "service_name": "payments",
        "environment": "prod",
        "target": 1.0,
        "window_days": 30,
        "burn_rate_fast_threshold": 14.4,
        "burn_rate_slow_threshold": 1.0
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/slos")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_slos_does_not_return_other_tenant_definitions() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    let other_tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;
    insert_tenant(&db, other_tenant).await;

    sqlx::query(
        "INSERT INTO slo_definitions \
         (tenant_id, service_name, environment, sli_type, target, window_days, \
          burn_rate_fast_threshold, burn_rate_slow_threshold, description) \
         VALUES ($1, 'private-svc', 'prod', 'availability', 0.99, 30, 14.4, 1.0, 'Private SLO')",
    )
    .bind(other_tenant)
    .execute(&db)
    .await
    .expect("other tenant SLO inserted");

    let app = build_app(db, tenant);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/slos")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    let items = body["items"].as_array().expect("items array");
    assert!(
        items
            .iter()
            .all(|item| item["service_name"] != "private-svc"),
        "tenant-scoped list must not include other tenant SLOs"
    );
}

#[tokio::test]
async fn post_slo_enforces_sli_outcome_scope_approver_and_commitment_tier() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;

    // Missing sli_outcome/sli_scope cannot be created as approved
    let app = build_app(db.clone(), tenant);
    let unapproved_body = json!({
        "service_name": "checkout",
        "environment": "prod",
        "target": 0.999,
        "window_days": 30,
        "burn_rate_fast_threshold": 14.4,
        "burn_rate_slow_threshold": 1.0,
        "commitment_tier": "internal_slo",
        "approval_status": "approved"
    });
    let bad_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/slos")
                .header("content-type", "application/json")
                .body(Body::from(unapproved_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_resp.status(), StatusCode::BAD_REQUEST);

    // With sli_outcome, sli_scope, and approver, approved internal_slo succeeds
    let approver = Uuid::new_v4();
    let app2 = build_app(db, tenant);
    let ok_body = json!({
        "service_name": "checkout",
        "environment": "prod",
        "target": 0.999,
        "window_days": 30,
        "burn_rate_fast_threshold": 14.4,
        "burn_rate_slow_threshold": 1.0,
        "sli_outcome": "HTTP requests succeeding without 5xx",
        "sli_scope": "ingress HTTP spans excluding /healthz",
        "commitment_tier": "internal_slo",
        "approval_status": "approved",
        "approved_by": approver,
        "owner": { "principal_kind": "user", "principal_id": approver }
    });
    let ok_resp = app2
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/slos")
                .header("content-type", "application/json")
                .body(Body::from(ok_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok_resp.status(), StatusCode::CREATED);
    let created = response_body_json(ok_resp.into_body()).await;
    assert_eq!(
        created["sli_outcome"],
        "HTTP requests succeeding without 5xx"
    );
    assert_eq!(
        created["sli_scope"],
        "ingress HTTP spans excluding /healthz"
    );
    assert_eq!(created["commitment_tier"], "internal_slo");
    assert_eq!(created["approval_status"], "approved");
    assert_eq!(created["approved_by"], approver.to_string());
}

#[tokio::test]
async fn accelerated_burn_rate_produces_at_risk_state() {
    let (db, _pg) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;

    let resp = build_app(db, tenant)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/slos/evaluate-risk")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "target": 0.999,
                        "burn_rate": 16.0,
                        "burn_rate_fast_threshold": 14.4,
                        "remaining_budget_ratio": 0.40
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_body_json(resp.into_body()).await;
    assert_eq!(body["state"], "at_risk");
}

#[tokio::test]
async fn monthly_slo_review_distinguishes_passing_and_failing_objectives() {
    let (db, _pg) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;

    let resp = build_app(db, tenant)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/slos/monthly-review")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "reporting_period": "2026-10",
                        "business_area": "checkout",
                        "items": [
                            {"service_name": "checkout-api", "business_area": "checkout", "reporting_period": "2026-10", "target": 0.999, "attainment": 0.9995},
                            {"service_name": "payment-gateway", "business_area": "checkout", "reporting_period": "2026-10", "target": 0.999, "attainment": 0.992},
                            {"service_name": "search-api", "business_area": "discovery", "reporting_period": "2026-10", "target": 0.995, "attainment": 0.990}
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_body_json(resp.into_body()).await;
    assert_eq!(body["passing_count"], 1);
    assert_eq!(body["failing_count"], 1);
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn historical_report_uses_slo_target_in_force_for_period() {
    let (db, _pg) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;

    let resp = build_app(db, tenant)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/slos/effective-target")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "at": "2026-06-15T00:00:00Z",
                        "revisions": [
                            {"effective_from": "2026-01-01T00:00:00Z", "target": 0.995},
                            {"effective_from": "2026-09-01T00:00:00Z", "target": 0.999}
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_body_json(resp.into_body()).await;
    assert_eq!(body["effective_target"], 0.995);
}

#[tokio::test]
async fn journey_slo_reflects_failures_in_contributing_services() {
    let (db, _pg) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;

    let resp = build_app(db, tenant)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/slos/evaluate-journey")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "journey_name": "Order Checkout",
                        "target": 0.999,
                        "rule": "all_must_pass",
                        "services": [
                            {"service_name": "cart-service", "attainment": 0.9995},
                            {"service_name": "payment-service", "attainment": 0.991}
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_body_json(resp.into_body()).await;
    assert_eq!(body["passed"], false);
    assert_eq!(body["failing_services"][0], "payment-service");
}
