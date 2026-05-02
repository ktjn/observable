pub mod convert;
pub mod logs;
pub mod metrics;
pub mod traces;

use axum::{
    body::Bytes,
    extract::Request,
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Router,
};
use flate2::read::GzDecoder;
use serde_json::Value;
use std::io::Read;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing::Level;

use crate::{auth, deployments, AppState};

/// Set the OTel parent context on the current (TraceLayer) span by extracting
/// the W3C `traceparent` header. Must run INSIDE the TraceLayer span.
async fn extract_otel_context(request: Request, next: Next) -> Response {
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;
    let carrier: std::collections::HashMap<String, String> = request
        .headers()
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
        .collect();
    let parent_cx =
        opentelemetry::global::get_text_map_propagator(|propagator| propagator.extract(&carrier));
    let _ = tracing::Span::current().set_parent(parent_cx);
    next.run(request).await
}

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
        .layer(middleware::from_fn::<_, (axum::extract::Request,)>(
            extract_otel_context,
        ))
        .layer(TraceLayer::new_for_http().make_span_with(DefaultMakeSpan::new().level(Level::INFO)))
        .with_state(state)
}

/// Platform API router — Observable-specific authenticated write operations.
/// Hosted on a separate port so the OTLP port (4318) remains strictly OTLP.
pub fn build_platform_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/deployments", post(deployments::create_deployment))
        .route(
            "/v1/deployments/:deployment_id",
            axum::routing::patch(deployments::finish_deployment),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ))
        .route("/health", get(|| async { axum::http::StatusCode::OK }))
        .layer(middleware::from_fn::<_, (axum::extract::Request,)>(
            extract_otel_context,
        ))
        .layer(TraceLayer::new_for_http().make_span_with(DefaultMakeSpan::new().level(Level::INFO)))
        .with_state(state)
}

pub async fn start_http_server(state: AppState, port: u16) -> anyhow::Result<()> {
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "ingest-gateway HTTP/OTLP listening");
    axum::serve(listener, app)
        .await
        .map_err(anyhow::Error::from)
}

pub async fn start_platform_server(state: AppState, port: u16) -> anyhow::Result<()> {
    let app = build_platform_router(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "ingest-gateway Platform API listening");
    axum::serve(listener, app)
        .await
        .map_err(anyhow::Error::from)
}
