// HTTP integration test for GET /v1/events/changes against a real Postgres
// instance via Testcontainers, exercising the full handler path via
// tower::ServiceExt::oneshot.

use admin_service::{
    AdminServiceAppState, change_events, middleware::auth::TenantContext, observability,
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
        .route(
            "/v1/events/changes",
            get(change_events::handle_list_change_events),
        )
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id: Some(Uuid::new_v4()),
            role: "tenant_admin".into(),
        }))
        .with_state(state)
}

async fn seed_event(
    db: &PgPool,
    tenant_id: Uuid,
    event_type: &str,
    service_name: Option<&str>,
    environment: &str,
    title: &str,
) {
    sqlx::query(
        "INSERT INTO change_events (tenant_id, event_type, service_name, environment, title) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(tenant_id)
    .bind(event_type)
    .bind(service_name)
    .bind(environment)
    .bind(title)
    .execute(db)
    .await
    .expect("change event inserted");
}

#[tokio::test]
async fn list_change_events_returns_seeded_events_scoped_to_tenant() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::parse_str(DEV_TENANT_ID).unwrap();
    let other_tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'other')")
        .bind(other_tenant_id)
        .execute(&db)
        .await
        .unwrap();

    seed_event(
        &db,
        tenant_id,
        "feature_flag",
        Some("checkout"),
        "production",
        "Enabled new flow",
    )
    .await;
    seed_event(
        &db,
        other_tenant_id,
        "feature_flag",
        Some("other-svc"),
        "production",
        "Other tenant event",
    )
    .await;

    let app = build_app(db, tenant_id);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/changes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = response_body_json(resp.into_body()).await;
    let items = body["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Enabled new flow");
}

#[tokio::test]
async fn list_change_events_filters_by_service_name() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    seed_event(
        &db,
        tenant_id,
        "migration",
        Some("checkout"),
        "production",
        "Checkout schema migration",
    )
    .await;
    seed_event(
        &db,
        tenant_id,
        "migration",
        Some("billing"),
        "production",
        "Billing schema migration",
    )
    .await;

    let app = build_app(db, tenant_id);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/changes?service_name=billing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = response_body_json(resp.into_body()).await;
    let items = body["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Billing schema migration");
}
