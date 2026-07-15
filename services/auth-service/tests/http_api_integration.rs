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
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            observability::record_http_metrics,
        ))
        .with_state(state)
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
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.expect("pool connected");
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
