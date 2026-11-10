// HTTP integration tests for tenant retention configuration and hard-purge workflows:
//   GET    /v1/tenants/{id}/config
//   PUT    /v1/tenants/{id}/config
//   DELETE /v1/tenants/{id}
//
// Tests use real Postgres (the purge runs as the scoped observable_control login), real
// ClickHouse and a stand-in auth-service, and exercise the handler via tower::ServiceExt::oneshot.

use admin_service::{
    AdminServiceAppState,
    auth_directory::AuthDirectory,
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
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, header, method, path},
};

const INTERNAL_TOKEN: &str = "test-internal-token";

// ── Dev credentials (matching migrations seed) ──────────────────────────────
const DEV_TENANT_ID: &str = "00000000-0000-0000-0000-000000000002";

// ── Container helpers ────────────────────────────────────────────────────────

async fn start_pool() -> (PgPool, test_support::postgres::TestDatabase) {
    test_support::postgres::isolated_pool().await
}

// ── App builder ──────────────────────────────────────────────────────────────

fn build_admin_tenants_app(pool: PgPool, ctx: TenantContext) -> Router {
    build_app(
        pool,
        clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        "http://127.0.0.1:1".into(),
        ctx,
    )
}

fn build_app(pool: PgPool, ch: clickhouse::Client, auth_url: String, ctx: TenantContext) -> Router {
    let directory = AuthDirectory::new(&auth_url, INTERNAL_TOKEN, reqwest::Client::new());
    let state = AdminServiceAppState {
        db: pool,
        ch,
        auth_service_url: auth_url,
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
        .layer(Extension(directory))
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

async fn seed_tenant(owner: &PgPool, tenant: Uuid) {
    for sql in [
        "INSERT INTO control.tenants (id, name, retention_days, ingest_quota_per_sec) VALUES ($1, 'purge-target', 14, 50)",
        "INSERT INTO control.projects (tenant_id, name) VALUES ($1, 'demo-project')",
        "INSERT INTO control.deployment_markers (tenant_id, service_name, environment, service_version, status) VALUES ($1, 'srv', 'prod', 'v1.0', 'success')",
        "INSERT INTO control.dashboards (tenant_id, name) VALUES ($1, 'd')",
    ] {
        sqlx::query(sql).bind(tenant).execute(owner).await.unwrap();
    }
}

async fn control_rows(owner: &PgPool, tenant: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM control.tenants WHERE id = $1) \
         + (SELECT count(*) FROM control.projects WHERE tenant_id = $1) \
         + (SELECT count(*) FROM control.deployment_markers WHERE tenant_id = $1) \
         + (SELECT count(*) FROM control.dashboards WHERE tenant_id = $1)",
    )
    .bind(tenant)
    .fetch_one(owner)
    .await
    .unwrap()
}

async fn delete_tenant(app: Router, tenant: Uuid) -> (StatusCode, serde_json::Value) {
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/tenants/{tenant}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap_or_default())
}

fn admin_of(tenant: Uuid, user: Uuid) -> TenantContext {
    TenantContext {
        tenant_id: tenant,
        user_id: Some(user),
        role: "tenant_admin".to_string(),
    }
}

#[tokio::test]
async fn test_tenant_hard_purge_workflow() {
    let (owner, _db) = start_pool().await;
    let scoped = test_support::postgres::scoped_pool(&owner, "observable_control").await;
    let ch = test_support::clickhouse::shared_client().await;
    let (tenant, other, user) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    seed_tenant(&owner, tenant).await;
    seed_tenant(&owner, other).await;

    let auth = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/internal/tenants/{tenant}/purge")))
        .and(header("x-internal-token", INTERNAL_TOKEN))
        .and(body_json(serde_json::json!({ "actor": user.to_string() })))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({ "tenant_id": tenant, "purged_tables": ["api_keys"] }),
        ))
        .expect(1)
        .mount(&auth)
        .await;

    let app = build_app(scoped, ch, auth.uri(), admin_of(tenant, user));
    let (status, body) = delete_tenant(app, tenant).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let purge: PurgeTenantResponse = serde_json::from_value(body).unwrap();
    assert_eq!(purge.status, "purged");
    assert_eq!(purge.tenant_id, tenant);
    assert!(
        purge
            .purged_postgres_tables
            .contains(&"control.tenants".to_string())
    );
    assert!(
        purge
            .purged_postgres_tables
            .contains(&"auth.api_keys".to_string())
    );
    assert_eq!(purge.purged_clickhouse_tables.len(), 5);
    assert_eq!(purge.pending_components, ["alerting", "catalog", "health"]);

    assert_eq!(control_rows(&owner, tenant).await, 0);
    assert_eq!(
        control_rows(&owner, other).await,
        4,
        "other tenants are untouched"
    );
}

#[tokio::test]
async fn test_tenant_purge_retries_after_auth_is_unavailable() {
    let (owner, _db) = start_pool().await;
    let scoped = test_support::postgres::scoped_pool(&owner, "observable_control").await;
    let ch = test_support::clickhouse::shared_client().await;
    let (tenant, user) = (Uuid::new_v4(), Uuid::new_v4());
    seed_tenant(&owner, tenant).await;

    let auth = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&auth)
        .await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "tenant_id": tenant, "purged_tables": [] })),
        )
        .mount(&auth)
        .await;

    let app = build_app(scoped, ch, auth.uri(), admin_of(tenant, user));
    let (status, _) = delete_tenant(app.clone(), tenant).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    // Auth goes last: the caller's session survives, and the retry completes the purge.
    let (status, body) = delete_tenant(app, tenant).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(control_rows(&owner, tenant).await, 0);
}

#[tokio::test]
async fn test_tenant_purge_is_refused_before_any_deletion() {
    let (owner, _db) = start_pool().await;
    let (tenant, other) = (Uuid::new_v4(), Uuid::new_v4());
    seed_tenant(&owner, tenant).await;
    let auth = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&auth)
        .await;
    let ch = test_support::clickhouse::shared_client().await;

    let member = TenantContext {
        role: "member".to_string(),
        ..admin_of(tenant, Uuid::new_v4())
    };
    let app = build_app(owner.clone(), ch.clone(), auth.uri(), member);
    assert_eq!(delete_tenant(app, tenant).await.0, StatusCode::FORBIDDEN);
    let app = build_app(
        owner.clone(),
        ch.clone(),
        auth.uri(),
        admin_of(other, Uuid::new_v4()),
    );
    assert_eq!(delete_tenant(app, tenant).await.0, StatusCode::FORBIDDEN);

    // ClickHouse down: nothing in Postgres or auth is touched.
    let down = clickhouse::Client::default().with_url("http://127.0.0.1:19999");
    let app = build_app(
        owner.clone(),
        down,
        auth.uri(),
        admin_of(tenant, Uuid::new_v4()),
    );
    assert_eq!(
        delete_tenant(app, tenant).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(control_rows(&owner, tenant).await, 4);
}
