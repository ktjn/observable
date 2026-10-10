// HTTP integration tests for admin-service's /internal/* service-to-service
// routes (Phase 6's internal-endpoint split by owner,
// docs/component-decomposition.md) against a real Postgres instance via
// Testcontainers, exercising the full handler path via
// tower::ServiceExt::oneshot. These cover the deployment_markers (control)
// half; the alerting half now lives in
// services/alert-evaluator/tests/internal_integration.rs.

use admin_service::{
    AdminServiceAppState, internal, middleware::auth::InternalServiceToken, observability,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    middleware as axum_middleware,
    routing::get,
};
use chrono::{Duration, Utc};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::{path::Path, sync::Arc};
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use uuid::Uuid;

const INTERNAL_TOKEN: &str = "test-internal-token";

async fn start_postgres() -> (PgPool, testcontainers::ContainerAsync<Postgres>) {
    let container = Postgres::default()
        .with_tag("17")
        .start()
        .await
        .expect("postgres container started");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = observable_config::with_search_path(&format!(
        "postgres://postgres:postgres@127.0.0.1:{port}/postgres"
    ));
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("postgres pool connected");
    apply_pg_migrations(&pool).await;
    (pool, container)
}

async fn apply_pg_migrations(pool: &PgPool) {
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
            .expect("pg migration applied");
    }
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

fn build_app(db: PgPool) -> Router {
    let state = AdminServiceAppState {
        db: db.clone(),
        ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        auth_service_url: "http://auth-service:4319".into(),
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AdminServiceMetrics::new()),
        producer: None,
    };
    Router::new()
        .route(
            "/internal/deployment-enrichment",
            get(internal::handle_deployment_enrichment),
        )
        .route(
            "/internal/deployment-correlation",
            get(internal::handle_deployment_correlation),
        )
        .layer(axum_middleware::from_fn(
            admin_service::middleware::auth::require_internal_service,
        ))
        .layer(axum::Extension(InternalServiceToken(
            INTERNAL_TOKEN.to_string(),
        )))
        .with_state(state)
}

fn authed_request(uri: String) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("x-internal-token", INTERNAL_TOKEN)
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn internal_routes_reject_missing_token() {
    let (db, _container) = start_postgres().await;
    let app = build_app(db);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/internal/deployment-enrichment?tenant_id=00000000-0000-0000-0000-000000000001")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn deployment_enrichment_reports_latest_deploy() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::new_v4();
    insert_tenant(&db, tenant_id).await;

    sqlx::query(
        "INSERT INTO deployment_markers \
         (tenant_id, service_name, environment, service_version, status, started_at) \
         VALUES ($1, 'checkout', 'prod', 'v2.3.1', 'success', NOW())",
    )
    .bind(tenant_id)
    .execute(&db)
    .await
    .expect("deployment marker inserted");

    let app = build_app(db);
    let resp = app
        .oneshot(authed_request(format!(
            "/internal/deployment-enrichment?tenant_id={tenant_id}"
        )))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = response_body_json(resp.into_body()).await;
    let items = body["items"].as_array().expect("items array");
    let checkout = items
        .iter()
        .find(|i| i["service_name"] == "checkout")
        .expect("checkout item present");
    assert_eq!(checkout["latest_deployment"], "v2.3.1");
}

#[tokio::test]
async fn deployment_enrichment_filters_by_environment() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::new_v4();
    insert_tenant(&db, tenant_id).await;

    sqlx::query(
        "INSERT INTO deployment_markers \
         (tenant_id, service_name, environment, service_version, status, started_at) \
         VALUES ($1, 'checkout', 'staging', 'v9.9.9', 'success', NOW())",
    )
    .bind(tenant_id)
    .execute(&db)
    .await
    .expect("staging deployment inserted");

    let app = build_app(db);
    let resp = app
        .oneshot(authed_request(format!(
            "/internal/deployment-enrichment?tenant_id={tenant_id}&environment=prod"
        )))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_body_json(resp.into_body()).await;
    let items = body["items"].as_array().expect("items array");
    assert!(
        items.is_empty(),
        "staging-only deployment must not appear in a prod-filtered query"
    );
}

#[tokio::test]
async fn deployment_correlation_filters_service_environment_and_interval() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::new_v4();
    insert_tenant(&db, tenant_id).await;
    let service_name = "checkout";
    let from = Utc::now() - Duration::hours(6);
    let to = Utc::now();

    sqlx::query(
        "INSERT INTO deployment_markers \
         (deployment_id, tenant_id, project_id, service_name, environment, service_version, status, started_at, finished_at, deployed_by, commit_sha, rollback_of, metadata) \
         VALUES ($1, $2, NULL, $3, 'prod', '2026.05.22', 'success', $4, $5, 'ci-bot', 'abc123', NULL, NULL)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(service_name)
    .bind(from + Duration::hours(4))
    .bind(from + Duration::hours(5))
    .execute(&db)
    .await
    .expect("prod deployment inserted");

    sqlx::query(
        "INSERT INTO deployment_markers \
         (deployment_id, tenant_id, project_id, service_name, environment, service_version, status, started_at, finished_at, deployed_by, commit_sha, rollback_of, metadata) \
         VALUES ($1, $2, NULL, $3, 'staging', '2026.05.21', 'success', $4, $5, 'ci-bot', 'def456', NULL, NULL)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(service_name)
    .bind(from + Duration::hours(4))
    .bind(from + Duration::hours(5))
    .execute(&db)
    .await
    .expect("staging deployment inserted");

    let app = build_app(db);
    let uri = format!(
        "/internal/deployment-correlation?tenant_id={tenant_id}&service_name={service_name}&environment=prod&from={}&to={}",
        from.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        to.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    );
    let resp = app.oneshot(authed_request(uri)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = response_body_json(resp.into_body()).await;
    let deployments = body["deployments"].as_array().expect("deployments array");
    assert_eq!(deployments.len(), 1, "only the prod checkout deployment");
    assert_eq!(deployments[0]["service_version"], "2026.05.22");
}
