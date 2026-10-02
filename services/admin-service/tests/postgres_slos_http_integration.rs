// HTTP integration test for GET/POST /v1/slos against a real Postgres
// instance via Testcontainers, exercising the full handler path via
// tower::ServiceExt::oneshot.

use admin_service::{AdminServiceAppState, middleware::auth::TenantContext, observability, slos};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::{path::Path, sync::Arc};
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use uuid::Uuid;

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

fn build_app(db: PgPool, tenant_id: Uuid) -> Router {
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
            "/v1/slos",
            get(slos::handle_list_slos).post(slos::handle_create_slo),
        )
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id: None,
            role: "tenant_admin".into(),
        }))
        .with_state(state)
}

#[tokio::test]
async fn post_slo_creates_tenant_scoped_definition() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;
    let app = build_app(db, tenant);

    let body = json!({
        "service_name": "payments",
        "environment": "prod",
        "target": 0.999,
        "window_days": 30,
        "burn_rate_fast_threshold": 14.4,
        "burn_rate_slow_threshold": 1.0,
        "description": "Payments availability SLO"
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/slos")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(body["service_name"], "payments");
    assert_eq!(body["environment"], "prod");
    assert_eq!(body["sli_type"], "availability");
    assert_eq!(body["target"], 0.999);
    assert_eq!(body["firing"], false);
    assert!(body["last_fired_at"].is_null());
}

#[tokio::test]
async fn post_slo_rejects_invalid_target() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;
    let app = build_app(db, tenant);

    let body = json!({
        "service_name": "payments",
        "environment": "prod",
        "target": 1.0,
        "window_days": 30,
        "burn_rate_fast_threshold": 14.4,
        "burn_rate_slow_threshold": 1.0
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/slos")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_slos_does_not_return_other_tenant_definitions() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    let other_tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;
    insert_tenant(&db, other_tenant).await;

    sqlx::query(
        "INSERT INTO slo_definitions \
         (tenant_id, service_name, environment, sli_type, target, window_days, \
          burn_rate_fast_threshold, burn_rate_slow_threshold, description) \
         VALUES ($1, 'private-svc', 'prod', 'availability', 0.99, 30, 14.4, 1.0, 'Private SLO')",
    )
    .bind(other_tenant)
    .execute(&db)
    .await
    .expect("other tenant SLO inserted");

    let app = build_app(db, tenant);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/slos")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    let items = body["items"].as_array().expect("items array");
    assert!(
        items
            .iter()
            .all(|item| item["service_name"] != "private-svc"),
        "tenant-scoped list must not include other tenant SLOs"
    );
}
