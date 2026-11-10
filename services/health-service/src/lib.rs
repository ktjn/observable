//! Phase 1 C2.1 runtime for observable-health in the service-assessment domain.
//! C2.2 adds the worker's scoped catalog and telemetry ports (`ports`, `scope`).
//! C2.3 adds evaluation and scope-bound snapshots (`assessment`) and the authorized read
//! contract (`api`).
//! Phase 2 E1 adds threshold conditions and tenant rule revisions (`rules`); E2 adds
//! maintenance windows with second-person approval (`maintenance`); E3 adds transitions,
//! derived interruptions and the history read (`history`); E4 adds data-quality issues
//! (`issues`) and the worker self-status (`self_status`).

pub mod api;
pub mod assessment;
pub mod guard;
pub mod history;
pub mod issues;
pub mod maintenance;
pub mod observability;
pub mod ports;
pub mod rules;
pub mod scope;
pub mod self_status;
pub mod tenant_purge;

use std::sync::Arc;

use axum::{
    Router,
    http::StatusCode,
    routing::{get, post},
};
use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use tracing::Level;

use ports::{CatalogInputs, Credentials, Decider, Evidence, ServiceReads};

pub const DEFAULT_PORT: u16 = 4326;

#[derive(Clone)]
pub struct HealthState {
    pub db: PgPool,
    pub metrics: Arc<observability::HealthServiceMetrics>,
    pub credentials: Credentials,
    pub decider: Decider,
    /// The caller's own catalog read (lifecycle, classification, authorization).
    pub services: ServiceReads,
    /// The worker's tenant-bound catalog inputs (current scope).
    pub inputs: CatalogInputs,
    pub evidence: Evidence,
    /// Evaluation worker progress for `/internal/health/self-status`.
    pub worker: Arc<self_status::WorkerStatus>,
    /// Internal service token; empty refuses every internal caller.
    pub internal_token: Arc<String>,
}

impl HealthState {
    /// Probes only: every port is a substitute that refuses or has no data, so `/v1/health`
    /// answers 401 until real ports are attached with `with_ports`.
    pub fn new(db: PgPool) -> Self {
        Self {
            db,
            metrics: Arc::new(observability::HealthServiceMetrics::new()),
            credentials: Credentials::Static(Arc::new(Default::default())),
            decider: Decider::Mock(observable_auth::authz::MockAccessDecider::new()),
            services: ServiceReads::substitute(),
            inputs: CatalogInputs::substitute(),
            evidence: Evidence::substitute(),
            worker: self_status::WorkerStatus::new(),
            internal_token: Arc::new(String::new()),
        }
    }

    pub fn with_ports(
        mut self,
        credentials: Credentials,
        decider: Decider,
        services: ServiceReads,
        inputs: CatalogInputs,
        evidence: Evidence,
    ) -> Self {
        self.credentials = credentials;
        self.decider = decider;
        self.services = services;
        self.inputs = inputs;
        self.evidence = evidence;
        self
    }

    pub fn with_internal_token(mut self, token: &str) -> Self {
        self.internal_token = Arc::new(token.to_owned());
        self
    }
}

pub fn app(state: HealthState) -> Router {
    Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/readyz", get(observability::readyz))
        .route("/metrics", get(observability::metrics))
        .route("/v1/health/services/{id}", get(api::get_health))
        .route(
            "/internal/health/self-status",
            get(self_status::get_self_status),
        )
        .route("/v1/health/services/{id}/issues", get(issues::get_issues))
        .route(
            "/v1/health/services/{id}/history",
            get(history::get_history),
        )
        .route(
            "/v1/health/services/{id}/maintenance",
            get(maintenance::list).post(maintenance::request),
        )
        .route(
            "/v1/health/maintenance/{id}/approve",
            post(maintenance::approve),
        )
        .route(
            "/v1/health/maintenance/{id}/cancel",
            post(maintenance::cancel),
        )
        .route(
            "/v1/health/rules",
            get(rules::get_rule).put(rules::put_rule),
        )
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
