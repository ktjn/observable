//! Trace table query planning, extracted from the pure (non-I/O) half of
//! `services/query-api/src/mcp_query.rs::execute_trace_query`. Production
//! keeps its existing ClickHouse-flavored inline SQL unchanged — this module
//! is additive only, and `render_trace_query_duckdb` is a *new* dialect
//! renderer for the browser-local playground engine, not a replacement for
//! production behavior. See
//! `docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md`
//! section 10 ("semantic plan -> dialect renderer -> engine").

use crate::sql_templates::{SqlTemplateError, escape_string_value, parse_time_expr};
use domain_core::nlq::{NlqFilterOp, NlqIr};

/// Semantic (dialect-neutral) filters for a trace table query, extracted
/// from an [`NlqIr`] the same way `execute_trace_query` does: equality
/// filters on `service_name`/`status_code`/`environment`, free-text `query`
/// as an operation-name substring match, and a resolved time range.
pub struct TraceQueryFilters {
    pub service_name: Option<String>,
    pub status_code: Option<String>,
    pub environment: Option<String>,
    pub operation_text: Option<String>,
    pub from_expr: String,
    pub to_expr: String,
}

pub fn extract_trace_query_filters(ir: &NlqIr) -> Result<TraceQueryFilters, SqlTemplateError> {
    let filter_val = |field: &str| -> Option<String> {
        let want = field.to_lowercase();
        ir.filters
            .iter()
            .find(|f| f.field.to_lowercase() == want && f.op == NlqFilterOp::Eq)
            .map(|f| f.value.clone())
    };

    Ok(TraceQueryFilters {
        service_name: filter_val("service_name"),
        status_code: filter_val("status_code"),
        environment: filter_val("environment"),
        operation_text: ir.query.clone().filter(|s: &String| !s.is_empty()),
        from_expr: parse_time_expr(&ir.time_range.from)?,
        to_expr: parse_time_expr(&ir.time_range.to)?,
    })
}

/// Renders `filters` into a DuckDB-flavored SQL string against the
/// playground's local `spans` table. This table models API-level semantics
/// rather than mirroring ClickHouse DDL (plan section 7) — `environment` is
/// a plain column here, so no JSON extraction is needed, and
/// `duration_ns / 1000000` replaces ClickHouse's `intDiv`.
pub fn render_trace_query_duckdb(filters: &TraceQueryFilters) -> String {
    let mut where_clauses: Vec<String> = vec![
        "(parent_span_id = '' OR parent_span_id IS NULL)".to_string(),
        format!("start_time_unix_nano >= {}", filters.from_expr),
        format!("start_time_unix_nano <= {}", filters.to_expr),
    ];

    if let Some(svc) = &filters.service_name {
        where_clauses.push(format!("service_name = '{}'", escape_string_value(svc)));
    }
    if let Some(sc) = &filters.status_code {
        where_clauses.push(format!("status_code = '{}'", escape_string_value(sc)));
    }
    if let Some(env) = &filters.environment {
        where_clauses.push(format!("environment = '{}'", escape_string_value(env)));
    }
    if let Some(text) = &filters.operation_text {
        where_clauses.push(format!(
            "operation_name ILIKE '%{}%'",
            escape_string_value(text)
        ));
    }

    let where_sql = where_clauses.join(" AND ");
    format!(
        "SELECT \
           trace_id, \
           service_name AS root_service, \
           operation_name AS root_operation, \
           CAST(duration_ns / 1000000 AS BIGINT) AS duration_ms, \
           status_code, \
           environment, \
           start_time_unix_nano \
         FROM spans \
         WHERE {where_sql} \
         ORDER BY start_time_unix_nano DESC \
         LIMIT 500"
    )
}

/// Bucketed count-of-traces plan, mirroring `plan_trace_histogram` in
/// `crate::planner` (which still emits ClickHouse's `intDiv` for
/// production) but rendered for the playground's local `spans` table.
pub struct TraceHistogramPlan {
    pub sql: String,
    pub from_ns: u64,
    pub interval_ns: u64,
}

/// Renders a bucketed `count(DISTINCT trace_id)` query. `bucket_count` is
/// clamped to at least 1; the caller (mirroring production's handler) is
/// expected to fill in zero-count buckets for indices missing from the
/// result using `from_ns`/`interval_ns`.
pub fn render_trace_histogram_duckdb(
    from_ns: u64,
    to_ns: u64,
    bucket_count: u32,
    service: Option<&str>,
) -> TraceHistogramPlan {
    let bucket_count = bucket_count.max(1);
    let range_ns = to_ns.saturating_sub(from_ns).max(1);
    let interval_ns = (range_ns / bucket_count as u64).max(1);

    let mut where_clauses = vec![
        format!("start_time_unix_nano >= {from_ns}"),
        format!("start_time_unix_nano <= {to_ns}"),
    ];
    if let Some(svc) = service {
        where_clauses.push(format!("service_name = '{}'", escape_string_value(svc)));
    }
    let where_sql = where_clauses.join(" AND ");

    let sql = format!(
        "SELECT \
           CAST((start_time_unix_nano - {from_ns}) / {interval_ns} AS BIGINT) AS bucket_idx, \
           count(DISTINCT trace_id) AS cnt \
         FROM spans \
         WHERE {where_sql} \
         GROUP BY bucket_idx \
         ORDER BY bucket_idx ASC"
    );

    TraceHistogramPlan {
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
            signals: vec![NlqSignal::Traces],
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
        let filters = extract_trace_query_filters(&base_ir()).unwrap();
        assert!(filters.service_name.is_none());
        assert!(filters.status_code.is_none());
        assert!(filters.environment.is_none());
        assert!(filters.operation_text.is_none());
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
                field: "status_code".into(),
                op: NlqFilterOp::Eq,
                value: "ERROR".into(),
            },
            NlqFilter {
                field: "environment".into(),
                op: NlqFilterOp::Eq,
                value: "production".into(),
            },
        ];
        let filters = extract_trace_query_filters(&ir).unwrap();
        assert_eq!(filters.service_name.as_deref(), Some("checkout"));
        assert_eq!(filters.status_code.as_deref(), Some("ERROR"));
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
        let filters = extract_trace_query_filters(&ir).unwrap();
        assert!(filters.service_name.is_none());
    }

    #[test]
    fn time_range_literal_passes_through_unchanged() {
        let filters = extract_trace_query_filters(&base_ir()).unwrap();
        assert_eq!(filters.from_expr, "1700000000000000000");
        assert_eq!(filters.to_expr, "1700003600000000000");
    }

    #[test]
    fn render_includes_parent_span_root_filter_and_limit() {
        let filters = extract_trace_query_filters(&base_ir()).unwrap();
        let sql = render_trace_query_duckdb(&filters);
        assert!(sql.contains("(parent_span_id = '' OR parent_span_id IS NULL)"));
        assert!(sql.contains("ORDER BY start_time_unix_nano DESC"));
        assert!(sql.contains("LIMIT 500"));
        assert!(sql.contains("CAST(duration_ns / 1000000 AS BIGINT)"));
        assert!(!sql.contains("intDiv"));
        assert!(!sql.contains("JSONExtractString"));
    }

    #[test]
    fn render_includes_service_status_environment_filters() {
        let mut ir = base_ir();
        ir.filters = vec![
            NlqFilter {
                field: "service_name".into(),
                op: NlqFilterOp::Eq,
                value: "checkout".into(),
            },
            NlqFilter {
                field: "status_code".into(),
                op: NlqFilterOp::Eq,
                value: "ERROR".into(),
            },
            NlqFilter {
                field: "environment".into(),
                op: NlqFilterOp::Eq,
                value: "production".into(),
            },
        ];
        let filters = extract_trace_query_filters(&ir).unwrap();
        let sql = render_trace_query_duckdb(&filters);
        assert!(sql.contains("service_name = 'checkout'"));
        assert!(sql.contains("status_code = 'ERROR'"));
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
        let filters = extract_trace_query_filters(&ir).unwrap();
        let sql = render_trace_query_duckdb(&filters);
        assert!(sql.contains("service_name = 'o\\'brien'"));
    }

    #[test]
    fn render_uses_ilike_for_operation_text() {
        let mut ir = base_ir();
        ir.query = Some("checkout".into());
        let filters = extract_trace_query_filters(&ir).unwrap();
        let sql = render_trace_query_duckdb(&filters);
        assert!(sql.contains("operation_name ILIKE '%checkout%'"));
    }

    #[test]
    fn empty_query_string_is_treated_as_absent() {
        let mut ir = base_ir();
        ir.query = Some("".into());
        let filters = extract_trace_query_filters(&ir).unwrap();
        assert!(filters.operation_text.is_none());
    }

    #[test]
    fn invalid_time_expression_is_rejected() {
        let mut ir = base_ir();
        ir.time_range.from = "not-a-time".into();
        assert!(extract_trace_query_filters(&ir).is_err());
    }

    // ── histogram ─────────────────────────────────────────────────────────

    #[test]
    fn histogram_interval_divides_range_by_bucket_count() {
        let plan = render_trace_histogram_duckdb(0, 60_000_000_000, 60, None);
        assert_eq!(plan.from_ns, 0);
        assert_eq!(plan.interval_ns, 1_000_000_000);
    }

    #[test]
    fn histogram_interval_clamps_to_one_for_degenerate_range() {
        let plan = render_trace_histogram_duckdb(1000, 1000, 30, None);
        assert_eq!(plan.interval_ns, 1);
    }

    #[test]
    fn histogram_bucket_count_clamps_to_at_least_one() {
        let plan = render_trace_histogram_duckdb(0, 100, 0, None);
        assert_eq!(plan.interval_ns, 100);
    }

    #[test]
    fn histogram_sql_uses_duckdb_arithmetic_not_intdiv() {
        let plan = render_trace_histogram_duckdb(1000, 61000, 60, None);
        assert!(
            plan.sql
                .contains("CAST((start_time_unix_nano - 1000) / 1000 AS BIGINT)")
        );
        assert!(!plan.sql.contains("intDiv"));
        assert!(plan.sql.contains("count(DISTINCT trace_id)"));
        assert!(plan.sql.contains("FROM spans"));
    }

    #[test]
    fn histogram_sql_includes_service_filter_when_present() {
        let plan = render_trace_histogram_duckdb(0, 1000, 10, Some("checkout"));
        assert!(plan.sql.contains("service_name = 'checkout'"));
    }

    #[test]
    fn histogram_sql_omits_service_filter_when_absent() {
        let plan = render_trace_histogram_duckdb(0, 1000, 10, None);
        assert!(!plan.sql.contains("service_name"));
    }

    #[test]
    fn histogram_sql_escapes_service_name() {
        let plan = render_trace_histogram_duckdb(0, 1000, 10, Some("o'brien"));
        assert!(plan.sql.contains("service_name = 'o\\'brien'"));
    }
}
