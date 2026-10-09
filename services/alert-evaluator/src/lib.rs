pub mod alerts;
pub mod evaluator;
pub mod middleware;
pub mod observability;
pub mod readyz;

use clickhouse::Client;
use std::sync::Arc;

/// Shared application state for alert-evaluator, which is the `observable-alerting`
/// component during decomposition (Phase 6, docs/component-decomposition.md).
///
/// `auth_service_url` and `http_client` are used by `middleware::auth::require_tenant`
/// to validate credentials against auth-service for the alert-rule CRUD routes.
#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub ch: Client,
    pub auth_service_url: String,
    pub http_client: reqwest::Client,
    pub metrics: Arc<observability::AlertEvaluatorMetrics>,
}
