use axum::{
    body::Body,
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use prometheus::{Encoder, IntCounterVec, IntGauge, Registry, TextEncoder, opts};

use crate::readyz::IngestGatewayProbeState;

#[allow(dead_code)]
pub struct IngestGatewayMetrics {
    pub registry: Registry,
    pub ingest_requests_total: IntCounterVec,
    pub ingest_rejections_total: IntCounterVec,
    pub spans_received_total: IntGauge,
    pub logs_received_total: IntGauge,
    pub metrics_received_total: IntGauge,
}

impl IngestGatewayMetrics {
    pub fn new() -> Self {
        let registry = Registry::new();

        let ingest_requests_total = IntCounterVec::new(
            opts!(
                "ingest_gateway_requests_total",
                "Total ingest requests by signal and protocol"
            ),
            &["signal", "protocol"],
        )
        .expect("create ingest_gateway_requests_total");

        let ingest_rejections_total = IntCounterVec::new(
            opts!(
                "ingest_gateway_rejections_total",
                "Total rejected ingest requests by reason"
            ),
            &["signal", "reason"],
        )
        .expect("create ingest_gateway_rejections_total");

        let spans_received_total = IntGauge::with_opts(opts!(
            "ingest_gateway_spans_received_total",
            "Total spans received"
        ))
        .expect("create ingest_gateway_spans_received_total");

        let logs_received_total = IntGauge::with_opts(opts!(
            "ingest_gateway_logs_received_total",
            "Total log records received"
        ))
        .expect("create ingest_gateway_logs_received_total");

        let metrics_received_total = IntGauge::with_opts(opts!(
            "ingest_gateway_metrics_received_total",
            "Total metric data points received"
        ))
        .expect("create ingest_gateway_metrics_received_total");

        registry
            .register(Box::new(ingest_requests_total.clone()))
            .expect("register ingest_requests_total");
        registry
            .register(Box::new(ingest_rejections_total.clone()))
            .expect("register ingest_rejections_total");
        registry
            .register(Box::new(spans_received_total.clone()))
            .expect("register spans_received_total");
        registry
            .register(Box::new(logs_received_total.clone()))
            .expect("register logs_received_total");
        registry
            .register(Box::new(metrics_received_total.clone()))
            .expect("register metrics_received_total");

        Self {
            registry,
            ingest_requests_total,
            ingest_rejections_total,
            spans_received_total,
            logs_received_total,
            metrics_received_total,
        }
    }
}

impl Default for IngestGatewayMetrics {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn metrics(State(state): State<IngestGatewayProbeState>) -> Response {
    let Some(ref registry) = state.metrics_registry else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let encoder = TextEncoder::new();
    let mut buffer = Vec::new();
    let metric_families = registry.gather();
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
