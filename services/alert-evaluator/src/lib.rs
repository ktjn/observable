pub mod evaluator;
pub mod readyz;

use clickhouse::Client;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<sqlx::PgPool>,
    pub ch: Client,
}
