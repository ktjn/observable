pub mod convert;
pub mod logs;
pub mod metrics;
pub mod traces;

use axum::{
    Router,
    body::Bytes,
    http::{HeaderMap, StatusCode, header},
    middleware,
    routing::{get, post},
};
use flate2::read::GzDecoder;
use serde_json::Value;
use std::io::Read;
use tower_http::trace::TraceLayer;
use tracing::Level;

use crate::{AppState, auth, change_events, deployments, prometheus_rw};

pub fn decode_json_otlp_request(headers: &HeaderMap, body: Bytes) -> Result<Value, StatusCode> {
    let content_type = get_content_type(headers);
    let body = decode_request_body(headers, body)?;

    if matches_content_type(content_type, "application/json") {
        return serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST);
    }

    Err(StatusCode::UNSUPPORTED_MEDIA_TYPE)
}

fn decode_request_body(headers: &HeaderMap, body: Bytes) -> Result<Vec<u8>, StatusCode> {
    match get_content_encoding(headers) {
        "" | "identity" => Ok(body.to_vec()),
        "gzip" => {
            let mut decoder = GzDecoder::new(body.as_ref());
            let mut decoded = Vec::new();
            decoder
                .read_to_end(&mut decoded)
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            Ok(decoded)
        }
        "zstd" => zstd::decode_all(std::io::Cursor::new(body.as_ref()))
            .map_err(|_| StatusCode::BAD_REQUEST),
        _ => Err(StatusCode::UNSUPPORTED_MEDIA_TYPE),
    }
}

fn get_content_type(headers: &HeaderMap) -> &str {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .unwrap_or_default()
}

fn get_content_encoding(headers: &HeaderMap) -> &str {
    headers
        .get(header::CONTENT_ENCODING)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .unwrap_or_default()
}

fn matches_content_type(actual: &str, expected: &str) -> bool {
    actual
        .split(';')
        .next()
        .map(str::trim)
        .map(|value| value.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

/// OTLP/HTTP router — strictly OTLP signals only (ADR-001, ADR-023).
/// Non-OTLP platform writes (e.g. deployment markers) belong on the platform port.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/traces", post(traces::export_traces))
        .route("/v1/logs", post(logs::export_logs))
        .route("/v1/metrics", post(metrics::export_metrics))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ))
        .route("/health", get(|| async { axum::http::StatusCode::OK }))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(domain::telemetry::OtelMakeSpan::new(Level::INFO)),
        )
        .with_state(state)
}

/// Platform API router — Observable-specific authenticated write operations.
/// Hosted on a separate port so the OTLP port (4318) remains strictly OTLP.
pub fn build_platform_router(
    state: AppState,
    probe_state: crate::readyz::IngestGatewayProbeState,
) -> Router {
    let authenticated = Router::new()
        .route("/v1/deployments", post(deployments::create_deployment))
        .route(
            "/v1/deployments/{deployment_id}",
            axum::routing::patch(deployments::finish_deployment),
        )
        .route(
            "/v1/events/changes",
            post(change_events::create_change_event),
        )
        .route("/api/v1/write", post(prometheus_rw::write))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ))
        .with_state(state);

    let probes = Router::new()
        .route("/health", get(|| async { axum::http::StatusCode::OK }))
        .route("/readyz", get(crate::readyz::readyz))
        .with_state(probe_state);

    Router::new().merge(authenticated).merge(probes).layer(
        TraceLayer::new_for_http()
            .make_span_with(domain::telemetry::OtelMakeSpan::new(Level::INFO)),
    )
}

pub async fn start_http_server(state: AppState, port: u16) -> anyhow::Result<()> {
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "ingest-gateway HTTP/OTLP listening");
    axum::serve(listener, app)
        .await
        .map_err(anyhow::Error::from)
}

pub async fn start_platform_server(
    state: AppState,
    probe_state: crate::readyz::IngestGatewayProbeState,
    port: u16,
) -> anyhow::Result<()> {
    let app = build_platform_router(state, probe_state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "ingest-gateway Platform API listening");
    axum::serve(listener, app)
        .await
        .map_err(anyhow::Error::from)
}
