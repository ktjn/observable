pub mod admin_members;
pub mod alerts;
pub mod change_events;
pub mod config;
pub mod dashboards;
pub mod deployments;
pub mod llm_probe;
pub mod middleware;
pub mod notifications;
pub mod observability;
pub mod saved_views;
pub mod schemas;
pub mod slos;
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
#[derive(Clone)]
pub struct AdminServiceAppState {
    pub db: PgPool,
    pub ch: clickhouse::Client,
    pub auth_service_url: String,
    pub http_client: reqwest::Client,
    pub metrics: Arc<observability::AdminServiceMetrics>,
}
