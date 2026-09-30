// HTTP integration test for GET /v1/deployments against a real Postgres
// instance via Testcontainers, exercising the full handler path via
// tower::ServiceExt::oneshot.

use admin_service::{
    AdminServiceAppState, deployments, middleware::auth::TenantContext, observability,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::{path::Path, sync::Arc};
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use uuid::Uuid;

const DEV_TENANT_ID: &str = "00000000-0000-0000-0000-000000000002";

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

fn build_app(db: PgPool, tenant_id: Uuid) -> Router {
    let state = AdminServiceAppState {
        db: db.clone(),
        ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        auth_service_url: "http://auth-service:4319".into(),
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AdminServiceMetrics::new()),
    };
    Router::new()
        .route("/v1/deployments", get(deployments::list_deployments))
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id: Some(Uuid::new_v4()),
            role: "tenant_admin".into(),
        }))
        .with_state(state)
}

async fn seed_deployment(
    db: &PgPool,
    tenant_id: Uuid,
    service_name: &str,
    environment: &str,
    status: &str,
) {
    sqlx::query(
        r#"
        INSERT INTO deployment_markers
            (deployment_id, tenant_id, service_name, environment, service_version, status, started_at)
        VALUES ($1, $2, $3, $4, '1.0.0', $5, now())
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(service_name)
    .bind(environment)
    .bind(status)
    .execute(db)
    .await
    .expect("deployment marker inserted");
}

#[tokio::test]
async fn list_deployments_returns_seeded_markers_scoped_to_tenant() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::parse_str(DEV_TENANT_ID).unwrap();
    let other_tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'other')")
        .bind(other_tenant_id)
        .execute(&db)
        .await
        .unwrap();

    seed_deployment(&db, tenant_id, "shop-api", "production", "success").await;
    seed_deployment(&db, other_tenant_id, "other-svc", "production", "success").await;

    let app = build_app(db, tenant_id);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/deployments")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = response_body_json(resp.into_body()).await;
    let items = body["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["service_name"], "shop-api");
}

#[tokio::test]
async fn list_deployments_filters_by_service_name() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    seed_deployment(&db, tenant_id, "shop-api", "production", "success").await;
    seed_deployment(&db, tenant_id, "billing-api", "production", "success").await;

    let app = build_app(db, tenant_id);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/deployments?service_name=billing-api")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = response_body_json(resp.into_body()).await;
    let items = body["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["service_name"], "billing-api");
}
