//! Service summary query planning and row-shaping, extracted from the pure
//! (non-I/O) half of `services/query-api/src/discovery.rs::list_service_summaries`
//! (`service_summary_from_row`, `health_state` are moved verbatim; the SQL
//! aggregation itself is a *new* DuckDB dialect renderer for the
//! browser-local playground engine, mirroring `trace_query.rs`/`log_query.rs`).
//! Production keeps its existing ClickHouse-flavored SQL and Postgres-backed
//! catalog enrichment (`active_alert_count`, `latest_deployment`) unchanged —
//! the playground has no admin-service/Postgres locally, so those two fields
//! are always `0`/`None` here. See
//! `docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md`
//! section 10.

use crate::sql_templates::escape_string_value;
use serde::Serialize;

/// One aggregated row from the playground's DuckDB `spans` table.
#[derive(serde::Deserialize)]
pub struct ServiceSummaryRow {
    pub service_name: String,
    pub request_count: u64,
    pub error_count: u64,
    pub p95_latency_ns: f64,
}

#[derive(Serialize)]
pub struct ServiceSummary {
    pub service_name: String,
    pub request_rate: f64,
    pub error_rate: f64,
    pub p95_latency_ms: f64,
    pub health_state: String,
    pub active_alert_count: u64,
    pub latest_deployment: Option<String>,
}

/// Renders a DuckDB-flavored per-service aggregation query against the
/// playground's local `spans` table, mirroring `list_service_summaries`'s
/// ClickHouse query (`count()`, `countIf(status_code = 'ERROR')`,
/// `quantile(0.95)(duration_ns)`) with DuckDB's `FILTER` clause and
/// `quantile_cont` instead.
pub fn render_service_summary_duckdb(
    from_ns: u64,
    to_ns: u64,
    environment: Option<&str>,
) -> String {
    let mut where_clauses = vec![
        format!("start_time_unix_nano >= {from_ns}"),
        format!("start_time_unix_nano <= {to_ns}"),
    ];
    if let Some(env) = environment {
        where_clauses.push(format!("environment = '{}'", escape_string_value(env)));
    }
    let where_sql = where_clauses.join(" AND ");

    format!(
        "SELECT \
           service_name, \
           count(*) AS request_count, \
           count(*) FILTER (WHERE status_code = 'ERROR') AS error_count, \
           quantile_cont(duration_ns, 0.95) AS p95_latency_ns \
         FROM spans \
         WHERE {where_sql} \
         GROUP BY service_name \
         ORDER BY service_name"
    )
}

/// Moved verbatim from `discovery.rs::service_summary_from_row`, except
/// `active_alert_count`/`latest_deployment` are always the "no catalog
/// enrichment available" defaults (see module doc comment).
pub fn service_summary_from_row(row: ServiceSummaryRow, duration_secs: f64) -> ServiceSummary {
    let error_rate = if row.request_count > 0 {
        (row.error_count as f64) / (row.request_count as f64)
    } else {
        0.0
    };

    ServiceSummary {
        service_name: row.service_name,
        request_rate: (row.request_count as f64) / duration_secs,
        error_rate,
        p95_latency_ms: row.p95_latency_ns / 1_000_000.0,
        health_state: health_state(error_rate).to_string(),
        active_alert_count: 0,
        latest_deployment: None,
    }
}

/// Moved verbatim from `discovery.rs::health_state`.
pub fn health_state(error_rate: f64) -> &'static str {
    if error_rate > 0.05 {
        "breach"
    } else if error_rate > 0.01 {
        "watch"
    } else {
        "healthy"
    }
}

/// Renders a DuckDB-flavored bucketed response-time query for one service
/// against the playground's local `spans` table, mirroring
/// `crate::planner::plan_response_time_histogram` (still ClickHouse
/// `intDiv`/`quantile` for production) with DuckDB's arithmetic and
/// `quantile_cont` instead.
pub fn render_response_time_histogram_duckdb(
    from_ns: u64,
    to_ns: u64,
    bucket_count: u32,
    service_name: &str,
) -> ResponseTimeHistogramPlan {
    let bucket_count = bucket_count.max(1);
    let range_ns = to_ns.saturating_sub(from_ns).max(1);
    let interval_ns = (range_ns / bucket_count as u64).max(1);
    let service_name = escape_string_value(service_name);

    let sql = format!(
        "SELECT \
           CAST((start_time_unix_nano - {from_ns}) / {interval_ns} AS BIGINT) AS bucket_idx, \
           quantile_cont(duration_ns, 0.50) AS p50_latency_ns, \
           quantile_cont(duration_ns, 0.95) AS p95_latency_ns, \
           count(*) AS span_count \
         FROM spans \
         WHERE service_name = '{service_name}' \
           AND start_time_unix_nano >= {from_ns} \
           AND start_time_unix_nano <= {to_ns} \
         GROUP BY bucket_idx \
         ORDER BY bucket_idx ASC"
    );

    ResponseTimeHistogramPlan {
        sql,
        from_ns,
        interval_ns,
    }
}

pub struct ResponseTimeHistogramPlan {
    pub sql: String,
    pub from_ns: u64,
    pub interval_ns: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_includes_group_by_and_order_by() {
        let sql = render_service_summary_duckdb(0, 3_600_000_000_000, None);
        assert!(sql.contains("FROM spans"));
        assert!(sql.contains("GROUP BY service_name"));
        assert!(sql.contains("ORDER BY service_name"));
        assert!(sql.contains("quantile_cont(duration_ns, 0.95)"));
        assert!(!sql.contains("quantile(0.95)"));
        assert!(!sql.contains("countIf"));
    }

    #[test]
    fn render_includes_environment_filter_when_present() {
        let sql = render_service_summary_duckdb(0, 1000, Some("production"));
        assert!(sql.contains("environment = 'production'"));
    }

    #[test]
    fn render_omits_environment_filter_when_absent() {
        let sql = render_service_summary_duckdb(0, 1000, None);
        assert!(!sql.contains("environment"));
    }

    #[test]
    fn render_escapes_environment_value() {
        let sql = render_service_summary_duckdb(0, 1000, Some("o'brien"));
        assert!(sql.contains("environment = 'o\\'brien'"));
    }

    #[test]
    fn health_state_thresholds() {
        assert_eq!(health_state(0.0), "healthy");
        assert_eq!(health_state(0.01), "healthy");
        assert_eq!(health_state(0.011), "watch");
        assert_eq!(health_state(0.05), "watch");
        assert_eq!(health_state(0.051), "breach");
    }

    #[test]
    fn summary_from_row_computes_rates_and_ms() {
        let row = ServiceSummaryRow {
            service_name: "checkout".into(),
            request_count: 200,
            error_count: 10,
            p95_latency_ns: 250_000_000.0,
        };
        let summary = service_summary_from_row(row, 100.0);
        assert_eq!(summary.service_name, "checkout");
        assert_eq!(summary.request_rate, 2.0);
        assert_eq!(summary.error_rate, 0.05);
        assert_eq!(summary.p95_latency_ms, 250.0);
        assert_eq!(summary.health_state, "watch");
        assert_eq!(summary.active_alert_count, 0);
        assert!(summary.latest_deployment.is_none());
    }

    #[test]
    fn summary_from_row_handles_zero_requests() {
        let row = ServiceSummaryRow {
            service_name: "idle".into(),
            request_count: 0,
            error_count: 0,
            p95_latency_ns: 0.0,
        };
        let summary = service_summary_from_row(row, 100.0);
        assert_eq!(summary.error_rate, 0.0);
        assert_eq!(summary.health_state, "healthy");
    }

    // ── response-time histogram ──────────────────────────────────────────

    #[test]
    fn response_time_histogram_interval_divides_range_by_bucket_count() {
        let plan = render_response_time_histogram_duckdb(0, 60_000_000_000, 60, "checkout");
        assert_eq!(plan.from_ns, 0);
        assert_eq!(plan.interval_ns, 1_000_000_000);
    }

    #[test]
    fn response_time_histogram_bucket_count_clamps_to_at_least_one() {
        let plan = render_response_time_histogram_duckdb(0, 100, 0, "checkout");
        assert_eq!(plan.interval_ns, 100);
    }

    #[test]
    fn response_time_histogram_sql_uses_duckdb_arithmetic_and_quantile_cont() {
        let plan = render_response_time_histogram_duckdb(1000, 61000, 60, "checkout");
        assert!(
            plan.sql
                .contains("CAST((start_time_unix_nano - 1000) / 1000 AS BIGINT)")
        );
        assert!(!plan.sql.contains("intDiv"));
        assert!(plan.sql.contains("quantile_cont(duration_ns, 0.50)"));
        assert!(plan.sql.contains("quantile_cont(duration_ns, 0.95)"));
        assert!(!plan.sql.contains("quantile(0.50)"));
        assert!(plan.sql.contains("FROM spans"));
    }

    #[test]
    fn response_time_histogram_sql_filters_by_service_name() {
        let plan = render_response_time_histogram_duckdb(0, 1000, 10, "checkout");
        assert!(plan.sql.contains("service_name = 'checkout'"));
    }

    #[test]
    fn response_time_histogram_sql_escapes_service_name() {
        let plan = render_response_time_histogram_duckdb(0, 1000, 10, "o'brien");
        assert!(plan.sql.contains("service_name = 'o\\'brien'"));
    }
}
