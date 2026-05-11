// HTTP integration tests for tenant discovery endpoints:
//   GET /v1/tenants
//   GET /v1/tenants/:id/environments
//
// Both endpoints are outside the tenant-auth middleware (bootstrap resources).
// Tests use a real Postgres instance via Testcontainers and exercise the full
// handler path via tower::ServiceExt::oneshot.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    routing::{get, post},
    Router,
};
use http_body_util::BodyExt;
use query_api::{middleware::auth::TenantContext, tenants, tokens, traces::AppState};
use serde_json::Value;
use sqlx::PgPool;
use std::path::Path;
use testcontainers::{runners::AsyncRunner, ImageExt};
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
        .with_tag("16")
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

/// Injects TenantContext from X-Tenant-ID without verifying an API key.
/// Used for routes that need a TenantContext but where auth is not under test.
async fn inject_tenant_ctx(mut req: Request<Body>, next: Next) -> Response {
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

fn build_tenants_app(pool: PgPool) -> Router {
    let ch = clickhouse::Client::default().with_url("http://127.0.0.1:19999");
    let state = AppState {
        ch,
        db: pool,
        planner: std::sync::Arc::new(query_api::planner::QueryPlanner),
        llm: None,
        auth_service_url: "http://auth-service:4319".into(),
    };
    // No tenant-auth middleware — these are bootstrap endpoints.
    Router::new()
        .route("/v1/tenants", get(tenants::list_tenants))
        .route(
            "/v1/tenants/:id/environments",
            get(tenants::list_tenant_environments),
        )
        // Tokens route seeds environments in tests; auth is not under test here.
        .route(
            "/v1/tokens",
            post(tokens::create_token).layer(axum::middleware::from_fn(inject_tenant_ctx)),
        )
        .with_state(state)
}

// ── Seed constants ────────────────────────────────────────────────────────────

const DEV_TENANT_ID: &str = "00000000-0000-0000-0000-000000000001";

fn dev_tenant_id() -> Uuid {
    DEV_TENANT_ID.parse().unwrap()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn plain_get(uri: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

fn tenant_post_json(uri: &str, tenant_id: Uuid, value: &serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(value).unwrap()))
        .unwrap()
}

async fn body_json(body: Body) -> Value {
    let bytes = body.collect().await.expect("body collected").to_bytes();
    serde_json::from_slice(&bytes).expect("valid JSON")
}

// ── Tests: GET /v1/tenants ────────────────────────────────────────────────────

#[tokio::test]
async fn list_tenants_returns_seeded_tenant() {
    let (pool, _container) = start_pool().await;
    let app = build_tenants_app(pool);

    let resp = app.oneshot(plain_get("/v1/tenants")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp.into_body()).await;
    let tenants = body["tenants"].as_array().expect("tenants array");
    // Migrations seed at least the local-dev tenant.
    assert!(!tenants.is_empty(), "expected at least one seeded tenant");
    let ids: Vec<&str> = tenants.iter().filter_map(|t| t["id"].as_str()).collect();
    assert!(
        ids.contains(&DEV_TENANT_ID),
        "expected dev tenant id in list"
    );
}

#[tokio::test]
async fn list_tenants_response_shape() {
    let (pool, _container) = start_pool().await;
    let app = build_tenants_app(pool);

    let resp = app.oneshot(plain_get("/v1/tenants")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp.into_body()).await;
    let first = &body["tenants"][0];
    assert!(first["id"].is_string(), "id must be a string");
    assert!(first["name"].is_string(), "name must be a string");
}

// ── Tests: GET /v1/tenants/:id/environments ───────────────────────────────────

#[tokio::test]
async fn list_tenant_environments_returns_seeded_environments() {
    let (pool, _container) = start_pool().await;
    let app = build_tenants_app(pool);

    let resp = app
        .oneshot(plain_get(&format!(
            "/v1/tenants/{DEV_TENANT_ID}/environments"
        )))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp.into_body()).await;
    let envs = body["environments"].as_array().expect("environments array");
    // Migrations seed 'testbench' and 'observable' environments.
    let env_names: Vec<&str> = envs
        .iter()
        .filter_map(|e| e["environment"].as_str())
        .collect();
    assert!(
        env_names.contains(&"testbench") || env_names.contains(&"observable"),
        "expected at least one seeded environment, got: {env_names:?}"
    );
}

#[tokio::test]
async fn list_tenant_environments_includes_newly_created_token_environment() {
    let (pool, _container) = start_pool().await;
    let tenant_id = dev_tenant_id();
    let app = build_tenants_app(pool);

    // Create a token for a new environment.
    let create_req = tenant_post_json(
        "/v1/tokens",
        tenant_id,
        &serde_json::json!({"name": "ci-token", "environment": "ci-unique-env"}),
    );
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);

    // The new environment should now appear in the list.
    let resp = app
        .oneshot(plain_get(&format!(
            "/v1/tenants/{DEV_TENANT_ID}/environments"
        )))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp.into_body()).await;
    let env_names: Vec<&str> = body["environments"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["environment"].as_str())
        .collect();
    assert!(
        env_names.contains(&"ci-unique-env"),
        "newly created environment must appear; got: {env_names:?}"
    );
}

#[tokio::test]
async fn list_tenant_environments_excludes_revoked_token_only_environments() {
    let (pool, _container) = start_pool().await;
    let tenant_id = dev_tenant_id();

    // Insert a token directly via SQL with a revoked_at set so no active
    // token covers this environment.
    sqlx::query(
        r#"
        INSERT INTO api_keys (tenant_id, key_hash, name, environment, role, revoked_at)
        VALUES ($1, 'deadbeefdeadbeefdeadbeefdeadbeef00000000000000000000000000000000',
                'revoked-token', 'revoked-only-env', 'member', now())
        "#,
    )
    .bind(tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    let app = build_tenants_app(pool);

    let resp = app
        .oneshot(plain_get(&format!(
            "/v1/tenants/{DEV_TENANT_ID}/environments"
        )))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp.into_body()).await;
    let env_names: Vec<&str> = body["environments"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["environment"].as_str())
        .collect();
    assert!(
        !env_names.contains(&"revoked-only-env"),
        "revoked-only environment must not appear; got: {env_names:?}"
    );
}

#[tokio::test]
async fn list_tenant_environments_unknown_tenant_returns_empty() {
    let (pool, _container) = start_pool().await;
    let app = build_tenants_app(pool);
    let unknown_id = Uuid::new_v4();

    let resp = app
        .oneshot(plain_get(&format!("/v1/tenants/{unknown_id}/environments")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp.into_body()).await;
    let envs = body["environments"].as_array().unwrap();
    assert!(envs.is_empty(), "unknown tenant should return empty list");
}
