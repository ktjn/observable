// HTTP integration tests for tenant discovery endpoints:
//   GET /v1/tenants
//   GET /v1/tenants/{id}/environments
//
// Both endpoints are outside the tenant-auth middleware (bootstrap resources).
// Tests use a real Postgres instance via Testcontainers and exercise the full
// handler path via tower::ServiceExt::oneshot.

use admin_service::{AdminServiceAppState, auth_directory::AuthDirectory, observability, tenants};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::PgPool;
use std::path::Path;
use std::sync::Arc;
use tower::ServiceExt;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ── Container helpers ────────────────────────────────────────────────────────

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
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
            .execute(pool)
            .await
            .expect("migration applied");
    }
}

async fn start_pool() -> (PgPool, test_support::postgres::TestDatabase) {
    test_support::postgres::isolated_pool().await
}

// ── App builder ──────────────────────────────────────────────────────────────

fn build_tenants_app(pool: PgPool) -> Router {
    build_tenants_app_with_auth(pool, "http://127.0.0.1:1")
}

/// `auth_url` serves auth-service's directory views; key environments are auth data.
fn build_tenants_app_with_auth(pool: PgPool, auth_url: &str) -> Router {
    let state = AdminServiceAppState {
        db: pool,
        ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        auth_service_url: "http://auth-service:4319".into(),
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AdminServiceMetrics::new()),
        producer: None,
    };
    // No tenant-auth middleware — these are bootstrap endpoints.
    Router::new()
        .route("/v1/tenants", get(tenants::list_tenants))
        .route(
            "/v1/tenants/{id}/environments",
            get(tenants::list_tenant_environments),
        )
        .layer(axum::Extension(AuthDirectory::new(
            auth_url,
            "test-internal-token",
            reqwest::Client::new(),
        )))
        .with_state(state)
}

// ── Seed constants ────────────────────────────────────────────────────────────

const DEV_TENANT_ID: &str = "00000000-0000-0000-0000-000000000001";

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

// ── Tests: GET /v1/tenants/{id}/environments ───────────────────────────────────
// Which environments count (distinct, active keys only) is auth-service's contract, covered in
// services/auth-service/tests/key_environments_integration.rs.

#[tokio::test]
async fn list_tenant_environments_come_from_auth_key_environments() {
    let (pool, _container) = start_pool().await;
    let auth = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/internal/key-environments"))
        .and(query_param("tenant_id", DEV_TENANT_ID))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"environments": ["observable", "testbench"]})),
        )
        .mount(&auth)
        .await;
    let app = build_tenants_app_with_auth(pool, &auth.uri());

    let resp = app
        .oneshot(plain_get(&format!(
            "/v1/tenants/{DEV_TENANT_ID}/environments"
        )))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(
        body,
        serde_json::json!({"environments": [{"environment": "observable"}, {"environment": "testbench"}]})
    );
}

#[tokio::test]
async fn list_tenant_environments_fail_closed_when_auth_is_unavailable() {
    let (pool, _container) = start_pool().await;
    let app = build_tenants_app(pool);
    let resp = app
        .oneshot(plain_get(&format!(
            "/v1/tenants/{DEV_TENANT_ID}/environments"
        )))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn migrations_enforce_schema_ownership_and_zero_cross_schema_fks() {
    let (pool, _container) = start_pool().await;

    let has_control_audit_log: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM pg_tables
            WHERE schemaname = 'control' AND tablename = 'query_audit_log'
        )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        has_control_audit_log,
        "query_audit_log must reside in the control schema"
    );

    let cross_schema_fks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM pg_constraint con
         JOIN pg_class cl_src      ON cl_src.oid = con.conrelid
         JOIN pg_namespace ns_src  ON ns_src.oid = cl_src.relnamespace
         JOIN pg_class cl_dst      ON cl_dst.oid = con.confrelid
         JOIN pg_namespace ns_dst  ON ns_dst.oid = cl_dst.relnamespace
         WHERE con.contype = 'f'
           AND ns_src.nspname IN ('auth', 'control', 'alerting')
           AND ns_dst.nspname <> ns_src.nspname",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        cross_schema_fks, 0,
        "expected zero cross-schema foreign keys in auth/control/alerting"
    );

    // Replay all migrations on a bare connection (matching postgres-setup / migration-job)
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .unwrap();
    let server = test_support::postgres::server_url().await;
    let replay_pool = PgPool::connect(&format!("{server}/{database}"))
        .await
        .expect("replay pool connected");
    apply_migrations(&replay_pool).await;

    let cross_schema_fks_after_replay: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM pg_constraint con
         JOIN pg_class cl_src      ON cl_src.oid = con.conrelid
         JOIN pg_namespace ns_src  ON ns_src.oid = cl_src.relnamespace
         JOIN pg_class cl_dst      ON cl_dst.oid = con.confrelid
         JOIN pg_namespace ns_dst  ON ns_dst.oid = cl_dst.relnamespace
         WHERE con.contype = 'f'
           AND ns_src.nspname IN ('auth', 'control', 'alerting')
           AND ns_dst.nspname <> ns_src.nspname",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        cross_schema_fks_after_replay, 0,
        "expected zero cross-schema foreign keys after migration replay"
    );

    // Verify per-component roles have access only to their owned schemas
    let role_isolation_ok: bool = sqlx::query_scalar(
        "SELECT
            has_table_privilege('observable_auth_role', 'auth.api_keys', 'SELECT, INSERT')
            AND NOT has_table_privilege('observable_auth_role', 'control.tenants', 'SELECT')
            AND NOT has_table_privilege('observable_auth_role', 'alerting.alert_rules', 'SELECT')
            AND has_table_privilege('observable_control_role', 'control.tenants', 'SELECT, INSERT')
            AND has_table_privilege('observable_control_role', 'control.query_audit_log', 'SELECT, INSERT')
            AND NOT has_table_privilege('observable_control_role', 'auth.api_keys', 'SELECT')
            AND has_table_privilege('observable_alerting_role', 'alerting.alert_rules', 'SELECT, INSERT')
            AND NOT has_table_privilege('observable_alerting_role', 'control.tenants', 'SELECT')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        role_isolation_ok,
        "component PostgreSQL roles must only have privileges on their owned schema tables"
    );

    // Verify per-component login users inherit the schema-scoped roles and isolation
    let user_isolation_ok: bool = sqlx::query_scalar(
        "SELECT
            has_table_privilege('observable_auth', 'auth.api_keys', 'SELECT, INSERT')
            AND NOT has_table_privilege('observable_auth', 'control.tenants', 'SELECT')
            AND NOT has_table_privilege('observable_auth', 'alerting.alert_rules', 'SELECT')
            AND has_table_privilege('observable_control', 'control.tenants', 'SELECT, INSERT')
            AND has_table_privilege('observable_control', 'control.query_audit_log', 'SELECT, INSERT')
            AND NOT has_table_privilege('observable_control', 'auth.api_keys', 'SELECT')
            AND has_table_privilege('observable_alerting', 'alerting.alert_rules', 'SELECT, INSERT')
            AND NOT has_table_privilege('observable_alerting', 'control.tenants', 'SELECT')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        user_isolation_ok,
        "component PostgreSQL login users must inherit schema-scoped permissions and remain isolated"
    );
}
