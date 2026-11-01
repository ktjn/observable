// HTTP integration tests for tenant retention configuration and hard-purge workflows:
//   GET    /v1/tenants/{id}/config
//   PUT    /v1/tenants/{id}/config
//   DELETE /v1/tenants/{id}
//
// Tests use a real Postgres instance via Testcontainers and exercise the full
// handler path via tower::ServiceExt::oneshot.

use admin_service::{
    AdminServiceAppState,
    middleware::auth::TenantContext,
    observability,
    tenants::{self, PurgeTenantResponse, TenantConfigResponse, UpdateTenantConfigRequest},
};
use axum::{
    Extension, Router,
    body::Body,
    http::{Request, StatusCode},
    routing::{delete, get},
};
use http_body_util::BodyExt;
use sqlx::PgPool;
use std::path::Path;
use std::sync::Arc;
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use uuid::Uuid;

// ── Dev credentials (matching migrations seed) ──────────────────────────────
const DEV_TENANT_ID: &str = "00000000-0000-0000-0000-000000000002";

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

async fn start_pool() -> (PgPool, testcontainers::ContainerAsync<Postgres>) {
    let container = Postgres::default()
        .with_tag("17")
        .start()
        .await
        .expect("postgres container started");
    let host = container.get_host().await.expect("host");
    let port = container.get_host_port_ipv4(5432).await.expect("port");
    let url = observable_config::with_search_path(&format!(
        "postgres://postgres:postgres@{host}:{port}/postgres"
    ));
    let pool = PgPool::connect(&url).await.expect("pool connected");
    apply_migrations(&pool).await;
    (pool, container)
}

// ── App builder ──────────────────────────────────────────────────────────────

fn build_admin_tenants_app(pool: PgPool, ctx: TenantContext) -> Router {
    let state = AdminServiceAppState {
        db: pool,
        ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        auth_service_url: "http://auth-service:4319".into(),
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AdminServiceMetrics::new()),
        producer: None,
    };
    Router::new()
        .route(
            "/v1/tenants/{id}/config",
            get(tenants::handle_get_tenant_config).put(tenants::handle_update_tenant_config),
        )
        .route("/v1/tenants/{id}", delete(tenants::handle_purge_tenant))
        .layer(Extension(ctx))
        .with_state(state)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_tenant_config_get_and_update() {
    let (pool, _container) = start_pool().await;
    let tenant_id = Uuid::parse_str(DEV_TENANT_ID).unwrap();
    let ctx = TenantContext {
        tenant_id,
        user_id: Some(Uuid::new_v4()),
        role: "tenant_admin".to_string(),
    };
    let app = build_admin_tenants_app(pool.clone(), ctx.clone());

    // 1. GET initial config
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/tenants/{tenant_id}/config"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let config: TenantConfigResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(config.tenant_id, tenant_id);
    assert_eq!(config.retention_days, 30);
    assert_eq!(config.ingest_quota_per_sec, 100);

    // 2. PUT updated config
    let update_req_body = serde_json::to_vec(&UpdateTenantConfigRequest {
        retention_days: 90,
        ingest_quota_per_sec: 500,
    })
    .unwrap();
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/tenants/{tenant_id}/config"))
        .header("Content-Type", "application/json")
        .body(Body::from(update_req_body))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let updated: TenantConfigResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(updated.retention_days, 90);
    assert_eq!(updated.ingest_quota_per_sec, 500);

    // 3. GET verify persisted values
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/tenants/{tenant_id}/config"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let config2: TenantConfigResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(config2.retention_days, 90);
    assert_eq!(config2.ingest_quota_per_sec, 500);
}

#[tokio::test]
async fn test_tenant_config_forbidden_for_non_admin() {
    let (pool, _container) = start_pool().await;
    let tenant_id = Uuid::parse_str(DEV_TENANT_ID).unwrap();
    let ctx = TenantContext {
        tenant_id,
        user_id: Some(Uuid::new_v4()),
        role: "member".to_string(),
    };
    let app = build_admin_tenants_app(pool.clone(), ctx);

    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/tenants/{tenant_id}/config"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_tenant_hard_purge_workflow() {
    let (pool, _container) = start_pool().await;

    // Create a target tenant for purge
    let purge_tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name, retention_days, ingest_quota_per_sec) VALUES ($1, 'purge-target', 14, 50)")
        .bind(purge_tenant_id)
        .execute(&pool)
        .await
        .unwrap();

    // Seed child rows across various tables
    sqlx::query("INSERT INTO projects (id, tenant_id, name) VALUES ($1, $2, 'demo-project')")
        .bind(Uuid::new_v4())
        .bind(purge_tenant_id)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO api_keys (tenant_id, key_hash, name, environment, role) VALUES ($1, 'hash-purge', 'purge-key', 'prod', 'member')")
        .bind(purge_tenant_id)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO alert_rules (rule_id, tenant_id, name, alert_type, severity, condition) VALUES ($1, $2, 'purge-alert', 'threshold', 'warning', '{}')")
        .bind(Uuid::new_v4())
        .bind(purge_tenant_id)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO deployment_markers (deployment_id, tenant_id, service_name, environment, service_version, status) VALUES ($1, $2, 'srv', 'prod', 'v1.0', 'success')")
        .bind(Uuid::new_v4())
        .bind(purge_tenant_id)
        .execute(&pool)
        .await
        .unwrap();

    // Call DELETE /v1/tenants/{id} as admin of this tenant
    let user_id = Uuid::new_v4();
    let ctx = TenantContext {
        tenant_id: purge_tenant_id,
        user_id: Some(user_id),
        role: "tenant_admin".to_string(),
    };
    let app = build_admin_tenants_app(pool.clone(), ctx);

    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/tenants/{purge_tenant_id}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    if status != StatusCode::OK {
        eprintln!(
            "Purge failed with status {status}: {}",
            String::from_utf8_lossy(&body)
        );
    }
    assert_eq!(status, StatusCode::OK);
    let purge_resp: PurgeTenantResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(purge_resp.status, "purged");
    assert_eq!(purge_resp.tenant_id, purge_tenant_id);

    // Verify tenant is deleted from Postgres
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tenants WHERE id = $1")
        .bind(purge_tenant_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);

    // Verify related rows are deleted
    let key_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_keys WHERE tenant_id = $1")
        .bind(purge_tenant_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(key_count, 0);

    let rule_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM alert_rules WHERE tenant_id = $1")
            .bind(purge_tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(rule_count, 0);

    // Verify credential_audit_log contains the tenant_purge audit event
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM credential_audit_log WHERE tenant_id = $1 AND action = 'tenant_purge' AND outcome = 'allow'",
    )
    .bind(purge_tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 1);
}
