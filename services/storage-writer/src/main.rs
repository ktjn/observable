mod logs;
mod metrics;
mod spans;

use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use clickhouse::Client;
use serde::Deserialize;

#[derive(Clone)]
struct AppState {
    ch: Client,
}

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

async fn write_metrics(
    State(state): State<AppState>,
    Json(b): Json<MetricsBatch>,
) -> StatusCode {
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
    let otlp = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
    domain::telemetry::init_telemetry("storage-writer", otlp.as_deref())?;
    let ch_url = std::env::var("CLICKHOUSE_URL")
        .unwrap_or_else(|_| "http://localhost:8123".into());
    let ch = Client::default()
        .with_url(ch_url)
        .with_database("observable");
    let port: u16 = std::env::var("STORAGE_WRITER_PORT")
        .unwrap_or_else(|_| "4320".into())
        .parse()?;
    let app = Router::new()
        .route("/internal/spans", post(write_spans))
        .route("/internal/logs", post(write_logs))
        .route("/internal/metrics", post(write_metrics))
        .with_state(AppState { ch });
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "storage-writer listening");
    axum::serve(listener, app).await?;
    Ok(())
}
