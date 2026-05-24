// HTTP integration tests for ingestion token management endpoints:
//   GET    /v1/tokens
//   POST   /v1/tokens
//   DELETE /v1/tokens/{id}              (soft-revoke)
//   POST   /v1/tokens/{id}/renew
//   POST   /v1/tokens/{id}/restore
//   DELETE /v1/tokens/{id}/permanent    (hard-delete)
//
// All tests use a real Postgres instance via Testcontainers and exercise the
// full handler path via tower::ServiceExt::oneshot.

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    middleware as axum_middleware,
    response::Response,
    routing::{delete, get, post},
};
use http_body_util::BodyExt;
use query_api::{middleware::auth::TenantContext, tokens, traces::AppState};
use serde_json::Value;
use sqlx::PgPool;
use std::path::Path;
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use uuid::Uuid;

// ── Postgres helpers ─────────────────────────────────────────────────────────

async fn apply_migrations(pool: &PgPool) {
    let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("migrations/postgres");

    let mut entries: Vec<_> = std::fs::read_dir(&migrations_dir)
        .expect("migrations/postgres must exist")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let sql = std::fs::read_to_string(entry.path()).expect("readable migration");
        sqlx::raw_sql(&sql)
            .execute(pool)
            .await
            .expect("migration applied");
    }
}

async fn start_pool() -> (PgPool, testcontainers::ContainerAsync<Postgres>) {
    let container = Postgres::default()
        .with_tag("17")
        .start()
        .await
        .expect("postgres container started");
    let host = container.get_host().await.expect("host");
    let port = container.get_host_port_ipv4(5432).await.expect("port");
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    let pool = PgPool::connect(&url).await.expect("pool connected");
    apply_migrations(&pool).await;
    (pool, container)
}

// ── Test middleware ───────────────────────────────────────────────────────────

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

// ── App builder ──────────────────────────────────────────────────────────────

fn build_tokens_app(pool: PgPool) -> Router {
    let ch = clickhouse::Client::default().with_url("http://127.0.0.1:19999");
    let state = AppState {
        ch,
        db: pool,
        planner: std::sync::Arc::new(query_api::planner::QueryPlanner),
        llm: None,
        auth_service_url: "http://auth-service:4319".into(),
        metrics: std::sync::Arc::new(query_api::observability::QueryApiMetrics::new()),
    };
    Router::new()
        .route("/v1/tokens", get(tokens::list_tokens))
        .route("/v1/tokens", post(tokens::create_token))
        .route("/v1/tokens/{id}", delete(tokens::revoke_token))
        .route("/v1/tokens/{id}/renew", post(tokens::renew_token))
        .route("/v1/tokens/{id}/restore", post(tokens::restore_token))
        .route("/v1/tokens/{id}/permanent", delete(tokens::delete_token))
        .layer(axum_middleware::from_fn(inject_tenant_ctx))
        .with_state(state)
}

// ── Seed helpers ─────────────────────────────────────────────────────────────

/// The dev tenant UUID seeded by migrations (001_create_tenants.sql).
const DEV_TENANT_ID: &str = "00000000-0000-0000-0000-000000000001";

fn dev_tenant_id() -> Uuid {
    DEV_TENANT_ID.parse().unwrap()
}

fn json_body(value: &serde_json::Value) -> Body {
    Body::from(serde_json::to_vec(value).unwrap())
}

fn tenant_post(uri: &str, tenant_id: Uuid, body: Body) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("Content-Type", "application/json")
        .body(body)
        .unwrap()
}

fn tenant_req(method: &str, uri: &str, tenant_id: Uuid) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("X-Tenant-ID", tenant_id.to_string())
        .body(Body::empty())
        .unwrap()
}

async fn body_json(body: axum::body::Body) -> Value {
    let bytes = body.collect().await.expect("body collected").to_bytes();
    serde_json::from_slice(&bytes).expect("valid JSON")
}

// ── Helper: create a token and return its id + plaintext ─────────────────────

async fn create_one_token(app: Router, tenant_id: Uuid) -> (Router, String, String) {
    let req = tenant_post(
        "/v1/tokens",
        tenant_id,
        json_body(&serde_json::json!({"name": "test-token", "environment": "test"})),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    let id = body["id"].as_str().unwrap().to_string();
    let plaintext = body["plaintext"].as_str().unwrap().to_string();
    (app, id, plaintext)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_token_returns_plaintext() {
    let (pool, _container) = start_pool().await;
    let tenant_id = dev_tenant_id();
    let app = build_tokens_app(pool);

    let req = tenant_post(
        "/v1/tokens",
        tenant_id,
        json_body(&serde_json::json!({"name": "my-service", "environment": "prod"})),
    );
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert!(body["plaintext"].as_str().is_some_and(|s| s.len() == 64));
    assert_eq!(body["revoked"], false);
}

#[tokio::test]
async fn revoke_token_prevents_listing_as_active() {
    let (pool, _container) = start_pool().await;
    let tenant_id = dev_tenant_id();
    let app = build_tokens_app(pool);

    let (app, id, _) = create_one_token(app, tenant_id).await;

    // Revoke it.
    let resp = app
        .clone()
        .oneshot(tenant_req("DELETE", &format!("/v1/tokens/{id}"), tenant_id))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // List: should appear as revoked.
    let resp = app
        .oneshot(tenant_req("GET", "/v1/tokens", tenant_id))
        .await
        .unwrap();
    let body = body_json(resp.into_body()).await;
    let token = &body["tokens"][0];
    assert_eq!(token["revoked"], true);
}

#[tokio::test]
async fn renew_token_returns_new_plaintext_and_clears_revoked() {
    let (pool, _container) = start_pool().await;
    let tenant_id = dev_tenant_id();
    let app = build_tokens_app(pool);

    // Create then revoke.
    let (app, id, original_plaintext) = create_one_token(app, tenant_id).await;
    let resp = app
        .clone()
        .oneshot(tenant_req("DELETE", &format!("/v1/tokens/{id}"), tenant_id))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Renew the (revoked) token.
    let resp = app
        .clone()
        .oneshot(tenant_post(
            &format!("/v1/tokens/{id}/renew"),
            tenant_id,
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    let new_plaintext = body["plaintext"].as_str().unwrap();
    assert_ne!(new_plaintext, original_plaintext, "plaintext must rotate");
    assert_eq!(body["revoked"], false, "renew clears revoked_at");

    // List confirms the token is active again.
    let resp = app
        .oneshot(tenant_req("GET", "/v1/tokens", tenant_id))
        .await
        .unwrap();
    let list = body_json(resp.into_body()).await;
    assert_eq!(list["tokens"][0]["revoked"], false);
}

#[tokio::test]
async fn restore_token_clears_revoked_at() {
    let (pool, _container) = start_pool().await;
    let tenant_id = dev_tenant_id();
    let app = build_tokens_app(pool);

    let (app, id, _) = create_one_token(app, tenant_id).await;

    // Revoke then restore.
    app.clone()
        .oneshot(tenant_req("DELETE", &format!("/v1/tokens/{id}"), tenant_id))
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(tenant_post(
            &format!("/v1/tokens/{id}/restore"),
            tenant_id,
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let list_resp = app
        .oneshot(tenant_req("GET", "/v1/tokens", tenant_id))
        .await
        .unwrap();
    let list = body_json(list_resp.into_body()).await;
    assert_eq!(list["tokens"][0]["revoked"], false);
}

#[tokio::test]
async fn restore_already_active_token_returns_404() {
    let (pool, _container) = start_pool().await;
    let tenant_id = dev_tenant_id();
    let app = build_tokens_app(pool);

    let (app, id, _) = create_one_token(app, tenant_id).await;

    // Restore an active token — should 404 (nothing to restore).
    let resp = app
        .oneshot(tenant_post(
            &format!("/v1/tokens/{id}/restore"),
            tenant_id,
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_token_permanently_removes_row() {
    let (pool, _container) = start_pool().await;
    let tenant_id = dev_tenant_id();
    let app = build_tokens_app(pool);

    let (app, id, _) = create_one_token(app, tenant_id).await;

    // Must revoke before hard-delete.
    app.clone()
        .oneshot(tenant_req("DELETE", &format!("/v1/tokens/{id}"), tenant_id))
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(tenant_req(
            "DELETE",
            &format!("/v1/tokens/{id}/permanent"),
            tenant_id,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // The hard-deleted token must not appear in the list (seeded tokens may remain).
    let list_resp = app
        .oneshot(tenant_req("GET", "/v1/tokens", tenant_id))
        .await
        .unwrap();
    let list = body_json(list_resp.into_body()).await;
    let ids: Vec<&str> = list["tokens"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["id"].as_str())
        .collect();
    assert!(
        !ids.contains(&id.as_str()),
        "hard-deleted token must not appear in list"
    );
}

#[tokio::test]
async fn delete_active_token_returns_404() {
    let (pool, _container) = start_pool().await;
    let tenant_id = dev_tenant_id();
    let app = build_tokens_app(pool);

    let (app, id, _) = create_one_token(app, tenant_id).await;

    // Attempt hard-delete on an active token — should 404.
    let resp = app
        .oneshot(tenant_req(
            "DELETE",
            &format!("/v1/tokens/{id}/permanent"),
            tenant_id,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
