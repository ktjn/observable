mod discovery;
mod logs;
mod metrics;
mod middleware;
mod traces;

use axum::{middleware as axum_middleware, routing::get, Router};
use clickhouse::Client;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let otlp = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
    domain::telemetry::init_telemetry("query-api", otlp.as_deref())?;
    let ch_url = std::env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".into());
    let ch_user = std::env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "default".into());
    let ch_password = std::env::var("CLICKHOUSE_PASSWORD").unwrap_or_default();
    let ch = Client::default()
        .with_url(ch_url)
        .with_user(ch_user)
        .with_password(ch_password)
        .with_database("observable");
    let port: u16 = std::env::var("QUERY_API_PORT")
        .unwrap_or_else(|_| "8090".into())
        .parse()?;
    let state = traces::AppState { ch };
    let app = Router::new()
        .route("/health", get(|| async { axum::http::StatusCode::OK }))
        .route("/v1/traces", get(traces::search_traces))
        .route("/v1/traces/:trace_id", get(traces::get_trace))
        .route("/v1/logs", get(logs::search_logs))
        .route("/v1/metrics", get(metrics::list_metrics))
        .route("/v1/metrics/:series_id", get(metrics::get_metric_points))
        .route("/v1/services", get(discovery::list_services))
        .route("/v1/environments", get(discovery::list_environments))
        .layer(axum_middleware::from_fn(middleware::auth::require_tenant))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "query-api listening");
    axum::serve(listener, app).await?;
    Ok(())
}
