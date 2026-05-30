pub mod buffer;
pub mod logs;
pub mod metrics;
pub mod observability;
pub mod spans;

use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    /// Async write buffer — handlers push here and return immediately.
    pub buffer: Arc<buffer::WriteBuffer>,
    /// Direct ClickHouse client — used by the readyz probe and retention worker only.
    pub ch: clickhouse::Client,
    pub metrics: Arc<observability::StorageWriterMetrics>,
}
