mod retention;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use clickhouse::Client;
use serde::Deserialize;
use std::sync::Arc;
use storage_writer::{AppState, buffer, observability};
use tower_http::trace::TraceLayer;

async fn write_spans(
    State(state): State<AppState>,
    Json(batch): Json<Vec<domain::Span>>,
) -> StatusCode {
    state.buffer.send_spans(batch);
    StatusCode::NO_CONTENT
}

async fn write_logs(
    State(state): State<AppState>,
    Json(batch): Json<Vec<domain::LogRecord>>,
) -> StatusCode {
    state.buffer.send_logs(batch);
    StatusCode::NO_CONTENT
}

#[derive(Deserialize)]
struct MetricsBatch {
    series: Vec<domain::MetricSeries>,
    points: Vec<domain::MetricPoint>,
}

async fn write_metrics(State(state): State<AppState>, Json(b): Json<MetricsBatch>) -> StatusCode {
    state.buffer.send_metrics(b.series, b.points);
    StatusCode::NO_CONTENT
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = domain::telemetry::init_self_observability_telemetry("storage-writer")?;
    let ch_url = std::env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".into());
    let ch_user = std::env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "default".into());
    let ch_password = std::env::var("CLICKHOUSE_PASSWORD").unwrap_or_default();
    let ch = Client::default()
        .with_url(ch_url)
        .with_user(ch_user)
        .with_password(ch_password)
        .with_database("observable");
    let port: u16 = std::env::var("STORAGE_WRITER_PORT")
        .unwrap_or_else(|_| "4320".into())
        .parse()?;
    let flush_max_rows: usize = std::env::var("STORAGE_WRITER_FLUSH_MAX_ROWS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5_000);
    let flush_interval = std::time::Duration::from_millis(
        std::env::var("STORAGE_WRITER_FLUSH_INTERVAL_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(500),
    );
    let buffer = Arc::new(buffer::WriteBuffer::new(
        ch.clone(),
        flush_max_rows,
        flush_interval,
    ));
    let retention_config = retention::RetentionConfig::from_env();
    tokio::spawn(retention::start_retention_worker(
        ch.clone(),
        retention_config,
    ));
    let state = AppState {
        buffer,
        ch,
        metrics: Arc::new(observability::StorageWriterMetrics::new()),
    };
    let app = Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/readyz", get(observability::readyz))
        .route("/metrics", get(observability::metrics))
        .route("/internal/spans", post(write_spans))
        .route("/internal/logs", post(write_logs))
        .route("/internal/metrics", post(write_metrics))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            observability::record_http_metrics,
        ))
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &axum::extract::Request| {
                use tracing_opentelemetry::OpenTelemetrySpanExt as _;

                // Suppress span creation when processing observable-environment data to
                // prevent feedback loops: observable spans going through the pipeline
                // would generate new observable spans, which would loop indefinitely.
                let is_observable = request
                    .headers()
                    .get("x-observable-environment")
                    .and_then(|v| v.to_str().ok())
                    .map(|v| v == domain::telemetry::SELF_TELEMETRY_ENV)
                    .unwrap_or(false);
                if is_observable {
                    return tracing::Span::none();
                }

                // Extract incoming W3C traceparent before creating the span so
                // set_parent succeeds (SpanBuilder not yet consumed at this point).
                let carrier: std::collections::HashMap<String, String> = request
                    .headers()
                    .iter()
                    .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
                    .collect();
                let parent_cx =
                    opentelemetry::global::get_text_map_propagator(|p| p.extract(&carrier));
                let span = tracing::info_span!(
                    "request",
                    method = %request.method(),
                    uri = %request.uri().path(),
                );
                let _ = span.set_parent(parent_cx);
                span
            }),
        )
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "storage-writer listening");
    axum::serve(listener, app).await?;
    Ok(())
}
