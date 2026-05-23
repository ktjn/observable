use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use prometheus::{
    Encoder, HistogramVec, IntCounterVec, IntGauge, TextEncoder, histogram_opts, linear_buckets,
    opts, register_histogram_vec, register_int_counter_vec, register_int_gauge,
};
use serde::Deserialize;
use std::time::Instant;

use crate::traces::AppState;

#[derive(Debug, Deserialize, clickhouse::Row)]
struct ReadyCheckRow {
    ok: u8,
}

static HTTP_REQUESTS_TOTAL: std::sync::LazyLock<IntCounterVec> = std::sync::LazyLock::new(|| {
    register_int_counter_vec!(
        opts!(
            "query_api_http_requests_total",
            "Total HTTP requests handled by query-api"
        ),
        &["method", "status"]
    )
    .expect("register query_api_http_requests_total")
});

static HTTP_REQUEST_DURATION_SECONDS: std::sync::LazyLock<HistogramVec> =
    std::sync::LazyLock::new(|| {
        register_histogram_vec!(
            histogram_opts!(
                "query_api_http_request_duration_seconds",
                "HTTP request duration in seconds for query-api",
                linear_buckets(0.005, 0.005, 20).expect("valid histogram buckets")
            ),
            &["method", "status"]
        )
        .expect("register query_api_http_request_duration_seconds")
    });

static HTTP_IN_FLIGHT_REQUESTS: std::sync::LazyLock<IntGauge> = std::sync::LazyLock::new(|| {
    register_int_gauge!(opts!(
        "query_api_http_in_flight_requests",
        "Current in-flight HTTP requests handled by query-api"
    ))
    .expect("register query_api_http_in_flight_requests")
});

fn http_requests_total() -> &'static IntCounterVec {
    &HTTP_REQUESTS_TOTAL
}

fn http_request_duration_seconds() -> &'static HistogramVec {
    &HTTP_REQUEST_DURATION_SECONDS
}

fn http_in_flight_requests() -> &'static IntGauge {
    &HTTP_IN_FLIGHT_REQUESTS
}

fn init_http_metrics() {
    let _ = &*HTTP_REQUESTS_TOTAL;
    let _ = &*HTTP_REQUEST_DURATION_SECONDS;
    let _ = &*HTTP_IN_FLIGHT_REQUESTS;
}

pub async fn record_http_metrics(req: Request, next: Next) -> Response {
    init_http_metrics();
    let method = req.method().as_str().to_owned();
    let start = Instant::now();
    http_in_flight_requests().inc();

    let response = next.run(req).await;

    http_in_flight_requests().dec();
    let status = response.status().as_u16().to_string();
    http_requests_total()
        .with_label_values(&[method.as_str(), &status])
        .inc();
    http_request_duration_seconds()
        .with_label_values(&[method.as_str(), &status])
        .observe(start.elapsed().as_secs_f64());

    response
}

pub async fn metrics() -> Response {
    init_http_metrics();

    let encoder = TextEncoder::new();
    let mut buffer = Vec::new();
    let metric_families = prometheus::gather();
    if encoder.encode(&metric_families, &mut buffer).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let mut response = Response::new(Body::from(buffer));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(encoder.format_type())
            .expect("prometheus text encoder content type is valid"),
    );
    response
}

pub async fn readyz(State(state): State<AppState>) -> StatusCode {
    let postgres_ready = sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&state.db)
        .await
        .is_ok();

    let clickhouse_ready = match state.ch.query("SELECT 1 AS ok").fetch_all().await {
        Ok(rows) => rows.into_iter().any(|row: ReadyCheckRow| row.ok == 1),
        Err(e) => {
            tracing::warn!(error = %e, "query-api readiness clickhouse check failed");
            false
        }
    };

    if postgres_ready && clickhouse_ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, routing::get};
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    #[tokio::test]
    async fn metrics_endpoint_renders_prometheus_text() {
        let response = metrics().await;

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("content type header present")
            .to_str()
            .expect("content type is utf-8");
        assert!(content_type.starts_with("text/plain"));

        let bytes = http_body_util::BodyExt::collect(response.into_body())
            .await
            .expect("metrics body collected")
            .to_bytes();
        let body = String::from_utf8(bytes.to_vec()).expect("metrics body is utf-8");
        assert!(
            body.contains("query_api_http_in_flight_requests"),
            "metrics payload did not contain the in-flight gauge: {body}"
        );
    }

    #[tokio::test]
    async fn metrics_middleware_records_request_status() {
        let app = Router::new()
            .route("/ok", get(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn(record_http_metrics));

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/ok")
                    .body(Body::empty())
                    .expect("request body"),
            )
            .await
            .expect("router responded");
        assert_eq!(response.status(), StatusCode::OK);

        let metrics_response = metrics().await;
        let bytes = http_body_util::BodyExt::collect(metrics_response.into_body())
            .await
            .expect("metrics body collected")
            .to_bytes();
        let body = String::from_utf8(bytes.to_vec()).expect("metrics body is utf-8");
        assert!(body.contains("method=\"GET\""));
        assert!(body.contains("status=\"200\""));
    }

    #[tokio::test]
    async fn readyz_returns_503_when_clickhouse_is_unavailable() {
        let db = PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://user:pass@127.0.0.1:5432/db")
            .expect("lazy postgres pool");
        let ch = clickhouse::Client::default()
            .with_url("http://127.0.0.1:1")
            .with_user("default")
            .with_database("observable");
        let state = AppState {
            ch,
            db,
            planner: std::sync::Arc::new(crate::planner::QueryPlanner),
            llm: None,
            auth_service_url: "http://auth-service:4319".into(),
        };

        assert_eq!(readyz(State(state)).await, StatusCode::SERVICE_UNAVAILABLE);
    }
}
