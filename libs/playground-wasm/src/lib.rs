//! wasm-bindgen boundary for the browser-local playground engine. Keeps
//! wrappers thin (JSON in/out, `JsValue` only at the very edge) so the real
//! logic in `query-core`/`domain-core` stays unit-testable natively —
//! `JsValue` cannot be constructed outside an actual wasm/JS runtime, so
//! native tests exercise the pure inner functions instead.

mod generator;

use domain_core::nlq::NlqIr;
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
}
