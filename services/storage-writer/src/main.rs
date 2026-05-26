mod logs;
mod metrics;
mod retention;
mod spans;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use clickhouse::Client;
use serde::Deserialize;
use std::sync::Arc;
use storage_writer::{AppState, observability};
use tower_http::trace::TraceLayer;

async fn write_spans(
    State(state): State<AppState>,
    Json(batch): Json<Vec<domain::Span>>,
) -> StatusCode {
    match spans::insert_spans(&state.ch, batch).await {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(e) => {
            tracing::error!(error = %e, "clickhouse write failed");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

async fn write_logs(
    State(state): State<AppState>,
    Json(batch): Json<Vec<domain::LogRecord>>,
) -> StatusCode {
    match logs::insert_logs(&state.ch, batch).await {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(e) => {
            tracing::error!(error = %e, "ch write failed");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

#[derive(Deserialize)]
struct MetricsBatch {
    series: Vec<domain::MetricSeries>,
    points: Vec<domain::MetricPoint>,
}

async fn write_metrics(State(state): State<AppState>, Json(b): Json<MetricsBatch>) -> StatusCode {
    let r1 = metrics::insert_metric_series(&state.ch, b.series).await;
    let r2 = metrics::insert_metric_points(&state.ch, b.points).await;
    if r1.is_err() || r2.is_err() {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::NO_CONTENT
    }
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
    let retention_config = retention::RetentionConfig::from_env();
    tokio::spawn(retention::start_retention_worker(
        ch.clone(),
        retention_config,
    ));
    let state = AppState {
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
                    .map(|v| v == "observable")
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
