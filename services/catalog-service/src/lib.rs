//! `observable-catalog`: business service catalog (domain `service-model`).
//!
//! Phase 1 slice S1 provides only the runtime skeleton: probes, metrics and
//! the PostgreSQL pool scoped to the `catalog` schema. Catalog capabilities
//! (`manage-service-catalog`, `govern-observability-assets`, `manage-providers`)
//! arrive in later slices; see docs/plans/phase1-service-catalog.md.

pub mod observability;

use std::sync::Arc;

use axum::{Router, http::StatusCode, routing::get};
use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use tracing::Level;

pub const DEFAULT_PORT: u16 = 4325;

#[derive(Clone)]
pub struct CatalogState {
    pub db: PgPool,
    pub metrics: Arc<observability::CatalogServiceMetrics>,
}

impl CatalogState {
    pub fn new(db: PgPool) -> Self {
        Self {
            db,
            metrics: Arc::new(observability::CatalogServiceMetrics::new()),
        }
    }
}

pub fn app(state: CatalogState) -> Router {
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
