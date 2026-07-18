// Integration tests for Task 3's `platform_config.llm_provider` plumbing:
//   - `fetch_provider` resolves a real DB-stored value (env-only cases are covered by
//     fast unit tests in `llm_config.rs` itself).
//   - `POST /v1/nlq` returns 503 for a Webllm-configured tenant, and is unaffected for
//     the default Remote provider (existing shorthand-fallback behavior still applies).
//   - `POST /v1/nlq/prepare` behavior is unchanged for both provider values (Task 2's
//     pipeline logic doesn't branch on provider — this task only adds the resolution
//     call).
//
// All tests use a real Postgres container (via `test_support::postgres::shared_pool`,
// one fresh database per test) so `platform_config` reads/writes are legitimate.

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
use sqlx::postgres::PgPool;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

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

fn build_nlq_app(ch: ChClient, db: PgPool) -> Router {
    let state = traces::AppState {
        ch,
        db,
        planner: Arc::new(QueryPlanner),
        llm: None,
        auth_service_url: "http://auth-service:4319".into(),
        http_client: reqwest::Client::new(),
        metrics: Arc::new(query_api::observability::QueryApiMetrics::new()),
        sessions: query_api::nlq_session::NlqSessionStore::default(),
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

async fn set_llm_provider(db: &PgPool, value: &str) {
    sqlx::query(
        "INSERT INTO platform_config (key, value, updated_at)
         VALUES ('llm_provider', $1, now())
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(value)
    .execute(db)
    .await
    .expect("llm_provider upserted");
}

// ── fetch_provider (DB-backed cases) ────────────────────────────────────────

#[tokio::test]
async fn fetch_provider_reads_webllm_from_db_when_no_env() {
    let db = test_support::postgres::shared_pool().await;
    set_llm_provider(&db, "webllm").await;

    assert_eq!(fetch_provider(&db).await, LlmProvider::Webllm);
}

#[tokio::test]
async fn fetch_provider_invalid_db_value_falls_back_to_remote() {
    let db = test_support::postgres::shared_pool().await;
    set_llm_provider(&db, "not-a-real-provider").await;

    assert_eq!(fetch_provider(&db).await, LlmProvider::Remote);
}

#[tokio::test]
async fn fetch_provider_defaults_to_remote_in_fresh_db() {
    let db = test_support::postgres::shared_pool().await;

    assert_eq!(fetch_provider(&db).await, LlmProvider::Remote);
}

// ── POST /v1/nlq ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn nlq_query_returns_503_when_provider_is_webllm() {
    let db = test_support::postgres::shared_pool().await;
    set_llm_provider(&db, "webllm").await;
    let app = build_nlq_app(fake_ch(), db);
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

/// Default provider (no DB row) must behave completely unchanged: no LLM configured →
/// shorthand fallback, not a 503 (mirrors `nlq_shorthand_integration.rs`'s existing
/// coverage of this path, re-asserted here specifically to confirm this task didn't
/// alter Remote's behavior).
#[tokio::test]
async fn nlq_query_remote_default_falls_back_to_shorthand_unchanged() {
    let db = test_support::postgres::shared_pool().await;
    let app = build_nlq_app(fake_ch(), db);
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
    let db = test_support::postgres::shared_pool().await;
    let app = build_nlq_app(fake_ch(), db);
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
    let db = test_support::postgres::shared_pool().await;
    set_llm_provider(&db, "webllm").await;
    let app = build_nlq_app(fake_ch(), db);
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
    let db = test_support::postgres::shared_pool().await;
    set_llm_provider(&db, "webllm").await;
    let app = build_nlq_app(fake_ch(), db);
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
