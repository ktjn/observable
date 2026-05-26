use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    routing::get,
};
use http_body_util::BodyExt as _;
use std::sync::Arc;
use storage_writer::{AppState, observability};
use tower::ServiceExt;

fn test_state(ch: clickhouse::Client) -> AppState {
    AppState {
        ch,
        metrics: Arc::new(observability::StorageWriterMetrics::new()),
    }
}

fn test_app(state: AppState) -> Router {
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

#[tokio::test]
#[ignore]
async fn storage_writer_readyz_returns_200_when_clickhouse_is_reachable() {
    use testcontainers::{ImageExt, runners::AsyncRunner};
    use testcontainers_modules::clickhouse::ClickHouse;

    let container = ClickHouse::default()
        .with_tag("25.3")
        .with_env_var("CLICKHOUSE_USER", "default")
        .with_env_var("CLICKHOUSE_PASSWORD", "test")
        .start()
        .await
        .expect("clickhouse container started");
    let port = container.get_host_port_ipv4(8123).await.unwrap();
    let ch = clickhouse::Client::default()
        .with_url(format!("http://127.0.0.1:{port}"))
        .with_user("default")
        .with_password("test");

    let app = test_app(test_state(ch));

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
async fn storage_writer_readyz_returns_503_when_clickhouse_is_unavailable() {
    let ch = clickhouse::Client::default().with_url("http://127.0.0.1:1");
    let app = test_app(test_state(ch));

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
async fn storage_writer_metrics_endpoint_exposes_prometheus_text() {
    let ch = clickhouse::Client::default().with_url("http://127.0.0.1:1");
    let app = test_app(test_state(ch));

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
        body.contains("storage_writer_http_requests_total"),
        "metrics payload did not contain the storage-writer request counter: {body}"
    );
}
