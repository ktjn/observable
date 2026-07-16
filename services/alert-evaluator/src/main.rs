use alert_evaluator::{AppState, evaluator, observability, readyz};
use axum::{Router, middleware, routing::get};
use clickhouse::Client;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = domain::telemetry::init_self_observability_telemetry("alert-evaluator")?;

    let database_url = domain::config::require_env("DATABASE_URL")?;
    let db = Arc::new(
        PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?,
    );

    let ch_url = domain::config::require_env("CLICKHOUSE_URL")?;
    let ch_user = domain::config::require_env("CLICKHOUSE_USER")?;
    let ch_password = domain::config::require_env_or("CLICKHOUSE_PASSWORD", "");
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
        (*db).clone(),
        ch.clone(),
        std::time::Duration::from_secs(interval_secs),
    ));

    tokio::spawn(evaluator::notification_worker((*db).clone()));

    let metrics = Arc::new(observability::AlertEvaluatorMetrics::new());
    let state = AppState { db, ch, metrics };

    let app = Router::new()
        .route("/health", get(|| async { axum::http::StatusCode::OK }))
        .route("/readyz", get(readyz::readyz))
        .route("/metrics", get(observability::metrics))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            observability::record_http_metrics,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "alert-evaluator listening");
    axum::serve(listener, app).await?;
    Ok(())
}
