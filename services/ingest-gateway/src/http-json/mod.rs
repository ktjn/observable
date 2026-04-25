pub mod convert;
pub mod logs;
pub mod metrics;
pub mod traces;

use axum::{
    body::Bytes,
    http::{header, HeaderMap, StatusCode},
    middleware,
    routing::{get, post},
    Router,
};
use flate2::read::GzDecoder;
use serde_json::Value;
use std::io::Read;

use crate::{auth, AppState};

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
        .with_state(state)
}

pub async fn start_http_server(state: AppState, port: u16) -> anyhow::Result<()> {
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "ingest-gateway HTTP listening");
    axum::serve(listener, app)
        .await
        .map_err(anyhow::Error::from)
}
