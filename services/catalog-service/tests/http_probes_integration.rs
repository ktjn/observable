use std::time::Duration;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt as _;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

mod common;

fn unreachable_postgres_app() -> axum::Router {
    let db = PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(500))
        .connect_lazy("postgres://observable_catalog:observable_catalog@127.0.0.1:1/observable")
        .expect("lazy postgres pool");
    common::Harness::with_db(db, None).app
}

async fn get_path(app: &axum::Router, uri: &str) -> axum::response::Response {
    app.clone()
        .oneshot(Request::get(uri).body(Body::empty()).expect("request"))
        .await
        .expect("router responded")
}

#[tokio::test]
async fn catalog_service_health_readyz_and_metrics_endpoints() {
    let app = unreachable_postgres_app();

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
        "catalog_service_http_requests_total",
        "catalog_service_http_request_duration_seconds",
        "catalog_service_http_in_flight_requests",
    ] {
        assert!(body.contains(family), "missing {family} in:\n{body}");
    }
    assert!(
        body.contains("catalog_service_http_requests_total{method=\"GET\",status=\"503\"} 1"),
        "readyz failure must be counted:\n{body}"
    );
}

#[tokio::test]
async fn catalog_api_requires_a_credential() {
    let app = unreachable_postgres_app();
    assert_eq!(
        get_path(&app, "/v1/catalog/services").await.status(),
        StatusCode::UNAUTHORIZED
    );
}
