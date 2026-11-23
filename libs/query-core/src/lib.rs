//! Portable query semantic planning, extracted from
//! `services/query-api/src/planner/` and `sql_templates.rs` — no
//! axum/clickhouse/tokio/reqwest dependencies, compiles natively and to
//! `wasm32-unknown-unknown`. See
//! `docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md`
//! section 6.

pub mod change_event_query;
pub mod log_query;
pub mod metric_query;
pub mod params;
pub mod planner;
pub mod service_query;
pub mod sql_templates;
pub mod topology_query;
pub mod trace_query;

pub use params::*;
