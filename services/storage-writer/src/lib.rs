pub mod observability;

use clickhouse::Client;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub ch: Client,
    pub metrics: Arc<observability::StorageWriterMetrics>,
}
