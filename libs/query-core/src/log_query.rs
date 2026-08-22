//! Log table/histogram query planning, extracted from the pure (non-I/O)
//! half of `services/query-api/src/mcp_query.rs::execute_log_query` and
//! `QueryPlanner::plan_log_histogram`. Production keeps its existing
//! ClickHouse-flavored SQL (`sql_templates::generate_log_sql`,
//! `planner::plan_log_histogram`) unchanged — this module is additive only,
//! mirroring `trace_query.rs`'s "semantic plan -> dialect renderer -> engine"
//! pattern for the browser-local playground's DuckDB `logs` table. See
//! `docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md`
//! section 10.

use crate::sql_templates::{SqlTemplateError, escape_string_value, parse_time_expr};
use domain_core::nlq::{NlqFilterOp, NlqIr};

/// Semantic (dialect-neutral) filters for a log table query: equality
/// filters on `service_name`/`environment`, free-text `query` as a `body`
/// substring match (mirroring production's `positionCaseInsensitive`), and
/// a resolved time range.
pub struct LogQueryFilters {
    pub service_name: Option<String>,
    pub environment: Option<String>,
    pub body_text: Option<String>,
    pub from_expr: String,
    pub to_expr: String,
}

pub fn extract_log_query_filters(ir: &NlqIr) -> Result<LogQueryFilters, SqlTemplateError> {
    let filter_val = |field: &str| -> Option<String> {
        let want = field.to_lowercase();
        ir.filters
            .iter()
            .find(|f| f.field.to_lowercase() == want && f.op == NlqFilterOp::Eq)
            .map(|f| f.value.clone())
    };

    Ok(LogQueryFilters {
        service_name: filter_val("service_name"),
        environment: filter_val("environment"),
        body_text: ir.query.clone().filter(|s: &String| !s.is_empty()),
        from_expr: parse_time_expr(&ir.time_range.from)?,
        to_expr: parse_time_expr(&ir.time_range.to)?,
    })
}

/// Renders `filters` into a DuckDB-flavored SQL string against the
/// playground's local `logs` table.
pub fn render_log_query_duckdb(filters: &LogQueryFilters) -> String {
    let mut where_clauses: Vec<String> = vec![
        format!("timestamp_unix_nano >= {}", filters.from_expr),
        format!("timestamp_unix_nano <= {}", filters.to_expr),
    ];

    if let Some(svc) = &filters.service_name {
        where_clauses.push(format!("service_name = '{}'", escape_string_value(svc)));
    }
    if let Some(env) = &filters.environment {
        where_clauses.push(format!("environment = '{}'", escape_string_value(env)));
    }
    if let Some(text) = &filters.body_text {
        where_clauses.push(format!("body ILIKE '%{}%'", escape_string_value(text)));
    }

    let where_sql = where_clauses.join(" AND ");
    format!(
        "SELECT \
           log_id, \
           timestamp_unix_nano, \
           observed_timestamp_unix_nano, \
           severity_number, \
           severity_text, \
           body, \
           trace_id, \
           span_id, \
           service_name, \
           environment, \
           host_id \
         FROM logs \
         WHERE {where_sql} \
         ORDER BY timestamp_unix_nano DESC \
         LIMIT 500"
    )
}

/// Bucketed count-by-severity plan, mirroring `plan_log_histogram` in
/// `crate::planner` (which still emits ClickHouse's `intDiv` for
/// production) but rendered for the playground's local `logs` table.
pub struct LogHistogramPlan {
    pub sql: String,
    pub from_ns: u64,
    pub interval_ns: u64,
}

/// Renders a bucketed `(bucket_idx, severity_number) -> count` query.
/// `bucket_count` is clamped to at least 1; the caller fills in zero-count
/// buckets missing from the result using `from_ns`/`interval_ns`, mirroring
/// production's handler.
pub fn render_log_histogram_duckdb(
    from_ns: u64,
    to_ns: u64,
    bucket_count: u32,
    service: Option<&str>,
) -> LogHistogramPlan {
    let bucket_count = bucket_count.max(1);
    let range_ns = to_ns.saturating_sub(from_ns).max(1);
    let interval_ns = (range_ns / bucket_count as u64).max(1);

    let mut where_clauses = vec![
        format!("timestamp_unix_nano >= {from_ns}"),
        format!("timestamp_unix_nano <= {to_ns}"),
    ];
    if let Some(svc) = service {
        where_clauses.push(format!("service_name = '{}'", escape_string_value(svc)));
    }
    let where_sql = where_clauses.join(" AND ");

    let sql = format!(
        "SELECT \
           CAST((timestamp_unix_nano - {from_ns}) / {interval_ns} AS BIGINT) AS bucket_idx, \
           severity_number, \
           count() AS cnt \
         FROM logs \
         WHERE {where_sql} \
         GROUP BY bucket_idx, severity_number \
         ORDER BY bucket_idx ASC"
    );

    LogHistogramPlan {
        sql,
        from_ns,
        interval_ns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain_core::nlq::{NlqFilter, NlqOperation, NlqSignal, NlqTimeRange};

    fn base_ir() -> NlqIr {
        NlqIr {
            operation: NlqOperation::Table,
            signals: vec![NlqSignal::Logs],
            metric: None,
            window: None,
            filters: vec![],
            group_by: vec![],
            resolution: None,
            time_range: NlqTimeRange {
                from: "1700000000000000000".into(),
                to: "1700003600000000000".into(),
            },
            visualization_hint: None,
            percentiles: None,
            catalog_field: None,
            limit: None,
            query: None,
        }
    }

    #[test]
    fn extracts_no_filters_when_absent() {
        let filters = extract_log_query_filters(&base_ir()).unwrap();
        assert!(filters.service_name.is_none());
        assert!(filters.environment.is_none());
        assert!(filters.body_text.is_none());
    }

    #[test]
    fn extracts_equality_filters_by_field() {
        let mut ir = base_ir();
        ir.filters = vec![
            NlqFilter {
                field: "service_name".into(),
                op: NlqFilterOp::Eq,
                value: "checkout".into(),
            },
            NlqFilter {
                field: "environment".into(),
                op: NlqFilterOp::Eq,
                value: "production".into(),
            },
        ];
        let filters = extract_log_query_filters(&ir).unwrap();
        assert_eq!(filters.service_name.as_deref(), Some("checkout"));
        assert_eq!(filters.environment.as_deref(), Some("production"));
    }

    #[test]
    fn ignores_non_eq_filters() {
        let mut ir = base_ir();
        ir.filters = vec![NlqFilter {
            field: "service_name".into(),
            op: NlqFilterOp::Ne,
            value: "checkout".into(),
        }];
        let filters = extract_log_query_filters(&ir).unwrap();
        assert!(filters.service_name.is_none());
    }

    #[test]
    fn render_includes_order_by_and_limit() {
        let filters = extract_log_query_filters(&base_ir()).unwrap();
        let sql = render_log_query_duckdb(&filters);
        assert!(sql.contains("FROM logs"));
        assert!(sql.contains("ORDER BY timestamp_unix_nano DESC"));
        assert!(sql.contains("LIMIT 500"));
    }

    #[test]
    fn render_includes_service_and_environment_filters() {
        let mut ir = base_ir();
        ir.filters = vec![
            NlqFilter {
                field: "service_name".into(),
                op: NlqFilterOp::Eq,
                value: "checkout".into(),
            },
            NlqFilter {
                field: "environment".into(),
                op: NlqFilterOp::Eq,
                value: "production".into(),
            },
        ];
        let filters = extract_log_query_filters(&ir).unwrap();
        let sql = render_log_query_duckdb(&filters);
        assert!(sql.contains("service_name = 'checkout'"));
        assert!(sql.contains("environment = 'production'"));
    }

    #[test]
    fn render_escapes_quotes_in_filter_values() {
        let mut ir = base_ir();
        ir.filters = vec![NlqFilter {
            field: "service_name".into(),
            op: NlqFilterOp::Eq,
            value: "o'brien".into(),
        }];
        let filters = extract_log_query_filters(&ir).unwrap();
        let sql = render_log_query_duckdb(&filters);
        assert!(sql.contains("service_name = 'o\\'brien'"));
    }

    #[test]
    fn render_uses_ilike_for_body_text() {
        let mut ir = base_ir();
        ir.query = Some("failed".into());
        let filters = extract_log_query_filters(&ir).unwrap();
        let sql = render_log_query_duckdb(&filters);
        assert!(sql.contains("body ILIKE '%failed%'"));
    }

    #[test]
    fn empty_query_string_is_treated_as_absent() {
        let mut ir = base_ir();
        ir.query = Some("".into());
        let filters = extract_log_query_filters(&ir).unwrap();
        assert!(filters.body_text.is_none());
    }

    #[test]
    fn invalid_time_expression_is_rejected() {
        let mut ir = base_ir();
        ir.time_range.from = "not-a-time".into();
        assert!(extract_log_query_filters(&ir).is_err());
    }

    // ── histogram ─────────────────────────────────────────────────────────

    #[test]
    fn histogram_interval_divides_range_by_bucket_count() {
        let plan = render_log_histogram_duckdb(0, 60_000_000_000, 60, None);
        assert_eq!(plan.from_ns, 0);
        assert_eq!(plan.interval_ns, 1_000_000_000);
    }

    #[test]
    fn histogram_interval_clamps_to_one_for_degenerate_range() {
        let plan = render_log_histogram_duckdb(1000, 1000, 30, None);
        assert_eq!(plan.interval_ns, 1);
    }

    #[test]
    fn histogram_bucket_count_clamps_to_at_least_one() {
        let plan = render_log_histogram_duckdb(0, 100, 0, None);
        assert_eq!(plan.interval_ns, 100);
    }

    #[test]
    fn histogram_sql_uses_duckdb_arithmetic_not_intdiv() {
        let plan = render_log_histogram_duckdb(1000, 61000, 60, None);
        assert!(
            plan.sql
                .contains("CAST((timestamp_unix_nano - 1000) / 1000 AS BIGINT)")
        );
        assert!(!plan.sql.contains("intDiv"));
        assert!(plan.sql.contains("GROUP BY bucket_idx, severity_number"));
        assert!(plan.sql.contains("FROM logs"));
    }

    #[test]
    fn histogram_sql_includes_service_filter_when_present() {
        let plan = render_log_histogram_duckdb(0, 1000, 10, Some("checkout"));
        assert!(plan.sql.contains("service_name = 'checkout'"));
    }

    #[test]
    fn histogram_sql_omits_service_filter_when_absent() {
        let plan = render_log_histogram_duckdb(0, 1000, 10, None);
        assert!(!plan.sql.contains("service_name"));
    }

    #[test]
    fn histogram_sql_escapes_service_name() {
        let plan = render_log_histogram_duckdb(0, 1000, 10, Some("o'brien"));
        assert!(plan.sql.contains("service_name = 'o\\'brien'"));
    }
}
