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
use prost013::Message;
use serde_json::Value;

use crate::{auth, AppState};

pub enum DecodedOtlpRequest<T> {
    Json(Value),
    Protobuf(T),
}

pub fn decode_otlp_http_request<T>(
    headers: &HeaderMap,
    body: Bytes,
) -> Result<DecodedOtlpRequest<T>, StatusCode>
where
    T: Message + Default,
{
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .unwrap_or_default();

    if matches_content_type(content_type, "application/json") {
        let json = serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;
        return Ok(DecodedOtlpRequest::Json(json));
    }

    if matches_content_type(content_type, "application/x-protobuf") {
        let request = T::decode(body).map_err(|_| StatusCode::BAD_REQUEST)?;
        return Ok(DecodedOtlpRequest::Protobuf(request));
    }

    Err(StatusCode::UNSUPPORTED_MEDIA_TYPE)
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
