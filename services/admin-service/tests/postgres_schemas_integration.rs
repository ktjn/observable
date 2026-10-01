// HTTP integration test for the schema-registry/semantic-annotation routes against a
// real Postgres instance via Testcontainers, exercising the full handler path via
// tower::ServiceExt::oneshot.

use admin_service::{
    AdminServiceAppState, middleware::auth::TenantContext, observability, schemas,
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

const SEED_TENANT_ID: &str = "00000000-0000-0000-0000-000000000001";

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
            "/v1/schemas/{signal_type}/attributes",
            get(schemas::handle_list_attributes),
        )
        .route(
            "/v1/schemas/{signal_type}/attributes/{key}/annotations",
            get(schemas::handle_get_annotation)
                .put(schemas::handle_upsert_annotation)
                .patch(schemas::handle_patch_annotation)
                .delete(schemas::handle_delete_annotation),
        )
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id: Some(Uuid::new_v4()),
            role: "tenant_admin".into(),
        }))
        .with_state(state)
}

#[tokio::test]
async fn list_attributes_returns_seeded_metrics_entry() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::parse_str(SEED_TENANT_ID).unwrap();
    let app = build_app(db, tenant_id);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/schemas/metrics/attributes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = response_body_json(resp.into_body()).await;
    let attributes = body["attributes"].as_array().expect("attributes array");
    assert!(
        attributes
            .iter()
            .any(|a| a["field_name"] == "request_duration_ms"),
        "seeded request_duration_ms must appear"
    );
}

#[tokio::test]
async fn list_attributes_rejects_invalid_signal_type() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::parse_str(SEED_TENANT_ID).unwrap();
    let app = build_app(db, tenant_id);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/schemas/bogus/attributes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_annotation_returns_seeded_annotation_for_dev_tenant() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::parse_str(SEED_TENANT_ID).unwrap();
    let app = build_app(db, tenant_id);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/schemas/metrics/attributes/request_duration_ms/annotations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = response_body_json(resp.into_body()).await;
    assert_eq!(body["metric_type"], "gauge");
    assert_eq!(body["unit"], "ms");
}

#[tokio::test]
async fn get_annotation_not_visible_to_other_tenant() {
    let (db, _container) = start_postgres().await;
    let other_tenant = Uuid::new_v4();
    let app = build_app(db, other_tenant);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/schemas/metrics/attributes/request_duration_ms/annotations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn upsert_then_patch_then_delete_annotation_round_trip() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::new_v4();
    let app = build_app(db, tenant_id);

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v1/schemas/metrics/attributes/error_rate/annotations")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "display_name": "Error Rate",
                        "metric_type": "gauge",
                        "unit": "1"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);
    let created = response_body_json(create_resp.into_body()).await;
    assert_eq!(created["display_name"], "Error Rate");

    let patch_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/v1/schemas/metrics/attributes/error_rate/annotations")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"recommended_downsampling": "5m"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch_resp.status(), StatusCode::OK);
    let patched = response_body_json(patch_resp.into_body()).await;
    assert_eq!(patched["recommended_downsampling"], "5m");
    // Unpatched field from the original upsert survives the partial update.
    assert_eq!(patched["display_name"], "Error Rate");

    let delete_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v1/schemas/metrics/attributes/error_rate/annotations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete_resp.status(), StatusCode::NO_CONTENT);

    let get_after_delete = app
        .oneshot(
            Request::builder()
                .uri("/v1/schemas/metrics/attributes/error_rate/annotations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_after_delete.status(), StatusCode::NOT_FOUND);
}
