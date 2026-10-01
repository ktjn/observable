// HTTP integration test for GET/PUT /v1/dashboards/{id} against a real Postgres
// instance via Testcontainers, exercising the full handler path via
// tower::ServiceExt::oneshot.

use admin_service::{
    AdminServiceAppState, dashboards, middleware::auth::TenantContext, observability,
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
    };
    Router::new()
        .route(
            "/v1/dashboards/{id}",
            get(dashboards::handle_get_dashboard).put(dashboards::handle_update_dashboard),
        )
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id: None,
            role: "tenant_admin".into(),
        }))
        .with_state(state)
}

#[tokio::test]
async fn dashboard_get_http_returns_v2_panel_shape() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;

    let created = dashboards::create_dashboard(
        &db,
        tenant,
        &dashboards::CreateDashboardRequest {
            name: "HTTP dashboard".into(),
            panels: vec![dashboards::DashboardPanelRequest {
                title: "Notes".into(),
                panel_kind: Some("text".into()),
                query_kind: None,
                content: Some("HTTP text panel".into()),
                layout: Some(json!({"x":0,"y":0,"w":12,"h":2})),
                ..Default::default()
            }],
        },
        None,
    )
    .await
    .expect("dashboard created");

    let app = build_app(db, tenant);
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/dashboards/{}", created.dashboard_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(body["name"], "HTTP dashboard");
    assert_eq!(body["panels"][0]["panel_kind"], "text");
    assert_eq!(body["panels"][0]["content"], "HTTP text panel");
    assert_eq!(body["panels"][0]["layout"]["w"], 12);
}

#[tokio::test]
async fn dashboard_put_http_updates_panel_layout() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;

    let created = dashboards::create_dashboard(
        &db,
        tenant,
        &dashboards::CreateDashboardRequest {
            name: "HTTP dashboard".into(),
            panels: vec![dashboards::DashboardPanelRequest {
                title: "Notes".into(),
                panel_kind: Some("text".into()),
                query_kind: None,
                content: Some("Before".into()),
                layout: Some(json!({"x":0,"y":0,"w":6,"h":2})),
                ..Default::default()
            }],
        },
        None,
    )
    .await
    .expect("dashboard created");

    let app = build_app(db, tenant);
    let body = json!({
        "name": "Updated dashboard",
        "panels": [{
            "title": "Notes",
            "panel_kind": "text",
            "query_kind": null,
            "preset": null,
            "filters": {},
            "content": "After",
            "layout": {"x":0,"y":0,"w":8,"h":3},
            "time_range": {"mode":"global"}
        }]
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/v1/dashboards/{}", created.dashboard_id))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(body["name"], "Updated dashboard");
    assert_eq!(body["panels"][0]["content"], "After");
    assert_eq!(body["panels"][0]["layout"]["w"], 8);
}
