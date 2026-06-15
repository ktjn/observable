// NLQ IR (Intermediate Representation) — the structured object the LLM emits.
// The MCP server translates this IR into SQL/DataFusion plans.
// See ADR-021: LLM Natural Language Query Layer.
//
// Design invariants:
//  - The LLM emits IR, never SQL. SQL generation lives in the MCP server.
//  - The IR is stable and versioned independently of the LLM and SQL dialect.
//  - All MCP-generated SQL must carry the caller's tenant context.
//  - Every response must carry 6 provenance fields (see VisualizationFrame, Step 4).
use serde::{Deserialize, Serialize};

/// Top-level NLQ intermediate representation.
///
/// Mirrors `nlq.NlqIr` (value) in `models/nlq.mdl` field-for-field, except
/// `metric`/`window`/`resolution`/`visualization_hint` (Phase 1 backlog item
/// 8: `Option<T>` without `skip_serializing_if` can't be generated) and
/// `signals` (Phase 1 backlog item 9: `array<enum(...))` emits invalid
/// TypeScript, modeled as `array<string>`) — see
/// `docs/superpowers/specs/2026-06-15-nlq-visualization-modelable-migration-design.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NlqIr {
    /// Query operation type, e.g. timeseries, rate, histogram.
    pub operation: NlqOperation,
    /// Signal types to query (metrics, traces, logs).
    pub signals: Vec<NlqSignal>,
    /// Metric name to query (required for metric operations).
    pub metric: Option<String>,
    /// Lookback window for range-vector operations, e.g. "5m", "1h".
    pub window: Option<String>,
    /// Field-level filters.
    #[serde(default)]
    pub filters: Vec<NlqFilter>,
    /// Group-by label columns.
    #[serde(default)]
    pub group_by: Vec<String>,
    /// Time-bucket resolution, e.g. "1m", "5m".
    pub resolution: Option<String>,
    /// Query time range.
    pub time_range: NlqTimeRange,
    /// Preferred visualization type; the UI selects the panel type from this.
    pub visualization_hint: Option<NlqVisualizationHint>,
    /// For distribution operations: which stats to compute. Each entry is one of:
    /// - `"p{N}"` where N is 1–999 (e.g. `"p50"`, `"p75"`, `"p99"`, `"p999"`)
    /// - `"median"` (alias for p50)
    /// - `"average"` or `"mean"` (arithmetic mean)
    /// - `"min"`, `"max"`
    ///
    /// When absent or empty the SQL template defaults to p50/p90/p95/p99/min/max.
    /// Unrecognised entries are silently ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percentiles: Option<Vec<String>>,
    /// For catalog operations: the dimension to enumerate, e.g. "service_name", "pod", "metric_name".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_field: Option<String>,
    /// For topk operations: how many top results to return. Defaults to 10 if absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Free-text search term for log queries. Applied as substring match on the log `body` column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
}

/// Supported query operation types.
///
/// Mirrors the inline `enum(...)` used for `nlq.NlqIr.operation` in
/// `models/nlq.mdl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NlqOperation {
    /// Time-series line chart (gauge metrics, spans over time, log rates).
    Timeseries,
    /// Per-window rate of a monotonic counter (reset-aware).
    Rate,
    /// Instantaneous rate using only the two most-recent samples.
    Irate,
    /// Monotonic counter increase over the window.
    Increase,
    /// Histogram / latency distribution (explicit `le` bucket expansion).
    Histogram,
    /// Top-K series by value.
    Topk,
    /// Flat tabular result.
    Table,
    /// Empirical value distribution (width_bucket / percentile).
    Distribution,
    /// Catalog of observable entities (distinct-values query on series metadata).
    Catalog,
    /// Entity inventory filter — filter a predefined entity table (infrastructure, services)
    /// by attribute predicates (environment, entity_type, health_state, service_name, text).
    /// No metric required. Backend executes the infrastructure ClickHouse SQL with the
    /// IR-derived filters and returns a VisualizationFrame(table).
    Inventory,
}

/// Signal types Observable can query.
///
/// Mirrors `nlq.NlqIr.signals: array<string>` in `models/nlq.mdl` (Phase 1
/// backlog item 9 — not modeled as `array<enum(...))`, kept as a real Rust
/// enum here).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NlqSignal {
    Metrics,
    Traces,
    Logs,
}

/// A single field-level filter predicate.
///
/// Mirrors `nlq.NlqFilter` (value) in `models/nlq.mdl`, except `op` is
/// `string` not `enum(...)` (Phase 1 backlog item 6 — comparison-symbol
/// variants aren't valid modelable identifiers).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NlqFilter {
    /// Column or attribute name to filter on.
    pub field: String,
    /// Comparison operator.
    pub op: NlqFilterOp,
    /// Filter value (string for simplicity; MCP server casts appropriately).
    pub value: String,
}

/// Filter comparison operators (aligned with PromQL label matchers + SQL comparisons).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NlqFilterOp {
    /// Exact equality (`=`).
    #[serde(rename = "=")]
    Eq,
    /// Inequality (`!=`).
    #[serde(rename = "!=")]
    Ne,
    /// Regex match (`=~`).
    #[serde(rename = "=~")]
    Re,
    /// Regex non-match (`!~`).
    #[serde(rename = "!~")]
    Nre,
    /// Greater-than (`>`).
    #[serde(rename = ">")]
    Gt,
    /// Greater-than-or-equal (`>=`).
    #[serde(rename = ">=")]
    Gte,
    /// Less-than (`<`).
    #[serde(rename = "<")]
    Lt,
    /// Less-than-or-equal (`<=`).
    #[serde(rename = "<=")]
    Lte,
}

/// Query time range. Both `from` and `to` accept:
/// - Relative expressions: `"now"`, `"now-1h"`, `"now-30m"`, `"now-1d"`, `"now-30s"`
/// - Unix nanosecond integer strings: `"1746274719123000000"` (ADR-030)
///
/// ISO-8601 strings are explicitly rejected. Callers must convert to Unix nanoseconds before
/// including in the IR (the frontend uses `String(BigInt(Math.floor(ms)) * 1_000_000n)`).
///
/// Mirrors `nlq.NlqTimeRange` (value) in `models/nlq.mdl` field-for-field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NlqTimeRange {
    /// Start of the range, e.g. `"now-1h"` or `"1746274719123000000"` (Unix nanoseconds).
    pub from: String,
    /// End of the range, e.g. `"now"` or `"1746274732456000000"` (Unix nanoseconds).
    pub to: String,
}

/// Visualization hint returned by the LLM and honoured by the UI's auto-graphing layer.
/// Maps to Grafana panel types (see ADR-016).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NlqVisualizationHint {
    Timeseries,
    Histogram,
    Heatmap,
    Table,
    Topk,
    Flamegraph,
    Distribution,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_ir_json() -> &'static str {
        r#"{
            "operation": "timeseries",
            "signals": ["metrics"],
            "metric": "request_duration_ms",
            "window": null,
            "filters": [],
            "group_by": [],
            "resolution": "1m",
            "time_range": {"from": "now-1h", "to": "now"},
            "visualization_hint": "timeseries"
        }"#
    }

    fn full_ir_json() -> &'static str {
        r#"{
            "operation": "rate",
            "signals": ["metrics"],
            "metric": "http_requests_total",
            "window": "5m",
            "filters": [
                {"field": "method", "op": "=", "value": "GET"},
                {"field": "status", "op": "!=", "value": "500"},
                {"field": "path", "op": "=~", "value": "/api/.*"}
            ],
            "group_by": ["pod", "region"],
            "resolution": "1m",
            "time_range": {"from": "now-3h", "to": "now"},
            "visualization_hint": "timeseries"
        }"#
    }

    #[test]
    fn deserialize_minimal_ir() {
        let ir: NlqIr = serde_json::from_str(minimal_ir_json()).expect("deserialization failed");
        assert_eq!(ir.operation, NlqOperation::Timeseries);
        assert_eq!(ir.signals, vec![NlqSignal::Metrics]);
        assert_eq!(ir.metric.as_deref(), Some("request_duration_ms"));
        assert!(ir.filters.is_empty());
        assert!(ir.group_by.is_empty());
        assert_eq!(ir.resolution.as_deref(), Some("1m"));
        assert_eq!(ir.time_range.from, "now-1h");
        assert_eq!(ir.time_range.to, "now");
        assert_eq!(
            ir.visualization_hint,
            Some(NlqVisualizationHint::Timeseries)
        );
    }

    #[test]
    fn deserialize_full_ir_with_filters() {
        let ir: NlqIr = serde_json::from_str(full_ir_json()).expect("deserialization failed");
        assert_eq!(ir.operation, NlqOperation::Rate);
        assert_eq!(ir.window.as_deref(), Some("5m"));
        assert_eq!(ir.filters.len(), 3);
        assert_eq!(ir.filters[0].op, NlqFilterOp::Eq);
        assert_eq!(ir.filters[1].op, NlqFilterOp::Ne);
        assert_eq!(ir.filters[2].op, NlqFilterOp::Re);
        assert_eq!(ir.group_by, vec!["pod", "region"]);
    }

    #[test]
    fn roundtrip_serialize_deserialize() {
        let original: NlqIr = serde_json::from_str(full_ir_json()).unwrap();
        let json = serde_json::to_string(&original).unwrap();
        let roundtripped: NlqIr = serde_json::from_str(&json).unwrap();
        assert_eq!(original, roundtripped);
    }

    #[test]
    fn deserialize_histogram_operation() {
        let json = r#"{
            "operation": "histogram",
            "signals": ["metrics"],
            "metric": "request_duration_ms",
            "window": "5m",
            "filters": [],
            "group_by": [],
            "resolution": null,
            "time_range": {"from": "now-30m", "to": "now"},
            "visualization_hint": "histogram"
        }"#;
        let ir: NlqIr = serde_json::from_str(json).unwrap();
        assert_eq!(ir.operation, NlqOperation::Histogram);
        assert_eq!(ir.visualization_hint, Some(NlqVisualizationHint::Histogram));
    }

    #[test]
    fn deserialize_topk_operation() {
        let json = r#"{
            "operation": "topk",
            "signals": ["metrics"],
            "metric": "cpu_usage",
            "window": null,
            "filters": [],
            "group_by": ["service"],
            "resolution": null,
            "time_range": {"from": "now-1h", "to": "now"},
            "visualization_hint": "topk"
        }"#;
        let ir: NlqIr = serde_json::from_str(json).unwrap();
        assert_eq!(ir.operation, NlqOperation::Topk);
        assert_eq!(ir.group_by, vec!["service"]);
    }

    #[test]
    fn deserialize_multi_signal_ir() {
        let json = r#"{
            "operation": "table",
            "signals": ["metrics", "traces", "logs"],
            "metric": null,
            "window": null,
            "filters": [{"field": "env", "op": "=", "value": "prod"}],
            "group_by": [],
            "resolution": null,
            "time_range": {"from": "now-15m", "to": "now"},
            "visualization_hint": "table"
        }"#;
        let ir: NlqIr = serde_json::from_str(json).unwrap();
        assert_eq!(ir.signals.len(), 3);
        assert!(ir.signals.contains(&NlqSignal::Metrics));
        assert!(ir.signals.contains(&NlqSignal::Traces));
        assert!(ir.signals.contains(&NlqSignal::Logs));
    }

    #[test]
    fn deserialize_irate_operation() {
        let json = r#"{
            "operation": "irate",
            "signals": ["metrics"],
            "metric": "http_requests_total",
            "window": "1m",
            "filters": [],
            "group_by": [],
            "resolution": "30s",
            "time_range": {"from": "now-5m", "to": "now"},
            "visualization_hint": null
        }"#;
        let ir: NlqIr = serde_json::from_str(json).unwrap();
        assert_eq!(ir.operation, NlqOperation::Irate);
        assert_eq!(ir.window.as_deref(), Some("1m"));
        assert!(ir.visualization_hint.is_none());
    }

    #[test]
    fn deserialize_filters_with_all_ops() {
        let json = r#"{
            "operation": "table",
            "signals": ["metrics"],
            "metric": null,
            "window": null,
            "filters": [
                {"field": "a", "op": "=",  "value": "1"},
                {"field": "b", "op": "!=", "value": "2"},
                {"field": "c", "op": "=~", "value": "x.*"},
                {"field": "d", "op": "!~", "value": "y.*"},
                {"field": "e", "op": ">",  "value": "3"},
                {"field": "f", "op": ">=", "value": "4"},
                {"field": "g", "op": "<",  "value": "5"},
                {"field": "h", "op": "<=", "value": "6"}
            ],
            "group_by": [],
            "resolution": null,
            "time_range": {"from": "now-1h", "to": "now"},
            "visualization_hint": null
        }"#;
        let ir: NlqIr = serde_json::from_str(json).unwrap();
        assert_eq!(ir.filters[0].op, NlqFilterOp::Eq);
        assert_eq!(ir.filters[1].op, NlqFilterOp::Ne);
        assert_eq!(ir.filters[2].op, NlqFilterOp::Re);
        assert_eq!(ir.filters[3].op, NlqFilterOp::Nre);
        assert_eq!(ir.filters[4].op, NlqFilterOp::Gt);
        assert_eq!(ir.filters[5].op, NlqFilterOp::Gte);
        assert_eq!(ir.filters[6].op, NlqFilterOp::Lt);
        assert_eq!(ir.filters[7].op, NlqFilterOp::Lte);
    }

    #[test]
    fn serialize_filter_ops_as_symbols() {
        let filter = NlqFilter {
            field: "x".into(),
            op: NlqFilterOp::Re,
            value: ".*".into(),
        };
        let json = serde_json::to_string(&filter).unwrap();
        assert!(
            json.contains(r#""=~""#),
            "op must serialize as the symbol string, got: {json}"
        );
    }

    #[test]
    fn filters_default_empty_when_omitted() {
        let json = r#"{
            "operation": "timeseries",
            "signals": ["metrics"],
            "time_range": {"from": "now-1h", "to": "now"}
        }"#;
        let ir: NlqIr = serde_json::from_str(json).unwrap();
        assert!(ir.filters.is_empty());
        assert!(ir.group_by.is_empty());
    }

    #[test]
    fn percentiles_absent_deserializes_to_none() {
        let json = r#"{
            "operation": "distribution",
            "signals": ["metrics"],
            "metric": "request_duration_ms",
            "time_range": {"from": "now-1h", "to": "now"}
        }"#;
        let ir: NlqIr = serde_json::from_str(json).unwrap();
        assert!(ir.percentiles.is_none());
    }

    #[test]
    fn percentiles_single_value_roundtrips() {
        let json = r#"{
            "operation": "distribution",
            "signals": ["metrics"],
            "metric": "request_duration_ms",
            "time_range": {"from": "now-1h", "to": "now"},
            "percentiles": ["p99"]
        }"#;
        let ir: NlqIr = serde_json::from_str(json).unwrap();
        assert_eq!(ir.percentiles, Some(vec!["p99".to_string()]));
        let out = serde_json::to_string(&ir).unwrap();
        assert!(out.contains(r#""percentiles":["p99"]"#));
    }

    #[test]
    fn percentiles_multi_value_roundtrips() {
        let json = r#"{
            "operation": "distribution",
            "signals": ["metrics"],
            "metric": "request_duration_ms",
            "time_range": {"from": "now-1h", "to": "now"},
            "percentiles": ["p75", "p95", "p99", "average", "median"]
        }"#;
        let ir: NlqIr = serde_json::from_str(json).unwrap();
        let pcts = ir.percentiles.as_ref().unwrap();
        assert_eq!(pcts.len(), 5);
        assert!(pcts.contains(&"p75".to_string()));
        assert!(pcts.contains(&"median".to_string()));
    }

    #[test]
    fn percentiles_none_does_not_serialize() {
        let json = r#"{
            "operation": "distribution",
            "signals": ["metrics"],
            "metric": "request_duration_ms",
            "time_range": {"from": "now-1h", "to": "now"}
        }"#;
        let ir: NlqIr = serde_json::from_str(json).unwrap();
        let out = serde_json::to_string(&ir).unwrap();
        assert!(
            !out.contains("percentiles"),
            "field must be absent when None: {out}"
        );
    }

    // ── catalog_field ─────────────────────────────────────────────────────────

    #[test]
    fn catalog_operation_with_field_roundtrips() {
        let json = r#"{
            "operation": "catalog",
            "signals": ["metrics"],
            "time_range": {"from": "now-24h", "to": "now"},
            "catalog_field": "service_name"
        }"#;
        let ir: NlqIr = serde_json::from_str(json).expect("deserialization failed");
        assert_eq!(ir.operation, NlqOperation::Catalog);
        assert_eq!(ir.catalog_field.as_deref(), Some("service_name"));
        let out = serde_json::to_string(&ir).unwrap();
        assert!(
            out.contains(r#""catalog_field":"service_name""#),
            "catalog_field must be present in serialized JSON: {out}"
        );
    }

    #[test]
    fn catalog_operation_without_field_defaults_to_none() {
        let json = r#"{
            "operation": "catalog",
            "signals": ["metrics"],
            "time_range": {"from": "now-24h", "to": "now"}
        }"#;
        let ir: NlqIr = serde_json::from_str(json).expect("deserialization failed");
        assert_eq!(ir.operation, NlqOperation::Catalog);
        assert!(
            ir.catalog_field.is_none(),
            "catalog_field must default to None when omitted"
        );
    }

    #[test]
    fn catalog_field_absent_does_not_serialize() {
        let json = r#"{
            "operation": "catalog",
            "signals": ["metrics"],
            "time_range": {"from": "now-24h", "to": "now"}
        }"#;
        let ir: NlqIr = serde_json::from_str(json).unwrap();
        assert!(ir.catalog_field.is_none());
        let out = serde_json::to_string(&ir).unwrap();
        assert!(
            !out.contains("catalog_field"),
            "catalog_field must be absent when None: {out}"
        );
    }

    // ── inventory ──────────────────────────────────────────────────────────────

    #[test]
    fn inventory_operation_with_filters_roundtrips() {
        let json = r#"{
            "operation": "inventory",
            "signals": [],
            "time_range": {"from": "now-1h", "to": "now"},
            "filters": [
                {"field": "environment", "op": "=", "value": "observable"},
                {"field": "entity_type", "op": "=", "value": "pod"}
            ]
        }"#;
        let ir: NlqIr = serde_json::from_str(json).expect("deserialization failed");
        assert_eq!(ir.operation, NlqOperation::Inventory);
        assert_eq!(ir.filters.len(), 2);
        assert!(ir.metric.is_none(), "inventory requires no metric");
        let out = serde_json::to_string(&ir).unwrap();
        assert!(
            out.contains(r#""operation":"inventory""#),
            "operation must round-trip: {out}"
        );
    }

    #[test]
    fn inventory_operation_no_filters_roundtrips() {
        let json = r#"{
            "operation": "inventory",
            "signals": [],
            "time_range": {"from": "now-1h", "to": "now"}
        }"#;
        let ir: NlqIr = serde_json::from_str(json).expect("deserialization failed");
        assert_eq!(ir.operation, NlqOperation::Inventory);
        assert!(ir.filters.is_empty());
    }
}
