//! Service summary query planning and row-shaping, extracted from the pure
//! (non-I/O) half of `services/query-api/src/discovery.rs::list_service_summaries`
//! (`service_summary_from_row` is moved verbatim; the SQL
//! aggregation itself is a *new* DuckDB dialect renderer for the
//! browser-local playground engine, mirroring `trace_query.rs`/`log_query.rs`).
//! Production keeps its existing ClickHouse-flavored SQL and Postgres-backed
//! catalog enrichment (`active_alert_count`, `latest_deployment`) unchanged —
//! the playground has no admin-service/Postgres locally, so those two fields
//! are always `0`/`None` here. See
//! `docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md`
//! section 10.

use crate::slo_evaluation::{
    EvalResult, SloBurnRateCondition, calculate_burn_rate, evaluate_burn_rate,
};
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

/// Burn-rate inputs for one service's seeded SLO (ADR-047 §3 sub-slice B):
/// bad/total event counts over the fast/slow windows, computed from the
/// playground's local `spans` table by the caller. `None` when a service
/// has no seeded SLO definition.
#[derive(serde::Deserialize)]
pub struct SloBurnRateRow {
    pub service_name: String,
    pub target: f64,
    pub window_days: i32,
    pub fast_window_minutes: u64,
    pub slow_window_minutes: u64,
    pub fast_threshold: f64,
    pub slow_threshold: f64,
    pub fast_bad_count: u64,
    pub fast_total_count: u64,
    pub slow_bad_count: u64,
    pub slow_total_count: u64,
}

#[derive(Serialize)]
pub struct ServiceSummary {
    pub service_name: String,
    pub request_rate: f64,
    pub error_rate: f64,
    pub p95_latency_ms: f64,
    /// Real burn-rate evaluation against a seeded local SLO when one exists
    /// for the service (see `service_summary_from_row`); `false` otherwise.
    pub slo_breaching: bool,
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

/// Adapted from `discovery.rs::service_summary_from_row`: `latest_deployment`
/// is always `None` (no catalog enrichment locally), but `slo_breaching`/
/// `active_alert_count` now run the real `calculate_burn_rate`/
/// `evaluate_burn_rate` logic (shared with `alert-evaluator`) against a
/// seeded local SLO, when one is provided (ADR-047 §3 sub-slice B).
/// `active_alert_count` has no local firing-state store to count against —
/// approximated as `1` while breaching, `0` otherwise.
pub fn service_summary_from_row(
    row: ServiceSummaryRow,
    duration_secs: f64,
    slo: Option<&SloBurnRateRow>,
) -> ServiceSummary {
    let error_rate = if row.request_count > 0 {
        (row.error_count as f64) / (row.request_count as f64)
    } else {
        0.0
    };

    let slo_breaching = slo.is_some_and(|s| {
        let fast_burn_rate = calculate_burn_rate(
            s.fast_bad_count,
            s.fast_total_count,
            s.target,
            s.fast_window_minutes,
            s.window_days,
        );
        let slow_burn_rate = calculate_burn_rate(
            s.slow_bad_count,
            s.slow_total_count,
            s.target,
            s.slow_window_minutes,
            s.window_days,
        );
        let condition = SloBurnRateCondition {
            slo_id: uuid::Uuid::nil(),
            fast_window_minutes: s.fast_window_minutes,
            slow_window_minutes: s.slow_window_minutes,
        };
        evaluate_burn_rate(
            fast_burn_rate,
            slow_burn_rate,
            s.fast_threshold,
            s.slow_threshold,
            &condition,
        ) == EvalResult::Firing
    });

    ServiceSummary {
        service_name: row.service_name,
        request_rate: (row.request_count as f64) / duration_secs,
        error_rate,
        p95_latency_ms: row.p95_latency_ns / 1_000_000.0,
        slo_breaching,
        active_alert_count: if slo_breaching { 1 } else { 0 },
        latest_deployment: None,
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
    fn summary_from_row_computes_rates_and_ms() {
        let row = ServiceSummaryRow {
            service_name: "checkout".into(),
            request_count: 200,
            error_count: 10,
            p95_latency_ns: 250_000_000.0,
        };
        let summary = service_summary_from_row(row, 100.0, None);
        assert_eq!(summary.service_name, "checkout");
        assert_eq!(summary.request_rate, 2.0);
        assert_eq!(summary.error_rate, 0.05);
        assert_eq!(summary.p95_latency_ms, 250.0);
        assert!(!summary.slo_breaching);
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
        let summary = service_summary_from_row(row, 100.0, None);
        assert_eq!(summary.error_rate, 0.0);
        assert!(!summary.slo_breaching);
    }

    fn slo_row(fast_bad: u64, fast_total: u64, slow_bad: u64, slow_total: u64) -> SloBurnRateRow {
        SloBurnRateRow {
            service_name: "checkout".into(),
            target: 0.999,
            window_days: 28,
            fast_window_minutes: 60,
            slow_window_minutes: 360,
            fast_threshold: 14.4,
            slow_threshold: 6.0,
            fast_bad_count: fast_bad,
            fast_total_count: fast_total,
            slow_bad_count: slow_bad,
            slow_total_count: slow_total,
        }
    }

    #[test]
    fn summary_marks_slo_breaching_when_both_windows_exceed_threshold() {
        let row = ServiceSummaryRow {
            service_name: "checkout".into(),
            request_count: 200,
            error_count: 10,
            p95_latency_ns: 250_000_000.0,
        };
        // 15/1,000 against a 0.999 target burns at 15x; above both thresholds (14.4/6.0).
        let slo = slo_row(15, 1_000, 15, 1_000);
        let summary = service_summary_from_row(row, 100.0, Some(&slo));
        assert!(summary.slo_breaching);
        assert_eq!(summary.active_alert_count, 1);
    }

    #[test]
    fn summary_does_not_mark_slo_breaching_when_within_budget() {
        let row = ServiceSummaryRow {
            service_name: "checkout".into(),
            request_count: 200,
            error_count: 10,
            p95_latency_ns: 250_000_000.0,
        };
        let slo = slo_row(0, 1_000, 0, 1_000);
        let summary = service_summary_from_row(row, 100.0, Some(&slo));
        assert!(!summary.slo_breaching);
        assert_eq!(summary.active_alert_count, 0);
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
