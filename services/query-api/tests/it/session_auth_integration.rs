use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    routing::get,
};
use query_api::{middleware::auth::require_tenant, planner::QueryPlanner, traces::AppState};
use sqlx::postgres::PgPool;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

fn build_app(db: PgPool, auth_service_url: String) -> Router {
    let state = AppState {
        ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        db: db.clone(),
        planner: Arc::new(QueryPlanner),
        llm: None,
        auth_service_url: auth_service_url.clone(),
        metrics: Arc::new(query_api::observability::QueryApiMetrics::new()),
        sessions: query_api::nlq_session::NlqSessionStore::default(),
        admin_service_url: "http://admin-service:4324".into(),
        alert_evaluator_url: "http://alert-evaluator:4322".into(),
        internal_service_token: "test-internal-token".into(),
        http_client: reqwest::Client::new(),
    };
    Router::new()
        .route("/v1/traces/histogram", get(|| async { StatusCode::OK }))
        .layer(axum::middleware::from_fn(require_tenant))
        .layer(axum::Extension(db))
        .layer(axum::Extension(Arc::new(auth_service_url)))
        .with_state(state)
}

#[tokio::test]
async fn session_auth_flow_success() {
    let db = test_support::postgres::shared_pool().await;
    let mock_server = MockServer::start().await;

    let user_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    // Mock auth-service session validation
    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "user_id": user_id.to_string(),
            "tenant_id": tenant_id.to_string(),
            "role": "tenant_admin",
            "environment": "prod"
        })))
        .mount(&mock_server)
        .await;

    let app = build_app(db, mock_server.uri());

    // 1. Request with session cookie
    let req = Request::builder()
        .uri("/v1/traces/histogram")
        .header(header::COOKIE, "session=valid-token")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 2. Request with Bearer token (session)
    let req = Request::builder()
        .uri("/v1/traces/histogram")
        .header(header::AUTHORIZATION, "Bearer valid-token")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn session_auth_tenant_mismatch_rejected() {
    let db = test_support::postgres::shared_pool().await;
    let mock_server = MockServer::start().await;

    let user_id = Uuid::new_v4();
    let session_tenant_id = Uuid::new_v4();
    let requested_tenant_id = Uuid::new_v4();

    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "user_id": user_id.to_string(),
            "tenant_id": session_tenant_id.to_string(),
            "role": "tenant_admin",
            "environment": "prod"
        })))
        .mount(&mock_server)
        .await;

    let app = build_app(db, mock_server.uri());

    // Request with session for tenant A but X-Tenant-ID for tenant B
    let req = Request::builder()
        .uri("/v1/traces/histogram")
        .header(header::COOKIE, "session=valid-token")
        .header("X-Tenant-ID", requested_tenant_id.to_string())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
