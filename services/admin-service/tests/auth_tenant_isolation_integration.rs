use std::{path::Path, sync::Arc};

use admin_service::middleware::auth::{TenantContext, require_tenant};
use axum::{
    Extension, Router,
    body::Body,
    http::{Request, StatusCode},
    middleware,
    routing::get,
};
use sqlx::postgres::{PgPool, PgPoolOptions};
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

async fn start_postgres() -> (PgPool, testcontainers::ContainerAsync<Postgres>) {
    let container = Postgres::default()
        .with_tag("17")
        .start()
        .await
        .expect("postgres container started");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
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
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "sql"))
        .collect();
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let sql = std::fs::read_to_string(entry.path()).expect("readable migration");
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
            .execute(pool)
            .await
            .expect("postgres migration applied");
    }
}

fn app(db: PgPool, auth_service_url: String) -> Router {
    Router::new()
        .route(
            "/",
            get(|Extension(ctx): Extension<TenantContext>| async move {
                ctx.tenant_id.to_string()
            }),
        )
        .layer(middleware::from_fn(require_tenant))
        .layer(Extension(db))
        .layer(Extension(Arc::new(auth_service_url)))
}

async fn mock_session(
    mock_server: &MockServer,
    user_id: Uuid,
    session_tenant_id: Uuid,
) {
    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "user_id": user_id.to_string(),
            "tenant_id": session_tenant_id.to_string(),
            "role": "tenant_admin",
            "environment": "production"
        })))
        .mount(mock_server)
        .await;
}

#[tokio::test]
async fn session_cannot_switch_to_unrelated_tenant() {
    let (db, _container) = start_postgres().await;
    let mock_server = MockServer::start().await;
    let user_id = Uuid::new_v4();
    let session_tenant_id = Uuid::new_v4();
    let requested_tenant_id = Uuid::new_v4();
    mock_session(&mock_server, user_id, session_tenant_id).await;

    let response = app(db, mock_server.uri())
        .oneshot(
            Request::builder()
                .uri("/")
                .header("cookie", "session=valid-session")
                .header("x-tenant-id", requested_tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn session_can_switch_to_tenant_with_membership() {
    let (db, _container) = start_postgres().await;
    let mock_server = MockServer::start().await;
    let user_id = Uuid::new_v4();
    let session_tenant_id = Uuid::new_v4();
    let requested_tenant_id = Uuid::new_v4();
    mock_session(&mock_server, user_id, session_tenant_id).await;

    sqlx::query("INSERT INTO users (id, idp_subject, email) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(format!("idp|{user_id}"))
        .bind(format!("{user_id}@example.com"))
        .execute(&db)
        .await
        .expect("user inserted");
    sqlx::query(
        "INSERT INTO user_tenant_roles (user_id, tenant_id, role) VALUES ($1, $2, $3)",
    )
    .bind(user_id)
    .bind(requested_tenant_id)
    .bind("member")
    .execute(&db)
    .await
    .expect("membership inserted");

    let response = app(db, mock_server.uri())
        .oneshot(
            Request::builder()
                .uri("/")
                .header("cookie", "session=valid-session")
                .header("x-tenant-id", requested_tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
