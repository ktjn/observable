//! ClickHouse row projections for tracing, logs, and metrics. This is the versioned
//! storage contract between `observable-store-clickhouse` (writer) and
//! `observable-query` (reader) described in ADR-035 -- shared, but not owned, by
//! either component. `watermark` reads the per-tenant ingest watermark (phase 2 E5).
mod generated;
mod log;
mod metric;
mod span;
pub mod watermark;

pub use log::LogRow;
pub use metric::{MetricPointRow, MetricSeriesRow};
pub use span::{SpanEventRow, SpanRow};
