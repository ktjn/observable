pub mod alerts;
pub mod asset_governance;
pub mod catalog_owner;
pub mod catalog_sync;
pub mod evaluator;
pub mod guidance;
pub mod guidance_graph;
pub mod incidents;
pub mod internal;
pub mod middleware;
pub mod notifications;
pub mod objectives;
pub mod observability;
pub mod procedures;
pub mod queue;
pub mod readyz;
pub mod self_status;
pub mod slos;
pub mod tenant_purge;

use clickhouse::Client;
use std::sync::Arc;

/// Shared application state for alert-evaluator, which is the `observable-alerting`
/// component during decomposition (Phase 6, docs/component-decomposition.md).
///
/// `auth_service_url` and `http_client` are used by `middleware::auth::require_tenant`
/// to validate credentials against auth-service for the alert-rule CRUD routes.
///
/// `producer` is `None` when `REDPANDA_BROKERS`/`ALERTING_STATE_TOPIC` aren't configured
/// (e.g. in tests) -- `incidents.rs`'s status-transition handlers treat a missing producer
/// as a soft failure (log a warning, still return success) rather than failing the write,
/// since publishing `alerting.state.v1` (PUSH-013) is a freshness signal for the read-only
/// SSE subscription facade, not required for incident-state correctness.
#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub ch: Client,
    pub auth_service_url: String,
    pub http_client: reqwest::Client,
    pub metrics: Arc<observability::AlertEvaluatorMetrics>,
    pub producer: Option<Arc<dyn queue::AlertEventPublisher>>,
}
