pub mod buffer;
pub mod logs;
pub mod metrics;
pub mod normalized_consumer;
pub mod observability;
pub mod spans;

use clickhouse::Client;
use serde::Serialize;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    /// Async write buffer — handlers push here and return immediately.
    pub buffer: Arc<buffer::WriteBuffer>,
    /// Direct ClickHouse client — used by the readyz probe and retention worker only.
    pub ch: clickhouse::Client,
    pub metrics: Arc<observability::StorageWriterMetrics>,
}

/// Computes a deterministic FNV-1a 128-bit deduplication token for a batch of
/// rows targeting `table` and returns a ClickHouse client configured with
/// `insert_deduplication_token` and `non_replicated_deduplication_window`.
///
/// This makes crash-window redeliveries idempotent on single-node `MergeTree`
/// tables (`spans`, `span_events`, `logs`, `metric_points`) as well as
/// `ReplacingMergeTree` (`metric_series`).
pub fn deduplicated_client<T: Serialize>(ch: &Client, table: &str, rows: &[T]) -> Client {
    let token = batch_deduplication_token(table, rows);
    ch.clone()
        .with_setting("non_replicated_deduplication_window", "1000")
        .with_setting("insert_deduplication_token", token)
}

pub fn batch_deduplication_token<T: Serialize>(table: &str, rows: &[T]) -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x00000100000001b3;

    let mut hi = FNV_OFFSET;
    let mut lo = FNV_OFFSET ^ 0x9e3779b97f4a7c15;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            hi ^= u64::from(*b);
            hi = hi.wrapping_mul(FNV_PRIME);
            lo ^= u64::from(*b).rotate_left(1);
            lo = lo.wrapping_mul(FNV_PRIME);
        }
    };
    feed(table.as_bytes());
    feed(b":");
    for row in rows {
        if let Ok(json) = serde_json::to_vec(row) {
            feed(&json);
            feed(b"\n");
        }
    }
    format!("{table}-{hi:016x}{lo:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_deduplication_token_is_deterministic_and_content_sensitive() {
        let a = batch_deduplication_token("spans", &["row-1", "row-2"]);
        let b = batch_deduplication_token("spans", &["row-1", "row-2"]);
        let c = batch_deduplication_token("spans", &["row-1", "row-3"]);
        let d = batch_deduplication_token("logs", &["row-1", "row-2"]);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
    }
}
