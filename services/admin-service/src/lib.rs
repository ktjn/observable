pub mod admin_members;
pub mod change_events;
pub mod config;
pub mod dashboards;
pub mod deployments;
pub mod internal;
pub mod llm_probe;
pub mod middleware;
pub mod observability;
pub mod queue;
pub mod saved_views;
pub mod schemas;
pub mod tenants;
pub mod tokens;
pub mod usage;

use std::sync::Arc;

use sqlx::PgPool;

/// Shared application state for admin-service handlers.
///
/// `auth_service_url` is used by `middleware::auth::require_tenant`; `ch` (ClickHouse) is
/// used only by `usage.rs`'s tenant usage report — the other three handler modules
/// (`admin_members`, `tokens`, `config`) use `db` only. `http_client` is used by `tenants.rs`'s
/// bootstrap endpoints, which validate an optional session directly (they sit outside the
/// `require_tenant` middleware, so they can't rely on its `Extension<reqwest::Client>` layer).
/// `producer` is `None` when `REDPANDA_BROKERS`/`DEPLOYMENT_MARKERS_TOPIC` aren't configured
/// (e.g. in tests) — `deployments.rs`'s create/finish handlers treat a missing producer as a
/// soft failure (log a warning, still return success) rather than failing the write, since
/// publishing this event is a cache-freshness optimization for ingest-gateway, not required for
/// deployment-marker correctness.
#[derive(Clone)]
pub struct AdminServiceAppState {
    pub db: PgPool,
    pub ch: clickhouse::Client,
    pub auth_service_url: String,
    pub http_client: reqwest::Client,
    pub metrics: Arc<observability::AdminServiceMetrics>,
    pub producer: Option<Arc<queue::DeploymentEventProducer>>,
}
