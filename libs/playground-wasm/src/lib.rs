//! wasm-bindgen boundary for the browser-local playground engine. Keeps
//! wrappers thin (JSON in/out, `JsValue` only at the very edge) so the real
//! logic in `query-core`/`domain-core` stays unit-testable natively —
//! `JsValue` cannot be constructed outside an actual wasm/JS runtime, so
//! native tests exercise the pure inner functions instead.

use domain_core::nlq::NlqIr;
use query_core::trace_query::{extract_trace_query_filters, render_trace_query_duckdb};
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
}
