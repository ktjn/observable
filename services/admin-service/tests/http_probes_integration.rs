use std::sync::Arc;
use std::time::Duration;

use admin_service::{AdminServiceAppState, observability};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    routing::get,
};
use http_body_util::BodyExt as _;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

fn unreachable_postgres_state() -> AdminServiceAppState {
    let db = PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(500))
        .connect_lazy("postgres://observable:observable@127.0.0.1:1/observable")
        .expect("lazy postgres pool");
    AdminServiceAppState {
        db,
        ch: clickhouse::Client::default(),
        auth_service_url: String::new(),
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AdminServiceMetrics::new()),
        producer: None,
    }
}

fn probe_app(state: AdminServiceAppState) -> Router {
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

async fn get_path(app: &Router, uri: &str) -> axum::response::Response {
    app.clone()
        .oneshot(Request::get(uri).body(Body::empty()).expect("request"))
        .await
        .expect("router responded")
}

#[tokio::test]
async fn admin_service_health_readyz_and_metrics_endpoints() {
    let app = probe_app(unreachable_postgres_state());

    assert_eq!(get_path(&app, "/health").await.status(), StatusCode::OK);
    assert_eq!(
        get_path(&app, "/readyz").await.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "readiness must fail while postgres is unreachable"
    );

    let response = get_path(&app, "/metrics").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/plain; version=0.0.4; charset=utf-8"
    );
    let body = response
        .into_body()
        .collect()
        .await
        .expect("metrics body")
        .to_bytes();
    let body = String::from_utf8(body.to_vec()).expect("utf-8 metrics body");
    for family in [
        "admin_service_http_requests_total",
        "admin_service_http_request_duration_seconds",
        "admin_service_http_in_flight_requests",
    ] {
        assert!(body.contains(family), "missing {family} in:\n{body}");
    }
    assert!(
        body.contains(r#"admin_service_http_requests_total{method="GET",status="503"} 1"#),
        "error responses must be counted by status:\n{body}"
    );
}
