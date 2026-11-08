//! `observable-catalog`: business service catalog (domain `service-model`).
//!
//! Provides `manage-service-catalog` (phase 1 S3-S4): service records, owners, teams, outcomes,
//! search, exceptions, critical journeys, scope, operating contexts and coverage (S5), with every read authorized through `decide-access`. Coverage
//! (`govern-observability-assets`) and providers (`manage-providers`) follow in S5-S7; see
//! docs/plans/phase1-service-catalog.md.

pub mod api;
pub mod coverage;
pub mod journeys;
pub mod observability;
pub mod ports;
pub mod providers;

use std::sync::Arc;

use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
    routing::{delete, get, post, put},
};
use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use tracing::Level;

use ports::{Credentials, Decider, Directory, Environments, Telemetry};

pub const DEFAULT_PORT: u16 = 4325;

#[derive(Clone)]
pub struct CatalogState {
    pub db: PgPool,
    pub metrics: Arc<observability::CatalogServiceMetrics>,
    pub credentials: Credentials,
    pub decider: Decider,
    pub directory: Directory,
    pub environments: Environments,
    pub telemetry: Telemetry,
    /// Shared secret for `/internal/catalog/*` producer routes; `None` refuses them.
    pub internal_token: Option<Arc<String>>,
}

impl CatalogState {
    pub fn new(
        db: PgPool,
        credentials: Credentials,
        decider: Decider,
        directory: Directory,
        environments: Environments,
        telemetry: Telemetry,
    ) -> Self {
        Self {
            db,
            metrics: Arc::new(observability::CatalogServiceMetrics::new()),
            credentials,
            decider,
            directory,
            environments,
            telemetry,
            internal_token: None,
        }
    }

    pub fn with_internal_token(mut self, token: impl Into<String>) -> Self {
        self.internal_token = Some(Arc::new(token.into()));
        self
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
        .route(
            "/v1/catalog/journeys/{id}/dependencies",
            get(providers::journey_dependencies),
        )
        .route(
            "/v1/catalog/providers",
            get(providers::list_providers).post(providers::create_provider),
        )
        .route("/v1/catalog/providers/gaps", get(providers::provider_gaps))
        .route(
            "/v1/catalog/providers/dependencies",
            post(providers::create_dependency),
        )
        .route(
            "/v1/catalog/providers/dependencies/{id}",
            delete(providers::delete_dependency),
        )
        .route(
            "/v1/catalog/providers/{id}",
            get(providers::get_provider)
                .put(providers::update_provider)
                .delete(providers::delete_provider),
        )
        .route(
            "/v1/catalog/services/{id}/dependencies",
            get(providers::service_dependencies),
        )
        .route("/v1/catalog/scope", get(journeys::scope))
        .route("/v1/catalog/contexts", get(journeys::contexts))
        .route(
            "/v1/catalog/services/{id}/bindings",
            put(coverage::put_bindings),
        )
        .route(
            "/v1/catalog/services/{id}/coverage",
            get(coverage::service_coverage),
        )
        .route(
            "/v1/catalog/services/{id}/coverage/claims/{item}",
            put(coverage::put_claim),
        )
        .route("/v1/catalog/coverage", get(coverage::list_coverage))
        .route("/v1/catalog/coverage/assess", post(coverage::assess_all))
        .route(
            "/v1/catalog/coverage/policies",
            get(coverage::list_policies),
        )
        .route(
            "/v1/catalog/coverage/policies/{criticality}",
            put(coverage::put_policy),
        )
        .route(
            "/v1/catalog/coverage/policies/{criticality}/approve",
            post(coverage::approve_policy),
        )
        .route("/v1/catalog/assets", get(coverage::list_assets))
        .route(
            "/v1/catalog/assets/{kind}/{external_id}",
            put(coverage::put_asset).delete(coverage::delete_asset),
        )
        .route("/v1/catalog/gaps", get(coverage::list_gaps))
        .route(
            "/v1/catalog/gaps/{id}/remediate",
            post(coverage::remediate_gap),
        )
        .route("/v1/catalog/gaps/{id}/accept", post(coverage::accept_gap))
        .route("/v1/catalog/gaps/{id}/close", post(coverage::close_gap))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_caller,
        ));

    // Producer routes authenticate with the internal service token, not a caller credential.
    let internal = Router::new()
        .route(
            "/internal/catalog/assets/sync",
            post(coverage::internal_sync_assets),
        )
        .route(
            "/internal/catalog/assets/{kind}/{external_id}",
            put(coverage::internal_put_asset).delete(coverage::internal_delete_asset),
        );

    Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .merge(internal)
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
