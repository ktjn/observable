//! wasm-bindgen boundary for the browser-local playground engine. Keeps
//! wrappers thin (JSON in/out, `JsValue` only at the very edge) so the real
//! logic in `query-core`/`domain-core` stays unit-testable natively —
//! `JsValue` cannot be constructed outside an actual wasm/JS runtime, so
//! native tests exercise the pure inner functions instead.

mod generator;

use domain_core::nlq::NlqIr;
use query_core::log_query::{
    extract_log_query_filters, render_log_histogram_duckdb, render_log_query_duckdb,
};
use query_core::metric_query::{render_metric_catalog_duckdb, render_metric_group_points_duckdb};
use query_core::service_query::{
    ServiceSummaryRow, render_service_summary_duckdb, service_summary_from_row,
};
use query_core::topology_query::{TopologyEdgeRow, render_topology_duckdb, topology_edge_from_row};
use query_core::trace_query::{
    extract_trace_query_filters, render_trace_histogram_duckdb, render_trace_query_duckdb,
};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn ping(seed: u32) -> String {
    format!("observable-playground-wasm:{seed}")
}

/// Plans a Traces table query (page-load / filter-pill shape — see
/// `docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md`
/// section 20) into DuckDB-flavored SQL against the playground's local
/// `spans` table. `ir_json` is a JSON-serialized `NlqIr`.
fn render_trace_search_sql_inner(ir_json: &str) -> Result<String, String> {
    let ir: NlqIr = serde_json::from_str(ir_json).map_err(|e| e.to_string())?;
    let filters = extract_trace_query_filters(&ir).map_err(|e| e.to_string())?;
    Ok(render_trace_query_duckdb(&filters))
}

#[wasm_bindgen]
pub fn render_trace_search_sql(ir_json: &str) -> Result<String, JsValue> {
    render_trace_search_sql_inner(ir_json).map_err(|e| JsValue::from_str(&e))
}

/// Generates a deterministic set of demo spans (see `generator.rs`) as a
/// JSON array. `now_unix_nano` is a decimal-string nanosecond epoch
/// timestamp (avoids JS `Number` precision loss for large integers).
fn generate_spans_json_inner(seed: u32, now_unix_nano: &str) -> Result<String, String> {
    let now: i64 = now_unix_nano
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    let spans = generator::generate_spans(seed, now);
    serde_json::to_string(&spans).map_err(|e| e.to_string())
}

#[wasm_bindgen]
pub fn generate_spans_json(seed: u32, now_unix_nano: &str) -> Result<String, JsValue> {
    generate_spans_json_inner(seed, now_unix_nano).map_err(|e| JsValue::from_str(&e))
}

#[derive(Serialize)]
struct TraceHistogramPlanJson {
    sql: String,
    /// Nanoseconds, as decimal strings — see `generate_spans_json`'s doc
    /// comment for why (JS `Number` precision loss).
    from_ns: String,
    interval_ns: String,
}

/// Plans a Traces histogram query (see
/// `docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md`
/// section 20) into DuckDB-flavored SQL against the playground's local
/// `spans` table. `from_ns`/`to_ns` are decimal-string nanosecond epoch
/// timestamps; `service` is optional. Returns JSON:
/// `{"sql", "from_ns", "interval_ns"}` — the caller fills in zero-count
/// buckets missing from the query result using `from_ns`/`interval_ns`,
/// mirroring production's handler.
fn render_trace_histogram_sql_inner(
    from_ns: &str,
    to_ns: &str,
    bucket_count: u32,
    service: Option<String>,
) -> Result<String, String> {
    let from_ns: u64 = from_ns
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    let to_ns: u64 = to_ns
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    let plan = render_trace_histogram_duckdb(from_ns, to_ns, bucket_count, service.as_deref());
    serde_json::to_string(&TraceHistogramPlanJson {
        sql: plan.sql,
        from_ns: plan.from_ns.to_string(),
        interval_ns: plan.interval_ns.to_string(),
    })
    .map_err(|e| e.to_string())
}

#[wasm_bindgen]
pub fn render_trace_histogram_sql(
    from_ns: &str,
    to_ns: &str,
    bucket_count: u32,
    service: Option<String>,
) -> Result<String, JsValue> {
    render_trace_histogram_sql_inner(from_ns, to_ns, bucket_count, service)
        .map_err(|e| JsValue::from_str(&e))
}

/// Plans a Logs table query into DuckDB-flavored SQL against the
/// playground's local `logs` table. `ir_json` is a JSON-serialized `NlqIr`.
fn render_log_search_sql_inner(ir_json: &str) -> Result<String, String> {
    let ir: NlqIr = serde_json::from_str(ir_json).map_err(|e| e.to_string())?;
    let filters = extract_log_query_filters(&ir).map_err(|e| e.to_string())?;
    Ok(render_log_query_duckdb(&filters))
}

#[wasm_bindgen]
pub fn render_log_search_sql(ir_json: &str) -> Result<String, JsValue> {
    render_log_search_sql_inner(ir_json).map_err(|e| JsValue::from_str(&e))
}

/// Generates one deterministic demo log record per generated span (see
/// `generator.rs::generate_logs`) as a JSON array. `now_unix_nano` is a
/// decimal-string nanosecond epoch timestamp.
fn generate_logs_json_inner(seed: u32, now_unix_nano: &str) -> Result<String, String> {
    let now: i64 = now_unix_nano
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    let logs = generator::generate_logs(seed, now);
    serde_json::to_string(&logs).map_err(|e| e.to_string())
}

#[wasm_bindgen]
pub fn generate_logs_json(seed: u32, now_unix_nano: &str) -> Result<String, JsValue> {
    generate_logs_json_inner(seed, now_unix_nano).map_err(|e| JsValue::from_str(&e))
}

#[derive(Serialize)]
struct LogHistogramPlanJson {
    sql: String,
    from_ns: String,
    interval_ns: String,
}

/// Plans a Logs histogram query into DuckDB-flavored SQL against the
/// playground's local `logs` table. `from_ns`/`to_ns` are decimal-string
/// nanosecond epoch timestamps; `service` is optional. Returns JSON:
/// `{"sql", "from_ns", "interval_ns"}` — the caller fills in zero-count
/// buckets missing from the query result, mirroring production's handler.
fn render_log_histogram_sql_inner(
    from_ns: &str,
    to_ns: &str,
    bucket_count: u32,
    service: Option<String>,
) -> Result<String, String> {
    let from_ns: u64 = from_ns
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    let to_ns: u64 = to_ns
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    let plan = render_log_histogram_duckdb(from_ns, to_ns, bucket_count, service.as_deref());
    serde_json::to_string(&LogHistogramPlanJson {
        sql: plan.sql,
        from_ns: plan.from_ns.to_string(),
        interval_ns: plan.interval_ns.to_string(),
    })
    .map_err(|e| e.to_string())
}

#[wasm_bindgen]
pub fn render_log_histogram_sql(
    from_ns: &str,
    to_ns: &str,
    bucket_count: u32,
    service: Option<String>,
) -> Result<String, JsValue> {
    render_log_histogram_sql_inner(from_ns, to_ns, bucket_count, service)
        .map_err(|e| JsValue::from_str(&e))
}

/// Plans a per-service summary aggregation query into DuckDB-flavored SQL
/// against the playground's local `spans` table. `from_ns`/`to_ns` are
/// decimal-string nanosecond epoch timestamps; `environment` is optional.
fn render_service_summary_sql_inner(
    from_ns: &str,
    to_ns: &str,
    environment: Option<String>,
) -> Result<String, String> {
    let from_ns: u64 = from_ns
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    let to_ns: u64 = to_ns
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    Ok(render_service_summary_duckdb(
        from_ns,
        to_ns,
        environment.as_deref(),
    ))
}

#[wasm_bindgen]
pub fn render_service_summary_sql(
    from_ns: &str,
    to_ns: &str,
    environment: Option<String>,
) -> Result<String, JsValue> {
    render_service_summary_sql_inner(from_ns, to_ns, environment).map_err(|e| JsValue::from_str(&e))
}

/// Shapes raw DuckDB aggregation rows (JSON array of `ServiceSummaryRow`)
/// into the frontend's `ServiceSummary` shape (rates, health state) — the
/// same pure computation `discovery.rs::list_service_summaries` runs after
/// its ClickHouse query, reused here verbatim via `service_summary_from_row`.
fn compute_service_summaries_inner(rows_json: &str, duration_secs: f64) -> Result<String, String> {
    let rows: Vec<ServiceSummaryRow> =
        serde_json::from_str(rows_json).map_err(|e| e.to_string())?;
    let summaries: Vec<_> = rows
        .into_iter()
        .map(|row| service_summary_from_row(row, duration_secs))
        .collect();
    serde_json::to_string(&summaries).map_err(|e| e.to_string())
}

#[wasm_bindgen]
pub fn compute_service_summaries(rows_json: &str, duration_secs: f64) -> Result<String, JsValue> {
    compute_service_summaries_inner(rows_json, duration_secs).map_err(|e| JsValue::from_str(&e))
}

/// Plans a service-topology join aggregation query into DuckDB-flavored SQL
/// against the playground's local `spans` table. `from_ns`/`to_ns` are
/// decimal-string nanosecond epoch timestamps; `environment`/`service` are
/// optional.
fn render_topology_sql_inner(
    from_ns: &str,
    to_ns: &str,
    environment: Option<String>,
    service: Option<String>,
) -> Result<String, String> {
    let from_ns: u64 = from_ns
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    let to_ns: u64 = to_ns
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    Ok(render_topology_duckdb(
        from_ns,
        to_ns,
        environment.as_deref(),
        service.as_deref(),
    ))
}

#[wasm_bindgen]
pub fn render_topology_sql(
    from_ns: &str,
    to_ns: &str,
    environment: Option<String>,
    service: Option<String>,
) -> Result<String, JsValue> {
    render_topology_sql_inner(from_ns, to_ns, environment, service)
        .map_err(|e| JsValue::from_str(&e))
}

/// Shapes raw DuckDB join-aggregation rows (JSON array of `TopologyEdgeRow`)
/// into the frontend's `TopologyEdge` shape (error rate, latency in ms).
fn compute_topology_edges_inner(rows_json: &str) -> Result<String, String> {
    let rows: Vec<TopologyEdgeRow> = serde_json::from_str(rows_json).map_err(|e| e.to_string())?;
    let edges: Vec<_> = rows.into_iter().map(topology_edge_from_row).collect();
    serde_json::to_string(&edges).map_err(|e| e.to_string())
}

#[wasm_bindgen]
pub fn compute_topology_edges(rows_json: &str) -> Result<String, JsValue> {
    compute_topology_edges_inner(rows_json).map_err(|e| JsValue::from_str(&e))
}

#[derive(Serialize)]
struct GeneratedMetricsJson {
    series: Vec<generator::GeneratedMetricSeries>,
    points: Vec<generator::GeneratedMetricPoint>,
}

/// Generates the playground's fixed demo metric catalog (see
/// `generator.rs::generate_metrics`) as JSON: `{"series": [...], "points": [...]}`.
/// `now_unix_nano` is a decimal-string nanosecond epoch timestamp.
fn generate_metrics_json_inner(seed: u32, now_unix_nano: &str) -> Result<String, String> {
    let now: i64 = now_unix_nano
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    let (series, points) = generator::generate_metrics(seed, now);
    serde_json::to_string(&GeneratedMetricsJson { series, points }).map_err(|e| e.to_string())
}

#[wasm_bindgen]
pub fn generate_metrics_json(seed: u32, now_unix_nano: &str) -> Result<String, JsValue> {
    generate_metrics_json_inner(seed, now_unix_nano).map_err(|e| JsValue::from_str(&e))
}

/// Plans the metric catalog aggregation query into DuckDB-flavored SQL
/// against the playground's local `metric_series` table. `service` is
/// optional.
#[wasm_bindgen]
pub fn render_metric_catalog_sql(service: Option<String>) -> String {
    render_metric_catalog_duckdb(service.as_deref())
}

/// Plans a metric group-points query into DuckDB-flavored SQL against the
/// playground's local `metric_points`/`metric_series` tables.
#[wasm_bindgen]
pub fn render_metric_group_points_sql(
    metric_name: &str,
    service: &str,
    environment: &str,
    metric_type: &str,
    unit: &str,
) -> String {
    render_metric_group_points_duckdb(metric_name, service, environment, metric_type, unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_includes_seed() {
        assert_eq!(ping(42), "observable-playground-wasm:42");
    }

    #[test]
    fn render_trace_search_sql_builds_duckdb_sql() {
        let ir_json = r#"{
            "operation": "table",
            "signals": ["traces"],
            "metric": null,
            "window": null,
            "filters": [{"field": "service_name", "op": "=", "value": "checkout"}],
            "group_by": [],
            "resolution": null,
            "time_range": {"from": "1700000000000000000", "to": "1700003600000000000"},
            "visualization_hint": null
        }"#;
        let sql = render_trace_search_sql_inner(ir_json).unwrap();
        assert!(sql.contains("FROM spans"));
        assert!(sql.contains("service_name = 'checkout'"));
    }

    #[test]
    fn render_trace_search_sql_rejects_invalid_json() {
        assert!(render_trace_search_sql_inner("not json").is_err());
    }

    #[test]
    fn render_trace_histogram_sql_builds_duckdb_sql() {
        let json =
            render_trace_histogram_sql_inner("0", "60000000000", 60, Some("checkout".into()))
                .unwrap();
        assert!(json.contains("\"sql\""));
        assert!(json.contains("service_name = 'checkout'"));
        assert!(json.contains("\"from_ns\":\"0\""));
        assert!(json.contains("\"interval_ns\":\"1000000000\""));
    }

    #[test]
    fn render_trace_histogram_sql_rejects_invalid_timestamps() {
        assert!(render_trace_histogram_sql_inner("not-a-number", "1000", 10, None).is_err());
    }

    #[test]
    fn render_log_search_sql_builds_duckdb_sql() {
        let ir_json = r#"{
            "operation": "table",
            "signals": ["logs"],
            "metric": null,
            "window": null,
            "filters": [{"field": "service_name", "op": "=", "value": "checkout"}],
            "group_by": [],
            "resolution": null,
            "time_range": {"from": "1700000000000000000", "to": "1700003600000000000"},
            "visualization_hint": null
        }"#;
        let sql = render_log_search_sql_inner(ir_json).unwrap();
        assert!(sql.contains("FROM logs"));
        assert!(sql.contains("service_name = 'checkout'"));
    }

    #[test]
    fn render_log_search_sql_rejects_invalid_json() {
        assert!(render_log_search_sql_inner("not json").is_err());
    }

    #[test]
    fn generate_logs_json_produces_one_log_per_span() {
        let spans_json = generate_spans_json_inner(7, "1700003600000000000").unwrap();
        let spans: Vec<serde_json::Value> = serde_json::from_str(&spans_json).unwrap();
        let logs_json = generate_logs_json_inner(7, "1700003600000000000").unwrap();
        let logs: Vec<serde_json::Value> = serde_json::from_str(&logs_json).unwrap();
        assert_eq!(spans.len(), logs.len());
    }

    #[test]
    fn generate_logs_json_rejects_invalid_timestamp() {
        assert!(generate_logs_json_inner(7, "not-a-number").is_err());
    }

    #[test]
    fn render_log_histogram_sql_builds_duckdb_sql() {
        let json = render_log_histogram_sql_inner("0", "60000000000", 60, Some("checkout".into()))
            .unwrap();
        assert!(json.contains("\"sql\""));
        assert!(json.contains("service_name = 'checkout'"));
        assert!(json.contains("\"from_ns\":\"0\""));
        assert!(json.contains("\"interval_ns\":\"1000000000\""));
    }

    #[test]
    fn render_log_histogram_sql_rejects_invalid_timestamps() {
        assert!(render_log_histogram_sql_inner("not-a-number", "1000", 10, None).is_err());
    }

    #[test]
    fn render_service_summary_sql_builds_duckdb_sql() {
        let sql = render_service_summary_sql_inner("0", "3600000000000", Some("production".into()))
            .unwrap();
        assert!(sql.contains("FROM spans"));
        assert!(sql.contains("environment = 'production'"));
    }

    #[test]
    fn render_service_summary_sql_rejects_invalid_timestamps() {
        assert!(render_service_summary_sql_inner("not-a-number", "1000", None).is_err());
    }

    #[test]
    fn compute_service_summaries_shapes_rows_into_summaries() {
        let rows_json = r#"[{"service_name":"checkout","request_count":200,"error_count":10,"p95_latency_ns":250000000.0}]"#;
        let json = compute_service_summaries_inner(rows_json, 100.0).unwrap();
        assert!(json.contains("\"service_name\":\"checkout\""));
        assert!(json.contains("\"health_state\":\"watch\""));
        assert!(json.contains("\"request_rate\":2.0"));
    }

    #[test]
    fn compute_service_summaries_rejects_invalid_json() {
        assert!(compute_service_summaries_inner("not json", 100.0).is_err());
    }

    #[test]
    fn render_topology_sql_builds_duckdb_sql() {
        let sql =
            render_topology_sql_inner("0", "3600000000000", None, Some("checkout".into())).unwrap();
        assert!(sql.contains("FROM spans AS child, spans AS parent"));
        assert!(sql.contains("checkout"));
    }

    #[test]
    fn render_topology_sql_rejects_invalid_timestamps() {
        assert!(render_topology_sql_inner("not-a-number", "1000", None, None).is_err());
    }

    #[test]
    fn compute_topology_edges_shapes_rows() {
        let rows_json = r#"[{"caller":"web","callee":"api-gateway","request_count":40,"error_count":4,"p95_latency_ns":66000000.0}]"#;
        let json = compute_topology_edges_inner(rows_json).unwrap();
        assert!(json.contains("\"caller\":\"web\""));
        assert!(json.contains("\"error_rate\":0.1"));
        assert!(json.contains("\"p95_latency_ms\":66.0"));
    }

    #[test]
    fn compute_topology_edges_rejects_invalid_json() {
        assert!(compute_topology_edges_inner("not json").is_err());
    }

    #[test]
    fn generate_metrics_json_produces_series_and_points() {
        let json = generate_metrics_json_inner(7, "1700003600000000000").unwrap();
        assert!(json.contains("\"series\""));
        assert!(json.contains("\"points\""));
        assert!(json.contains("http.server.duration"));
    }

    #[test]
    fn generate_metrics_json_rejects_invalid_timestamp() {
        assert!(generate_metrics_json_inner(7, "not-a-number").is_err());
    }

    #[test]
    fn render_metric_catalog_sql_builds_duckdb_sql() {
        let sql = render_metric_catalog_sql(Some("checkout".into()));
        assert!(sql.contains("FROM metric_series"));
        assert!(sql.contains("service_name = 'checkout'"));
    }

    #[test]
    fn render_metric_group_points_sql_builds_duckdb_sql() {
        let sql = render_metric_group_points_sql(
            "http.server.duration",
            "checkout",
            "production",
            "gauge",
            "ms",
        );
        assert!(sql.contains("FROM metric_points"));
        assert!(sql.contains("avg(mp.value_double)"));
    }
}
