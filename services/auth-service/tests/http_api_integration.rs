use auth_service::{
    observability,
    oidc::{OidcConfig, OidcState},
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    routing::get,
};
use http_body_util::BodyExt as _;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::path::Path;
use std::sync::Arc;
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;

fn test_state(db: PgPool) -> OidcState {
    OidcState {
        db,
        config: OidcConfig {
            issuer: "http://localhost:8082".into(),
            api_base: "http://localhost:8082".into(),
            client_id: "dev-client-id".into(),
            redirect_uri: "http://localhost:5173/auth/callback".into(),
            session_secret: "dev-session-secret-change-in-prod!!".into(),
            dev_mode: true,
        },
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AuthServiceMetrics::new()),
    }
}

fn test_app(state: OidcState) -> Router {
    Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/readyz", get(observability::readyz))
        .route("/metrics", get(observability::metrics))
        .route(
            "/internal/validate",
            axum::routing::post(auth_service::validate_handler),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            observability::record_http_metrics,
        ))
        .with_state(state)
}

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

async fn start_pool() -> (
    PgPool,
    testcontainers::ContainerAsync<testcontainers_modules::postgres::Postgres>,
) {
    let container = Postgres::default()
        .with_tag("17")
        .start()
        .await
        .expect("postgres container started");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = observable_config::with_search_path(&format!(
        "postgres://postgres:postgres@127.0.0.1:{port}/postgres"
    ));
    let pool = PgPool::connect(&url).await.expect("pool connected");
    apply_migrations(&pool).await;
    (pool, container)
}

#[tokio::test]
async fn auth_service_readyz_returns_200_when_postgres_is_reachable() {
    let (pool, _container) = start_pool().await;
    let app = test_app(test_state(pool));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/readyz")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("router responded");

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn auth_service_readyz_returns_503_when_postgres_is_unavailable() {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgres://user:pass@127.0.0.1:1/db")
        .expect("lazy postgres pool");
    let app = test_app(test_state(pool));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/readyz")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("router responded");

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn auth_service_metrics_endpoint_exposes_prometheus_text() {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgres://user:pass@127.0.0.1:1/db")
        .expect("lazy postgres pool");
    let app = test_app(test_state(pool));

    let health_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/health")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("router responded");
    assert_eq!(health_response.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/metrics")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("router responded");

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .expect("content type header present")
        .to_str()
        .expect("content type is utf-8");
    assert_eq!(
        content_type, "text/plain; version=0.0.4; charset=utf-8",
        "prometheus content type must match the text encoder"
    );

    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("metrics body collected")
        .to_bytes();
    let body = String::from_utf8(bytes.to_vec()).expect("metrics body is utf-8");
    assert!(
        body.contains("auth_service_http_requests_total"),
        "metrics payload did not contain the auth-service request counter: {body}"
    );
}

#[tokio::test]
async fn validate_api_key_writes_allow_audit_log_row() {
    let (pool, _container) = start_pool().await;
    let state = test_state(pool.clone());
    let app = test_app(state);

    let dev_key = "dev-api-key-0000";
    let req = Request::builder()
        .method("POST")
        .uri("/internal/validate")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({"api_key": dev_key}).to_string(),
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let hash = auth_service::validate::sha256_hex(dev_key);
    let row: (String, String, String, Option<uuid::Uuid>) = sqlx::query_as(
        "SELECT action, outcome, credential_hash, tenant_id FROM credential_audit_log WHERE credential_hash = $1 ORDER BY occurred_at DESC LIMIT 1",
    )
    .bind(&hash)
    .fetch_one(&pool)
    .await
    .expect("audit row written");

    assert_eq!(row.0, "credential_validate");
    assert_eq!(row.1, "allow");
    assert_eq!(row.2, hash);
    assert_eq!(
        row.3,
        Some(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap())
    );
}

#[tokio::test]
async fn validate_api_key_failure_writes_deny_audit_log_row() {
    let (pool, _container) = start_pool().await;
    let state = test_state(pool.clone());
    let app = test_app(state);

    let req = Request::builder()
        .method("POST")
        .uri("/internal/validate")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({"api_key": "not-a-real-key"}).to_string(),
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let hash = auth_service::validate::sha256_hex("not-a-real-key");
    let row: (String, String, Option<String>) = sqlx::query_as(
        "SELECT action, outcome, denial_reason FROM credential_audit_log WHERE credential_hash = $1 ORDER BY occurred_at DESC LIMIT 1",
    )
    .bind(&hash)
    .fetch_one(&pool)
    .await
    .expect("deny audit row written");

    assert_eq!(row.0, "credential_validate");
    assert_eq!(row.1, "deny");
    assert_eq!(row.2, Some("not_found".to_string()));
}
