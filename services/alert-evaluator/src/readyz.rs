use axum::{extract::State, http::StatusCode};

use crate::AppState;

pub async fn readyz(State(state): State<AppState>) -> StatusCode {
    if state.db.acquire().await.is_err() {
        tracing::warn!("alert-evaluator readiness postgres check failed");
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    match state.ch.query("SELECT 1").fetch_one::<u8>().await {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            tracing::warn!(error = %e, "alert-evaluator readiness clickhouse check failed");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
