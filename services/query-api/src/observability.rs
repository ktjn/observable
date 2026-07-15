use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use prometheus::{
    Encoder, HistogramVec, IntCounterVec, IntGauge, Registry, TextEncoder, histogram_opts,
    linear_buckets, opts,
};
use serde::Deserialize;
use std::time::Instant;

use crate::traces::AppState;

#[derive(Debug, Deserialize, clickhouse::Row)]
struct ReadyCheckRow {
    ok: i64,
}

pub struct QueryApiMetrics {
    pub registry: Registry,
    pub http_requests_total: IntCounterVec,
    pub http_request_duration_seconds: HistogramVec,
    pub http_in_flight_requests: IntGauge,
}

impl QueryApiMetrics {
    pub fn new() -> Self {
        let registry = Registry::new();

        let http_requests_total = IntCounterVec::new(
            opts!(
                "query_api_http_requests_total",
                "Total HTTP requests handled by query-api"
            ),
            &["method", "status"],
        )
        .expect("create query_api_http_requests_total");

        let http_request_duration_seconds = HistogramVec::new(
            histogram_opts!(
                "query_api_http_request_duration_seconds",
                "HTTP request duration in seconds for query-api",
                linear_buckets(0.005, 0.005, 20).expect("valid histogram buckets")
            ),
            &["method", "status"],
        )
        .expect("create query_api_http_request_duration_seconds");

        let http_in_flight_requests = IntGauge::with_opts(opts!(
            "query_api_http_in_flight_requests",
            "Current in-flight HTTP requests handled by query-api"
        ))
        .expect("create query_api_http_in_flight_requests");

        registry
            .register(Box::new(http_requests_total.clone()))
            .expect("register http_requests_total");
        registry
            .register(Box::new(http_request_duration_seconds.clone()))
            .expect("register http_request_duration_seconds");
        registry
            .register(Box::new(http_in_flight_requests.clone()))
            .expect("register http_in_flight_requests");

        Self {
            registry,
            http_requests_total,
            http_request_duration_seconds,
            http_in_flight_requests,
        }
    }
}

impl Default for QueryApiMetrics {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn record_http_metrics(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let method = req.method().as_str().to_owned();
    let start = Instant::now();
    state.metrics.http_in_flight_requests.inc();

    let response = next.run(req).await;

    state.metrics.http_in_flight_requests.dec();
    let status = response.status().as_u16().to_string();
    state
        .metrics
        .http_requests_total
        .with_label_values(&[method.as_str(), &status])
        .inc();
    state
        .metrics
        .http_request_duration_seconds
        .with_label_values(&[method.as_str(), &status])
        .observe(start.elapsed().as_secs_f64());

    response
}

pub async fn metrics(State(state): State<AppState>) -> Response {
    let encoder = TextEncoder::new();
    let mut buffer = Vec::new();
    let metric_families = state.metrics.registry.gather();
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
    let postgres_ready = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.db)
        .await
        .map(|_| true)
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "query-api readiness postgres check failed");
            false
        });

    let clickhouse_ready = match state
        .ch
        .query("SELECT toInt64(1) AS ok")
        .fetch_one::<ReadyCheckRow>()
        .await
    {
        Ok(row) => row.ok == 1,
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
    use std::sync::Arc;
    use tower::ServiceExt;

    fn setup_test_state() -> AppState {
        let db = PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://user:pass@127.0.0.1:5432/db")
            .expect("lazy postgres pool");
        let ch = clickhouse::Client::default()
            .with_url("http://127.0.0.1:1")
            .with_user("default")
            .with_database("observable");
        AppState {
            ch,
            db,
            planner: Arc::new(crate::planner::QueryPlanner),
            llm: None,
            auth_service_url: "http://auth-service:4319".into(),
            http_client: reqwest::Client::new(),
            metrics: Arc::new(QueryApiMetrics::new()),
        }
    }

    #[tokio::test]
    async fn metrics_endpoint_renders_prometheus_text() {
        let state = setup_test_state();
        let response = metrics(State(state)).await;

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
        let state = setup_test_state();
        let app = Router::new()
            .route("/ok", get(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                record_http_metrics,
            ))
            .with_state(state.clone());

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

        let metrics_response = metrics(State(state)).await;
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
        let state = setup_test_state();
        assert_eq!(readyz(State(state)).await, StatusCode::SERVICE_UNAVAILABLE);
    }
}
