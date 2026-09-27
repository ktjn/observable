//! ClickHouse row projections for tracing, logs, and metrics. This is the versioned
//! storage contract between `observable-store-clickhouse` (writer) and
//! `observable-query` (reader) described in ADR-035 -- shared, but not owned, by
//! either component.
mod generated;
mod log;
mod metric;
mod span;

pub use log::LogRow;
pub use metric::{MetricPointRow, MetricSeriesRow};
pub use span::{SpanEventRow, SpanRow};
