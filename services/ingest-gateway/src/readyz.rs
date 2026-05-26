use axum::{extract::State, http::StatusCode};
use std::sync::Arc;

#[derive(Clone)]
pub struct IngestGatewayProbeState {
    pub db: Arc<sqlx::PgPool>,
}

pub async fn readyz(State(state): State<IngestGatewayProbeState>) -> StatusCode {
    match state.db.acquire().await {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            tracing::warn!(error = %e, "ingest-gateway readiness postgres check failed");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
