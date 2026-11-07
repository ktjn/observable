//! `observable-catalog`: business service catalog (domain `service-model`).
//!
//! Provides `manage-service-catalog` (phase 1 S3-S4): service records, owners, teams, outcomes,
//! search, exceptions, critical journeys, scope and operating contexts, with every read authorized through `decide-access`. Coverage
//! (`govern-observability-assets`) and providers (`manage-providers`) follow in S5-S7; see
//! docs/plans/phase1-service-catalog.md.

pub mod api;
pub mod journeys;
pub mod observability;
pub mod ports;

use std::sync::Arc;

use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
    routing::{get, post, put},
};
use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use tracing::Level;

use ports::{Credentials, Decider, Directory, Environments};

pub const DEFAULT_PORT: u16 = 4325;

#[derive(Clone)]
pub struct CatalogState {
    pub db: PgPool,
    pub metrics: Arc<observability::CatalogServiceMetrics>,
    pub credentials: Credentials,
    pub decider: Decider,
    pub directory: Directory,
    pub environments: Environments,
}

impl CatalogState {
    pub fn new(
        db: PgPool,
        credentials: Credentials,
        decider: Decider,
        directory: Directory,
        environments: Environments,
    ) -> Self {
        Self {
            db,
            metrics: Arc::new(observability::CatalogServiceMetrics::new()),
            credentials,
            decider,
            directory,
            environments,
        }
    }
}

/// Resolves the caller through `validate-credential`; unauthenticated requests never reach a
/// catalog handler.
async fn require_caller(
    State(state): State<CatalogState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let caller = state
        .credentials
        .verify(req.headers())
        .await
        .map_err(StatusCode::from)?;
    req.extensions_mut().insert(caller);
    Ok(next.run(req).await)
}

pub fn app(state: CatalogState) -> Router {
    let catalog = Router::new()
        .route(
            "/v1/catalog/services",
            get(api::list_services).post(api::create_service),
        )
        .route(
            "/v1/catalog/services/{id}",
            get(api::get_service).patch(api::update_service),
        )
        .route(
            "/v1/catalog/services/{id}/owners/{role}",
            put(api::put_owner).delete(api::delete_owner),
        )
        .route(
            "/v1/catalog/services/{id}/outcomes",
            post(api::create_outcome),
        )
        .route(
            "/v1/catalog/services/{id}/outcomes/{outcome_id}/approve",
            post(api::approve_outcome),
        )
        .route("/v1/catalog/search", get(api::search))
        .route("/v1/catalog/exceptions", get(api::exceptions))
        .route(
            "/v1/catalog/teams",
            get(api::list_teams).post(api::create_team),
        )
        .route("/v1/catalog/teams/{id}/members", put(api::put_team_members))
        .route(
            "/v1/catalog/journeys",
            get(journeys::list_journeys).post(journeys::create_journey),
        )
        .route(
            "/v1/catalog/journeys/{id}",
            get(journeys::get_journey).patch(journeys::update_journey),
        )
        .route("/v1/catalog/scope", get(journeys::scope))
        .route("/v1/catalog/contexts", get(journeys::contexts))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_caller,
        ));

    Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/readyz", get(observability::readyz))
        .route("/metrics", get(observability::metrics))
        .merge(catalog)
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
