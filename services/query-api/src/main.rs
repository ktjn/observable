mod traces;

use axum::{routing::get, Router};
use clickhouse::Client;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().json().init();
    let ch_url = std::env::var("CLICKHOUSE_URL")
        .unwrap_or_else(|_| "http://localhost:8123".into());
    let ch = Client::default()
        .with_url(ch_url)
        .with_database("observable");
    let port: u16 = std::env::var("QUERY_API_PORT")
        .unwrap_or_else(|_| "8090".into())
        .parse()?;
    let tenant_id = std::env::var("DEV_TENANT_ID")
        .unwrap_or_else(|_| "00000000-0000-0000-0000-000000000001".into())
        .parse()?;
    let state = traces::AppState { ch, tenant_id };
    let app = Router::new()
        .route("/v1/traces", get(traces::search_traces))
        .route("/v1/traces/:trace_id", get(traces::get_trace))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "query-api listening");
    axum::serve(listener, app).await?;
    Ok(())
}
