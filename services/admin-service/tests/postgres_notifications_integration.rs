// HTTP integration test for /v1/notifications/channels against a real
// Postgres instance via Testcontainers, exercising the full handler path via
// tower::ServiceExt::oneshot.

use admin_service::{
    AdminServiceAppState, middleware::auth::TenantContext, notifications, observability,
};
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
            "/v1/notifications/channels",
            get(notifications::handle_list_channels).post(notifications::handle_create_channel),
        )
        .route(
            "/v1/notifications/channels/{id}",
            axum::routing::delete(notifications::handle_delete_channel),
        )
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id: None,
            role: "tenant_admin".into(),
        }))
        .with_state(state)
}

#[tokio::test]
async fn create_then_list_then_delete_channel_round_trips() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;
    let app = build_app(db, tenant);

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/notifications/channels")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "Ops webhook",
                        "channel_type": "webhook",
                        "config": {"url": "https://example.com/hook"}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let created = response_body_json(create_resp.into_body()).await;
    assert_eq!(created["name"], "Ops webhook");
    assert_eq!(created["channel_type"], "webhook");
    let channel_id = created["channel_id"].as_str().unwrap().to_string();

    let list_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/notifications/channels")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let items = response_body_json(list_resp.into_body()).await;
    let items = items.as_array().expect("array response");
    assert!(items.iter().any(|i| i["channel_id"] == channel_id));

    let delete_resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/notifications/channels/{channel_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete_resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn delete_nonexistent_channel_returns_404() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;
    let app = build_app(db, tenant);

    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/notifications/channels/{}", Uuid::new_v4()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_channels_does_not_return_other_tenant_channels() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    let other_tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;
    insert_tenant(&db, other_tenant).await;

    let other_app = build_app(db.clone(), other_tenant);
    let create_resp = other_app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/notifications/channels")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "Other tenant channel",
                        "channel_type": "webhook",
                        "config": {"url": "https://example.com/other"}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::CREATED);

    let app = build_app(db, tenant);
    let list_resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/notifications/channels")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let items = response_body_json(list_resp.into_body()).await;
    let items = items.as_array().expect("array response");
    assert!(
        items.is_empty(),
        "tenant must not see other tenant's notification channels"
    );
}
