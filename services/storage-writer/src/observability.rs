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
use std::time::Instant;

use crate::AppState;

pub struct StorageWriterMetrics {
    pub registry: Registry,
    pub http_requests_total: IntCounterVec,
    pub http_request_duration_seconds: HistogramVec,
    pub http_in_flight_requests: IntGauge,
}

impl StorageWriterMetrics {
    pub fn new() -> Self {
        let registry = Registry::new();

        let http_requests_total = IntCounterVec::new(
            opts!(
                "storage_writer_http_requests_total",
                "Total HTTP requests handled by storage-writer"
            ),
            &["method", "status"],
        )
        .expect("create storage_writer_http_requests_total");

        let http_request_duration_seconds = HistogramVec::new(
            histogram_opts!(
                "storage_writer_http_request_duration_seconds",
                "HTTP request duration in seconds for storage-writer",
                linear_buckets(0.005, 0.005, 20).expect("valid histogram buckets")
            ),
            &["method", "status"],
        )
        .expect("create storage_writer_http_request_duration_seconds");

        let http_in_flight_requests = IntGauge::with_opts(opts!(
            "storage_writer_http_in_flight_requests",
            "Current in-flight HTTP requests handled by storage-writer"
        ))
        .expect("create storage_writer_http_in_flight_requests");

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

impl Default for StorageWriterMetrics {
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
        HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
    );
    response
}

pub async fn readyz(State(state): State<AppState>) -> StatusCode {
    match state.ch.query("SELECT 1").fetch_one::<u8>().await {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            tracing::warn!(error = %e, "storage-writer readiness clickhouse check failed");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
