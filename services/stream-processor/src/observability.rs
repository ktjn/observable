use axum::{
    body::Body,
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use prometheus::{Encoder, IntCounterVec, IntGauge, Registry, TextEncoder, opts};

use crate::readyz::StreamProcessorProbeState;

pub struct StreamProcessorMetrics {
    pub registry: Registry,
    pub batches_processed_total: IntCounterVec,
    pub envelopes_processed_total: IntGauge,
    pub flush_errors_total: IntCounterVec,
}

impl StreamProcessorMetrics {
    pub fn new() -> Self {
        let registry = Registry::new();

        let batches_processed_total = IntCounterVec::new(
            opts!(
                "stream_processor_batches_processed_total",
                "Total batches processed by stream-processor"
            ),
            &["signal"],
        )
        .expect("create stream_processor_batches_processed_total");

        let envelopes_processed_total = IntGauge::with_opts(opts!(
            "stream_processor_envelopes_processed_total",
            "Total envelopes processed by stream-processor"
        ))
        .expect("create stream_processor_envelopes_processed_total");

        let flush_errors_total = IntCounterVec::new(
            opts!(
                "stream_processor_flush_errors_total",
                "Total errors flushing span metrics"
            ),
            &["stage"],
        )
        .expect("create stream_processor_flush_errors_total");

        registry
            .register(Box::new(batches_processed_total.clone()))
            .expect("register batches_processed_total");
        registry
            .register(Box::new(envelopes_processed_total.clone()))
            .expect("register envelopes_processed_total");
        registry
            .register(Box::new(flush_errors_total.clone()))
            .expect("register flush_errors_total");

        Self {
            registry,
            batches_processed_total,
            envelopes_processed_total,
            flush_errors_total,
        }
    }
}

impl Default for StreamProcessorMetrics {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn metrics(State(state): State<StreamProcessorProbeState>) -> Response {
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
