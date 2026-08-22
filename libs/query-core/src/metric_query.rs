//! Metric catalog and group-points query planning for the browser-local
//! playground, mirroring `service_query.rs`/`topology_query.rs`'s pattern.
//! Production's `services/query-api/src/metrics.rs::list_metrics`/
//! `get_metric_group_points` run ClickHouse SQL against `metric_series`/
//! `metric_points` and are untouched here; these are new DuckDB dialect
//! renderers for the playground's local tables of the same name (schema
//! simplified — see `engineWorker.ts`).

use crate::sql_templates::escape_string_value;

/// Renders a DuckDB-flavored per-metric catalog aggregation against the
/// playground's local `metric_series` table, mirroring `list_metrics`'s
/// `GROUP BY` shape. Each playground series is unique per
/// `(metric_name, service_name)` (see `generator.rs::generate_metrics`), so
/// `count(DISTINCT metric_series_id)` is always `1` here, same as
/// production's `countDistinct(metric_series_id)`.
pub fn render_metric_catalog_duckdb(service: Option<&str>) -> String {
    let where_sql = match service {
        Some(svc) => format!("WHERE service_name = '{}'", escape_string_value(svc)),
        None => String::new(),
    };

    format!(
        "SELECT \
           metric_name, \
           any_value(description) AS description, \
           unit, \
           metric_type, \
           is_monotonic, \
           aggregation_temporality, \
           service_name, \
           environment, \
           count(DISTINCT metric_series_id) AS series_count \
         FROM metric_series \
         {where_sql} \
         GROUP BY metric_name, unit, metric_type, is_monotonic, aggregation_temporality, service_name, environment \
         ORDER BY series_count DESC, metric_name ASC"
    )
}

/// Renders a DuckDB-flavored time-series points query against the
/// playground's local `metric_points`/`metric_series` tables, mirroring
/// `get_metric_group_points`'s join/aggregation shape (`avg` for gauges,
/// `sum` otherwise).
pub fn render_metric_group_points_duckdb(
    metric_name: &str,
    service: &str,
    environment: &str,
    metric_type: &str,
    unit: &str,
) -> String {
    let metric_name = escape_string_value(metric_name);
    let service = escape_string_value(service);
    let environment = escape_string_value(environment);
    let metric_type = escape_string_value(metric_type);
    let unit = escape_string_value(unit);

    let agg = if metric_type == "gauge" { "avg" } else { "sum" };

    format!(
        "SELECT \
           mp.metric_series_id AS metric_series_id, \
           ms.metric_name AS metric_name, \
           ms.service_name AS service_name, \
           mp.time_unix_nano AS time_unix_nano, \
           mp.start_time_unix_nano AS start_time_unix_nano, \
           {agg}(mp.value_double) AS value_double \
         FROM metric_points mp \
         INNER JOIN metric_series ms ON ms.metric_series_id = mp.metric_series_id \
         WHERE ms.metric_name = '{metric_name}' \
           AND ms.service_name = '{service}' \
           AND ms.environment = '{environment}' \
           AND ms.metric_type = '{metric_type}' \
           AND ms.unit = '{unit}' \
         GROUP BY mp.metric_series_id, ms.metric_name, ms.service_name, mp.time_unix_nano, mp.start_time_unix_nano \
         ORDER BY mp.time_unix_nano ASC"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_render_includes_group_by_and_order_by() {
        let sql = render_metric_catalog_duckdb(None);
        assert!(sql.contains("FROM metric_series"));
        assert!(sql.contains("GROUP BY metric_name"));
        assert!(sql.contains("ORDER BY series_count DESC, metric_name ASC"));
        assert!(!sql.contains("WHERE"));
    }

    #[test]
    fn catalog_render_includes_service_filter_when_present() {
        let sql = render_metric_catalog_duckdb(Some("checkout"));
        assert!(sql.contains("WHERE service_name = 'checkout'"));
    }

    #[test]
    fn catalog_render_escapes_service_value() {
        let sql = render_metric_catalog_duckdb(Some("o'brien"));
        assert!(sql.contains("service_name = 'o\\'brien'"));
    }

    #[test]
    fn points_render_uses_avg_for_gauge() {
        let sql = render_metric_group_points_duckdb(
            "http.server.duration",
            "checkout",
            "production",
            "gauge",
            "ms",
        );
        assert!(sql.contains("avg(mp.value_double)"));
        assert!(!sql.contains("sum(mp.value_double)"));
    }

    #[test]
    fn points_render_uses_sum_for_non_gauge() {
        let sql = render_metric_group_points_duckdb(
            "http.server.request_count",
            "checkout",
            "production",
            "sum",
            "1",
        );
        assert!(sql.contains("sum(mp.value_double)"));
        assert!(!sql.contains("avg(mp.value_double)"));
    }

    #[test]
    fn points_render_includes_all_filters() {
        let sql = render_metric_group_points_duckdb(
            "http.server.duration",
            "checkout",
            "production",
            "gauge",
            "ms",
        );
        assert!(sql.contains("ms.metric_name = 'http.server.duration'"));
        assert!(sql.contains("ms.service_name = 'checkout'"));
        assert!(sql.contains("ms.environment = 'production'"));
        assert!(sql.contains("ms.metric_type = 'gauge'"));
        assert!(sql.contains("ms.unit = 'ms'"));
        assert!(sql.contains("ORDER BY mp.time_unix_nano ASC"));
    }

    #[test]
    fn points_render_escapes_filter_values() {
        let sql = render_metric_group_points_duckdb("m", "o'brien", "prod", "gauge", "1");
        assert!(sql.contains("ms.service_name = 'o\\'brien'"));
    }
}
