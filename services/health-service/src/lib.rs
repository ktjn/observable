//! Phase 1 C2.1 runtime for observable-health in the service-assessment domain.
//! Health assessment routes are delivered with the authorized provider contract.

pub mod observability;

use std::sync::Arc;

use axum::{Router, http::StatusCode, routing::get};
use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use tracing::Level;

pub const DEFAULT_PORT: u16 = 4326;

#[derive(Clone)]
pub struct HealthState {
    pub db: PgPool,
    pub metrics: Arc<observability::HealthServiceMetrics>,
}

impl HealthState {
    pub fn new(db: PgPool) -> Self {
        Self {
            db,
            metrics: Arc::new(observability::HealthServiceMetrics::new()),
        }
    }
}

pub fn app(state: HealthState) -> Router {
    Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/readyz", get(observability::readyz))
        .route("/metrics", get(observability::metrics))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            observability::record_http_metrics,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(observable_telemetry::OtelMakeSpan::new(Level::INFO)),
        )
        .with_state(state)
}
