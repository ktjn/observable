pub mod convert;
pub mod logs;
pub mod metrics;
pub mod traces;

use axum::{
    middleware,
    routing::{get, post},
    Router,
};

use crate::{auth, AppState};

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
