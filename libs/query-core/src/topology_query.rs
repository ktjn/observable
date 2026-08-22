//! Service topology query planning and row-shaping for the browser-local
//! playground, mirroring `service_query.rs`'s pattern. Production's
//! `discovery.rs::get_topology`/`planner.rs::plan_topology` run a two-branch
//! ClickHouse UNION (parent/child span joins plus a trace-level
//! co-occurrence branch that approximates call relationships when
//! `parent_span_id` links are sparse) and are untouched here.
//!
//! The playground's generator always produces real, dense `parent_span_id`
//! links (see `generator.rs::generate_spans`), so the co-occurrence branch
//! has nothing to add locally — this renderer covers only the parent/child
//! join, a deliberate scope-down documented in
//! `docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md`
//! section 10.

use crate::sql_templates::escape_string_value;
use serde::Serialize;

/// One aggregated edge row from the playground's DuckDB `spans` table.
#[derive(serde::Deserialize)]
pub struct TopologyEdgeRow {
    pub caller: String,
    pub callee: String,
    pub request_count: u64,
    pub error_count: u64,
    pub p95_latency_ns: f64,
}

#[derive(Serialize)]
pub struct TopologyEdge {
    pub caller: String,
    pub callee: String,
    pub request_count: u64,
    pub error_rate: f64,
    pub p95_latency_ms: f64,
}

/// Renders a DuckDB-flavored parent/child join aggregation against the
/// playground's local `spans` table.
pub fn render_topology_duckdb(
    from_ns: u64,
    to_ns: u64,
    environment: Option<&str>,
    service: Option<&str>,
) -> String {
    let mut where_clauses = vec![
        "child.parent_span_id = parent.span_id".to_string(),
        "child.trace_id = parent.trace_id".to_string(),
        "child.service_name != parent.service_name".to_string(),
        format!("child.start_time_unix_nano >= {from_ns}"),
        format!("child.start_time_unix_nano <= {to_ns}"),
    ];
    if let Some(env) = environment {
        let escaped = escape_string_value(env);
        where_clauses.push(format!(
            "child.environment = '{escaped}' AND parent.environment = '{escaped}'"
        ));
    }
    if let Some(svc) = service {
        let escaped = escape_string_value(svc);
        where_clauses.push(format!(
            "(child.service_name = '{escaped}' OR parent.service_name = '{escaped}')"
        ));
    }
    let where_sql = where_clauses.join(" AND ");

    format!(
        "SELECT \
           parent.service_name AS caller, \
           child.service_name AS callee, \
           count(*) AS request_count, \
           count(*) FILTER (WHERE child.status_code = 'ERROR') AS error_count, \
           quantile_cont(child.duration_ns, 0.95) AS p95_latency_ns \
         FROM spans AS child, spans AS parent \
         WHERE {where_sql} \
         GROUP BY caller, callee \
         ORDER BY request_count DESC"
    )
}

/// Shapes a raw aggregated row into the frontend's `TopologyEdge` response
/// shape (error rate as a fraction, latency in milliseconds).
pub fn topology_edge_from_row(row: TopologyEdgeRow) -> TopologyEdge {
    let error_rate = if row.request_count > 0 {
        (row.error_count as f64) / (row.request_count as f64)
    } else {
        0.0
    };

    TopologyEdge {
        caller: row.caller,
        callee: row.callee,
        request_count: row.request_count,
        error_rate,
        p95_latency_ms: row.p95_latency_ns / 1_000_000.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_includes_join_group_by_and_order_by() {
        let sql = render_topology_duckdb(0, 3_600_000_000_000, None, None);
        assert!(sql.contains("FROM spans AS child, spans AS parent"));
        assert!(sql.contains("child.parent_span_id = parent.span_id"));
        assert!(sql.contains("child.trace_id = parent.trace_id"));
        assert!(sql.contains("GROUP BY caller, callee"));
        assert!(sql.contains("ORDER BY request_count DESC"));
        assert!(sql.contains("quantile_cont(child.duration_ns, 0.95)"));
    }

    #[test]
    fn render_includes_environment_filter_when_present() {
        let sql = render_topology_duckdb(0, 1000, Some("production"), None);
        assert!(sql.contains("child.environment = 'production'"));
        assert!(sql.contains("parent.environment = 'production'"));
    }

    #[test]
    fn render_includes_service_filter_when_present() {
        let sql = render_topology_duckdb(0, 1000, None, Some("checkout"));
        assert!(
            sql.contains("child.service_name = 'checkout' OR parent.service_name = 'checkout'")
        );
    }

    #[test]
    fn render_omits_optional_filters_when_absent() {
        let sql = render_topology_duckdb(0, 1000, None, None);
        assert!(!sql.contains("environment"));
    }

    #[test]
    fn render_escapes_filter_values() {
        let sql = render_topology_duckdb(0, 1000, Some("o'brien"), None);
        assert!(sql.contains("environment = 'o\\'brien'"));
    }

    #[test]
    fn edge_from_row_computes_error_rate_and_ms() {
        let row = TopologyEdgeRow {
            caller: "web".into(),
            callee: "api-gateway".into(),
            request_count: 40,
            error_count: 4,
            p95_latency_ns: 66_000_000.0,
        };
        let edge = topology_edge_from_row(row);
        assert_eq!(edge.caller, "web");
        assert_eq!(edge.callee, "api-gateway");
        assert_eq!(edge.request_count, 40);
        assert_eq!(edge.error_rate, 0.1);
        assert_eq!(edge.p95_latency_ms, 66.0);
    }

    #[test]
    fn edge_from_row_handles_zero_requests() {
        let row = TopologyEdgeRow {
            caller: "web".into(),
            callee: "api-gateway".into(),
            request_count: 0,
            error_count: 0,
            p95_latency_ns: 0.0,
        };
        let edge = topology_edge_from_row(row);
        assert_eq!(edge.error_rate, 0.0);
    }
}
