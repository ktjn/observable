// Integration tests for Task 3's `platform_config.llm_provider` plumbing:
//   - `fetch_provider` resolves a remote config value over HTTP from admin-service.
//   - `POST /v1/nlq` returns 503 for a Webllm-configured tenant, and is unaffected for
//     the default Remote provider (existing shorthand-fallback behavior still applies).
//   - `POST /v1/nlq/prepare` behavior is unchanged for both provider values (Task 2's
//     pipeline logic doesn't branch on provider — this task only adds the resolution
//     call).

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    middleware as axum_middleware,
    response::Response,
    routing::post,
};
use clickhouse::Client as ChClient;
use http_body_util::BodyExt;
use query_api::{
    llm_adapter,
    llm_config::{LlmProvider, fetch_provider},
    middleware::auth::TenantContext,
    planner::QueryPlanner,
    traces,
};
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TEST_TOKEN: &str = "test-internal-token";

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn inject_tenant_ctx(mut req: Request<Body>, next: axum_middleware::Next) -> Response {
    let tenant_id: Uuid = req
        .headers()
        .get("X-Tenant-ID")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or_default();
    req.extensions_mut().insert(TenantContext {
        tenant_id,
        user_id: None,
        role: "member".into(),
    });
    next.run(req).await
}

fn fake_ch() -> ChClient {
    ChClient::default().with_url("http://127.0.0.1:19999")
}

async fn start_admin_mock_with_provider(provider: Option<&str>) -> MockServer {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/internal/config/llm"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "api_key": null,
            "url": null,
            "model": null,
            "provider": provider,
            "webllm_model": null
        })))
        .mount(&mock)
        .await;

    // Also mount default empty schema responses so prepare/query pipelines can fetch schema context
    Mock::given(method("GET"))
        .and(path("/internal/schemas/metrics/fields"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;

    mock
}

fn build_nlq_app(ch: ChClient, admin_service_url: String) -> Router {
    let state = traces::AppState {
        ch,
        planner: Arc::new(QueryPlanner),
        llm: None,
        auth_service_url: "http://auth-service:4319".into(),
        metrics: Arc::new(query_api::observability::QueryApiMetrics::new()),
        sessions: query_api::nlq_session::NlqSessionStore::default(),
        admin_service_url,
        alert_evaluator_url: "http://alert-evaluator:4322".into(),
        internal_service_token: TEST_TOKEN.into(),
        http_client: reqwest::Client::new(),
    };
    Router::new()
        .route("/v1/nlq", post(llm_adapter::handle_nlq_query))
        .route("/v1/nlq/prepare", post(llm_adapter::handle_nlq_prepare))
        .layer(axum_middleware::from_fn(inject_tenant_ctx))
        .with_state(state)
}

fn post_req(uri: &str, tenant_id: Uuid, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn body_json(body: axum::body::Body) -> Value {
    let bytes = body.collect().await.expect("body").to_bytes();
    serde_json::from_slice(&bytes).expect("valid JSON")
}

// ── fetch_provider (Admin-backed cases) ───────────────────────────────────────

#[tokio::test]
async fn fetch_provider_reads_webllm_from_remote_when_no_env() {
    let admin_mock = start_admin_mock_with_provider(Some("webllm")).await;
    let client = reqwest::Client::new();

    assert_eq!(
        fetch_provider(&client, &admin_mock.uri(), TEST_TOKEN).await,
        LlmProvider::Webllm
    );
}

#[tokio::test]
async fn fetch_provider_invalid_remote_value_falls_back_to_remote() {
    let admin_mock = start_admin_mock_with_provider(Some("not-a-real-provider")).await;
    let client = reqwest::Client::new();

    assert_eq!(
        fetch_provider(&client, &admin_mock.uri(), TEST_TOKEN).await,
        LlmProvider::Remote
    );
}

#[tokio::test]
async fn fetch_provider_defaults_to_remote_in_fresh_admin() {
    let admin_mock = start_admin_mock_with_provider(None).await;
    let client = reqwest::Client::new();

    assert_eq!(
        fetch_provider(&client, &admin_mock.uri(), TEST_TOKEN).await,
        LlmProvider::Remote
    );
}

// ── POST /v1/nlq ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn nlq_query_returns_503_when_provider_is_webllm() {
    let admin_mock = start_admin_mock_with_provider(Some("webllm")).await;
    let app = build_nlq_app(fake_ch(), admin_mock.uri());
    let tenant = Uuid::new_v4();

    let resp = app
        .oneshot(post_req(
            "/v1/nlq",
            tenant,
            r#"{"question":"how many errors happened","mode":"execute"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    let json = body_json(resp.into_body()).await;
    assert!(
        json["error"].as_str().unwrap().contains("/v1/nlq/prepare"),
        "503 body should point callers at the two-phase endpoints, got: {json}"
    );
}

/// Default provider (no remote row) must behave completely unchanged: no LLM configured →
/// shorthand fallback, not a 503 (mirrors `nlq_shorthand_integration.rs`'s existing
/// coverage of this path, re-asserted here specifically to confirm this task didn't
/// alter Remote's behavior).
#[tokio::test]
async fn nlq_query_remote_default_falls_back_to_shorthand_unchanged() {
    let admin_mock = start_admin_mock_with_provider(None).await;
    let app = build_nlq_app(fake_ch(), admin_mock.uri());
    let tenant = Uuid::new_v4();

    let resp = app
        .oneshot(post_req(
            "/v1/nlq",
            tenant,
            r#"{"question":"service:checkout","mode":"interpret"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp.into_body()).await;
    assert_eq!(json["type"], "ir");
}

// ── POST /v1/nlq/prepare ─────────────────────────────────────────────────────

/// `/prepare` must reach `Prepared` for an ordinary question regardless of provider —
/// per the brief, provider resolution is a no-op for this endpoint's observable
/// behavior (WebLLM tenants are the intended callers; Remote tenants behave exactly as
/// Task 2 built).
#[tokio::test]
async fn nlq_prepare_reaches_prepared_for_remote_provider() {
    let admin_mock = start_admin_mock_with_provider(Some("remote")).await;
    let app = build_nlq_app(fake_ch(), admin_mock.uri());
    let tenant = Uuid::new_v4();

    let resp = app
        .oneshot(post_req(
            "/v1/nlq/prepare",
            tenant,
            r#"{"question":"how many errors happened","mode":"execute"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp.into_body()).await;
    assert_eq!(json["type"], "prepared");
}

#[tokio::test]
async fn nlq_prepare_reaches_prepared_for_webllm_provider() {
    let admin_mock = start_admin_mock_with_provider(Some("webllm")).await;
    let app = build_nlq_app(fake_ch(), admin_mock.uri());
    let tenant = Uuid::new_v4();

    let resp = app
        .oneshot(post_req(
            "/v1/nlq/prepare",
            tenant,
            r#"{"question":"how many errors happened","mode":"execute"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "webllm is the intended provider for /prepare — must not 503 or otherwise change behavior"
    );
    let json = body_json(resp.into_body()).await;
    assert_eq!(json["type"], "prepared");
}

#[tokio::test]
async fn nlq_prepare_shorthand_bypass_unaffected_by_webllm_provider() {
    let admin_mock = start_admin_mock_with_provider(Some("webllm")).await;
    let app = build_nlq_app(fake_ch(), admin_mock.uri());
    let tenant = Uuid::new_v4();

    let resp = app
        .oneshot(post_req(
            "/v1/nlq/prepare",
            tenant,
            r#"{"question":"/service:checkout error","mode":"interpret"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp.into_body()).await;
    assert_eq!(json["type"], "final");
    assert_eq!(json["response"]["type"], "ir");
}
