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
/// `ch` (ClickHouse) and `auth_service_url` are not yet used by this scaffold but are
/// included now so later tasks (member management, API tokens, usage reporting) don't
/// need to touch this struct again.
#[derive(Clone)]
pub struct AdminServiceAppState {
    pub db: PgPool,
    pub ch: clickhouse::Client,
    pub auth_service_url: String,
    pub metrics: Arc<observability::AdminServiceMetrics>,
}
