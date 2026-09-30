// Session-scoped behavior of the bootstrap tenant endpoints: GET /v1/tenants
// filters to the caller's tenants when a session cookie/bearer token is
// present, and GET /v1/tenants/{id}/environments rejects a session that
// lacks access to the requested tenant. Both endpoints must also stay usable
// without any session at all (bootstrap resources, outside require_tenant).

use admin_service::{AdminServiceAppState, observability, tenants};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    routing::get,
};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::PgPool;
use std::path::Path;
use std::sync::Arc;
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

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

fn build_app(db: PgPool, auth_service_url: String) -> Router {
    let state = AdminServiceAppState {
        db,
        ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        auth_service_url,
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AdminServiceMetrics::new()),
    };
    Router::new()
        .route("/v1/tenants", get(tenants::list_tenants))
        .route(
            "/v1/tenants/{id}/environments",
            get(tenants::list_tenant_environments),
        )
        .with_state(state)
}

async fn response_body_json(body: Body) -> Value {
    let bytes = body.collect().await.expect("body collected").to_bytes();
    serde_json::from_slice(&bytes).expect("valid JSON")
}

#[tokio::test]
async fn list_tenants_filtered_by_session() {
    let (db, _container) = start_pool().await;
    let mock_server = MockServer::start().await;

    let user_id = Uuid::new_v4();
    let tenant1 = Uuid::new_v4();
    let tenant2 = Uuid::new_v4();
    let tenant3 = Uuid::new_v4();

    // Setup DB: user exists and belongs to tenant1 and tenant2, but not tenant3.
    sqlx::query("INSERT INTO users (id, idp_subject, email) VALUES ($1, 'sub1', 'u1@example.com')")
        .bind(user_id)
        .execute(&db)
        .await
        .unwrap();

    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'Tenant 1'), ($2, 'Tenant 2'), ($3, 'Tenant 3')")
        .bind(tenant1).bind(tenant2).bind(tenant3)
        .execute(&db).await.unwrap();

    sqlx::query("INSERT INTO user_tenant_roles (user_id, tenant_id, role) VALUES ($1, $2, 'tenant_admin'), ($1, $3, 'viewer')")
        .bind(user_id).bind(tenant1).bind(tenant2)
        .execute(&db).await.unwrap();

    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "user_id": user_id.to_string(),
            "tenant_id": tenant1.to_string(),
            "role": "admin",
            "environment": "prod"
        })))
        .mount(&mock_server)
        .await;

    let app = build_app(db, mock_server.uri());

    // Request list tenants with session
    let req = Request::builder()
        .uri("/v1/tenants")
        .header(header::AUTHORIZATION, "Bearer some-session")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = response_body_json(resp.into_body()).await;
    let tenants = json["tenants"].as_array().unwrap();
    assert_eq!(tenants.len(), 2);
    let ids: Vec<_> = tenants.iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&tenant1.to_string().as_str()));
    assert!(ids.contains(&tenant2.to_string().as_str()));
    assert!(!ids.contains(&tenant3.to_string().as_str()));
}

#[tokio::test]
async fn list_environments_filtered_by_session() {
    let (db, _container) = start_pool().await;
    let mock_server = MockServer::start().await;

    let user_id = Uuid::new_v4();
    let tenant1 = Uuid::new_v4();
    let tenant2 = Uuid::new_v4();

    sqlx::query("INSERT INTO users (id, idp_subject, email) VALUES ($1, 'sub2', 'u2@example.com')")
        .bind(user_id)
        .execute(&db)
        .await
        .unwrap();

    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'Tenant 1'), ($2, 'Tenant 2')")
        .bind(tenant1)
        .bind(tenant2)
        .execute(&db)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO user_tenant_roles (user_id, tenant_id, role) VALUES ($1, $2, 'tenant_admin')",
    )
    .bind(user_id)
    .bind(tenant1)
    .execute(&db)
    .await
    .unwrap();

    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "user_id": user_id.to_string(),
            "tenant_id": tenant1.to_string(),
            "role": "admin",
            "environment": "prod"
        })))
        .mount(&mock_server)
        .await;

    let app = build_app(db, mock_server.uri());

    // 1. Access authorized tenant environments
    let req = Request::builder()
        .uri(format!("/v1/tenants/{}/environments", tenant1))
        .header(header::COOKIE, "session=valid")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 2. Access unauthorized tenant environments
    let req = Request::builder()
        .uri(format!("/v1/tenants/{}/environments", tenant2))
        .header(header::COOKIE, "session=valid")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn bootstrap_endpoints_public_without_session() {
    let (db, _container) = start_pool().await;

    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'Tenant 1')")
        .bind(Uuid::new_v4())
        .execute(&db)
        .await
        .unwrap();

    let app = build_app(db.clone(), "http://unreachable".to_string());

    // list_tenants is public
    let req = Request::builder()
        .uri("/v1/tenants")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // list_environments is public
    let tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'Tenant 2')")
        .bind(tenant_id)
        .execute(&db)
        .await
        .unwrap();

    let req = Request::builder()
        .uri(format!("/v1/tenants/{}/environments", tenant_id))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn list_tenants_public_without_session() {
    let (db, _container) = start_pool().await;
    let t1 = Uuid::new_v4();
    let t2 = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'T1'), ($2, 'T2')")
        .bind(t1)
        .bind(t2)
        .execute(&db)
        .await
        .unwrap();

    let app = build_app(db.clone(), "http://unreachable".to_string());

    let req = Request::builder()
        .uri("/v1/tenants")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = response_body_json(resp.into_body()).await;
    let tenants = json["tenants"].as_array().unwrap();
    assert!(tenants.len() >= 2);
}
