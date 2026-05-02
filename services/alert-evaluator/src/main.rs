mod evaluator;

use axum::{routing::get, Router};
use clickhouse::Client;
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = domain::telemetry::init_self_observability_telemetry("alert-evaluator")?;

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/observable".into());
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let ch_url = std::env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".into());
    let ch_user = std::env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "default".into());
    let ch_password = std::env::var("CLICKHOUSE_PASSWORD").unwrap_or_default();
    let ch = Client::default()
        .with_url(ch_url)
        .with_user(ch_user)
        .with_password(ch_password)
        .with_database("observable");

    let interval_secs = std::env::var("ALERT_EVAL_INTERVAL_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60);

    let port: u16 = std::env::var("ALERT_EVALUATOR_PORT")
        .unwrap_or_else(|_| "4322".into())
        .parse()?;

    tokio::spawn(evaluator::start_eval_worker(
        db,
        ch,
        std::time::Duration::from_secs(interval_secs),
    ));

    let app = Router::new().route("/health", get(|| async { axum::http::StatusCode::OK }));
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "alert-evaluator listening");
    axum::serve(listener, app).await?;
    Ok(())
}
