pub mod admin_members;
pub mod config;
pub mod llm_probe;
pub mod middleware;
pub mod observability;
pub mod tokens;
pub mod usage;

use std::sync::Arc;

use sqlx::PgPool;

/// Shared application state for admin-service handlers.
///
/// `auth_service_url` is used by `middleware::auth::require_tenant`; `ch` (ClickHouse) is
/// used only by `usage.rs`'s tenant usage report — the other three handler modules
/// (`admin_members`, `tokens`, `config`) use `db` only.
#[derive(Clone)]
pub struct AdminServiceAppState {
    pub db: PgPool,
    pub ch: clickhouse::Client,
    pub auth_service_url: String,
    pub metrics: Arc<observability::AdminServiceMetrics>,
}
