// HTTP integration tests for tenant discovery endpoints:
//   GET /v1/tenants
//   GET /v1/tenants/{id}/environments
//
// Both endpoints are outside the tenant-auth middleware (bootstrap resources).
// Tests use a real Postgres instance via Testcontainers and exercise the full
// handler path via tower::ServiceExt::oneshot.

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use http_body_util::BodyExt;
use query_api::{tenants, traces::AppState};
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

// ── App builder ──────────────────────────────────────────────────────────────

fn build_tenants_app(pool: PgPool) -> Router {
    let ch = clickhouse::Client::default().with_url("http://127.0.0.1:19999");
    let state = AppState {
        ch,
        db: pool,
        planner: std::sync::Arc::new(query_api::planner::QueryPlanner),
        llm: None,
        auth_service_url: "http://auth-service:4319".into(),
        http_client: reqwest::Client::new(),
        metrics: std::sync::Arc::new(query_api::observability::QueryApiMetrics::new()),
    };
    // No tenant-auth middleware — these are bootstrap endpoints.
    Router::new()
        .route("/v1/tenants", get(tenants::list_tenants))
        .route(
            "/v1/tenants/{id}/environments",
            get(tenants::list_tenant_environments),
        )
        .with_state(state)
}

/// Seeds an `api_keys` row directly (token issuance now lives in admin-service,
/// out of scope for query-api's test app; we only need the row's side effect of
/// making an environment visible to `list_tenant_environments`).
async fn seed_token_environment(pool: &PgPool, tenant_id: Uuid, name: &str, environment: &str) {
    let hash = format!("{name}-{environment}-test-hash");
    sqlx::query(
        r#"
        INSERT INTO api_keys (tenant_id, key_hash, name, environment, role)
        VALUES ($1, $2, $3, $4, 'member')
        "#,
    )
    .bind(tenant_id)
    .bind(hash)
    .bind(name)
    .bind(environment)
    .execute(pool)
    .await
    .unwrap();
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

async fn body_json(body: Body) -> Value {
    let bytes = body.collect().await.expect("body collected").to_bytes();
    serde_json::from_slice(&bytes).expect("valid JSON")
}

// ── Tests: GET /v1/tenants ────────────────────────────────────────────────────

#[tokio::test]
async fn list_tenants_returns_seeded_tenant() {
    let pool = test_support::postgres::shared_pool().await;
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
    let pool = test_support::postgres::shared_pool().await;
    let app = build_tenants_app(pool);

    let resp = app.oneshot(plain_get("/v1/tenants")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp.into_body()).await;
    let first = &body["tenants"][0];
    assert!(first["id"].is_string(), "id must be a string");
    assert!(first["name"].is_string(), "name must be a string");
}

// ── Tests: GET /v1/tenants/{id}/environments ───────────────────────────────────

#[tokio::test]
async fn list_tenant_environments_returns_seeded_environments() {
    let pool = test_support::postgres::shared_pool().await;
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
    let pool = test_support::postgres::shared_pool().await;
    let tenant_id = dev_tenant_id();

    // Seed a token for a new environment (token issuance itself is exercised
    // in admin-service's own test suite; here we only need the side effect).
    seed_token_environment(&pool, tenant_id, "ci-token", "ci-unique-env").await;

    let app = build_tenants_app(pool);

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
    let pool = test_support::postgres::shared_pool().await;
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
    let pool = test_support::postgres::shared_pool().await;
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
