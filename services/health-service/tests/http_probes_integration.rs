use std::time::Duration;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use health_service::{HealthState, app};
use http_body_util::BodyExt as _;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

fn unreachable_postgres_state() -> HealthState {
    let db = PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(500))
        .connect_lazy("postgres://observable_health:observable_health@127.0.0.1:1/observable")
        .expect("lazy postgres pool");
    HealthState::new(db)
}

async fn get_path(app: &axum::Router, uri: &str) -> axum::response::Response {
    app.clone()
        .oneshot(Request::get(uri).body(Body::empty()).expect("request"))
        .await
        .expect("router responded")
}

#[tokio::test]
async fn health_service_health_readyz_and_metrics_endpoints() {
    let app = app(unreachable_postgres_state());

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
        "health_service_http_requests_total",
        "health_service_http_request_duration_seconds",
        "health_service_http_in_flight_requests",
    ] {
        assert!(body.contains(family), "missing {family} in:\n{body}");
    }
    assert!(
        body.contains("health_service_http_requests_total{method=\"GET\",status=\"503\"} 1"),
        "readyz failure must be counted:\n{body}"
    );
}

#[tokio::test]
async fn health_service_unknown_route_is_not_found() {
    let app = app(unreachable_postgres_state());
    assert_eq!(
        get_path(
            &app,
            "/v1/health/services/00000000-0000-0000-0000-000000000000?environment_class=production"
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn migrated_health_login_can_use_only_its_own_schema() {
    let (admin, _guard) = test_support::postgres::isolated_pool().await;
    sqlx::query("CREATE TABLE health.probe_fixture (value integer NOT NULL)")
        .execute(&admin)
        .await
        .expect("migration creates health schema");
    let options = admin
        .connect_options()
        .as_ref()
        .clone()
        .username("observable_health")
        .password("observable_health");
    let db = PgPoolOptions::new()
        .connect_with(options)
        .await
        .expect("scoped health login");
    let app = app(HealthState::new(db.clone()));
    assert_eq!(get_path(&app, "/readyz").await.status(), StatusCode::OK);
    sqlx::query("INSERT INTO health.probe_fixture VALUES (1)")
        .execute(&db)
        .await
        .expect("own schema write");
    let value: i32 = sqlx::query_scalar("SELECT value FROM health.probe_fixture")
        .fetch_one(&db)
        .await
        .expect("own schema read");
    assert_eq!(value, 1);
    for schema in ["auth", "catalog", "control", "alerting"] {
        let allowed: bool =
            sqlx::query_scalar("SELECT has_schema_privilege(current_user, $1, 'USAGE')")
                .bind(schema)
                .fetch_one(&db)
                .await
                .expect("schema privilege check");
        assert!(!allowed, "health login has access to {schema}");
    }
}
