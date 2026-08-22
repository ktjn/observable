//! Plain query-parameter structs consumed by [`crate::planner`]. Moved out of
//! `services/query-api`'s handler files (`discovery.rs`, `logs.rs`,
//! `traces.rs`), which re-export these under their original names/paths so
//! existing call sites and the public `query_api::{discovery,logs}::...`
//! paths used by integration tests are unaffected.

use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct TopologyParams {
    pub environment: Option<String>,
    pub from: Option<DateTime<Utc>>,
    #[allow(dead_code)]
    pub to: Option<DateTime<Utc>>,
    pub service: Option<String>,
}

#[derive(Deserialize)]
pub struct LogSearchParams {
    pub service: Option<String>,
    pub severity: Option<i32>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub limit: Option<u32>,
    pub facets: Option<String>, // Comma-separated list of fields to facet
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
pub struct SearchParams {
    pub service: Option<String>,
    pub limit: Option<u32>,
    pub facets: Option<String>, // Comma-separated list of fields to facet
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

/// Column list for `SELECT ... FROM observable.spans`. Must name exactly the
/// same columns that `query-api`'s `SpanRow` deserializes — see the
/// `select_cols_names_match_span_row_field_count` test in
/// `services/query-api/src/traces.rs`.
pub const SELECT_COLS: &str = "tenant_id, trace_id, span_id, service_name, \
    service_namespace, service_version, operation_name, span_kind, \
    start_time_unix_nano, end_time_unix_nano, duration_ns, \
    status_code, status_message, attributes, resource_attributes, \
    environment, host_id, workload, deployment_id, parent_span_id";
