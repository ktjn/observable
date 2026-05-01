// VisualizationFrame — the typed, self-describing response from the MCP server.
//
// Every VisualizationFrame carries:
//   1. Visualization contract: frame_type, field mappings, field roles, data rows.
//   2. Provenance payload (all 6 fields required by ADR-021):
//      nlq_ir, source_sql, time_range, signal_types, sample_rate, approximation_statement.
//
// The frame maps directly to Grafana's DataFrame + PanelData model (ADR-016).
// The UI auto-selects the correct Grafana panel type from `suggested_visualization`
// without requiring the user to choose — this is the auto-graphing contract.
//
// `approximation_statement` and `nlq_ir` are always present. Neither the UI nor any
// automated system may omit the provenance payload when displaying or forwarding results.
use crate::nlq::{NlqIr, NlqSignal, NlqTimeRange};
use serde::{Deserialize, Serialize};

// ── VisualizationFrame ────────────────────────────────────────────────────────

/// The full response from the MCP server: query result + visualization hints + provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualizationFrame {
    // ── Visualization contract ────────────────────────────────────────────────
    /// Chart/table type the UI should render.
    pub frame_type: VisualizationFrameType,
    /// Column name to use as the x-axis (e.g. "bucket", "ts").
    pub x_field: Option<String>,
    /// Column name to use as the y-axis (e.g. "value", "rate").
    pub y_field: Option<String>,
    /// Column name to use for series grouping (e.g. "service_name").
    pub series_field: Option<String>,
    /// Unit label for the y-axis values (e.g. "ms", "req/s", "bytes").
    pub unit: Option<String>,
    /// Grafana panel type recommendation (e.g. "timeseries", "barchart", "table").
    /// The UI auto-selects this panel without user interaction.
    pub suggested_visualization: String,
    /// Semantic roles for columns that need disambiguation (e.g. histogram le buckets).
    #[serde(default)]
    pub field_roles: Vec<FieldRole>,
    /// Query result rows, each as a JSON object.
    pub data: Vec<serde_json::Value>,

    // ── Provenance payload (all 6 fields required by ADR-021) ─────────────────
    /// The NLQ IR that produced this frame. Enables reproducibility and debugging.
    pub nlq_ir: NlqIr,
    /// Verbatim SQL that was executed. Displayed alongside results for transparency.
    pub source_sql: String,
    /// The time range that was queried. Displayed in the UI.
    pub time_range: NlqTimeRange,
    /// Signal types consulted to produce this frame.
    pub signal_types: Vec<NlqSignal>,
    /// Effective sampling rate for the primary signal, if known from Schema Registry.
    /// None when the sample rate is unknown (treat result as unsampled).
    pub sample_rate: Option<f64>,
    /// Plain-language statement describing the approximation bounds of this result.
    /// Must always be non-empty. The UI displays this alongside the result.
    pub approximation_statement: String,
}

// ── VisualizationFrameType ────────────────────────────────────────────────────

/// Chart or table type the UI should render. Maps to Grafana panel types (ADR-016).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualizationFrameType {
    /// Time-series line chart.
    Timeseries,
    /// Histogram bar chart (latency distribution, etc.).
    Histogram,
    /// Heatmap (e.g. histogram over time).
    Heatmap,
    /// Flat tabular result.
    Table,
    /// Top-K ranked list.
    Topk,
    /// Flame graph (trace/profile).
    Flamegraph,
    /// Value distribution (percentile summary).
    Distribution,
}

/// Maps `NlqVisualizationHint` to a `VisualizationFrameType`.
impl From<crate::nlq::NlqVisualizationHint> for VisualizationFrameType {
    fn from(hint: crate::nlq::NlqVisualizationHint) -> Self {
        use crate::nlq::NlqVisualizationHint as H;
        match hint {
            H::Timeseries => Self::Timeseries,
            H::Histogram => Self::Histogram,
            H::Heatmap => Self::Heatmap,
            H::Table => Self::Table,
            H::Topk => Self::Topk,
            H::Flamegraph => Self::Flamegraph,
            H::Distribution => Self::Distribution,
        }
    }
}

// ── FieldRole ─────────────────────────────────────────────────────────────────

/// Describes the semantic role of a column in the result set.
/// Required for columns that the UI cannot infer from name alone (e.g. histogram buckets).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldRole {
    /// Column name in the result set.
    pub name: String,
    /// Semantic role.
    pub role: FieldRoleKind,
}

/// Semantic role kinds for result columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldRoleKind {
    /// Timestamp / time-bucket column (x-axis).
    Time,
    /// Numeric metric value (y-axis).
    Value,
    /// Histogram bucket upper-bound (le boundary).
    Bucket,
    /// Series identifier / grouping dimension.
    Series,
    /// Human-readable label column.
    Label,
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nlq::{
        NlqFilter, NlqFilterOp, NlqIr, NlqOperation, NlqSignal, NlqTimeRange, NlqVisualizationHint,
    };

    fn sample_ir() -> NlqIr {
        NlqIr {
            operation: NlqOperation::Timeseries,
            signals: vec![NlqSignal::Metrics],
            metric: Some("request_duration_ms".into()),
            window: None,
            filters: vec![],
            group_by: vec![],
            resolution: Some("1m".into()),
            time_range: NlqTimeRange {
                from: "now-1h".into(),
                to: "now".into(),
            },
            visualization_hint: Some(NlqVisualizationHint::Timeseries),
            percentiles: None,
            catalog_field: None,
            limit: None,
            query: None,
        }
    }

    fn sample_frame() -> VisualizationFrame {
        VisualizationFrame {
            frame_type: VisualizationFrameType::Timeseries,
            x_field: Some("bucket".into()),
            y_field: Some("value".into()),
            series_field: None,
            unit: Some("ms".into()),
            suggested_visualization: "timeseries".into(),
            field_roles: vec![
                FieldRole {
                    name: "bucket".into(),
                    role: FieldRoleKind::Time,
                },
                FieldRole {
                    name: "value".into(),
                    role: FieldRoleKind::Value,
                },
            ],
            data: vec![
                serde_json::json!({"bucket": "2026-01-01T00:00:00Z", "value": 42.5}),
                serde_json::json!({"bucket": "2026-01-01T00:01:00Z", "value": 38.1}),
            ],
            nlq_ir: sample_ir(),
            source_sql: "SELECT bucket, avg(value) FROM ... WHERE tenant_id = 'aaa' ...".into(),
            time_range: NlqTimeRange {
                from: "now-1h".into(),
                to: "now".into(),
            },
            signal_types: vec![NlqSignal::Metrics],
            sample_rate: Some(0.1),
            approximation_statement: "Result is sampled at 10%. Values are approximate within ±5%."
                .into(),
        }
    }

    // ── Serialization / roundtrip ─────────────────────────────────────────────

    #[test]
    fn visualization_frame_roundtrip_json() {
        let original = sample_frame();
        let json = serde_json::to_string(&original).expect("serialization failed");
        let roundtripped: VisualizationFrame =
            serde_json::from_str(&json).expect("deserialization failed");
        assert_eq!(original, roundtripped);
    }

    #[test]
    fn frame_type_serializes_as_snake_case() {
        let json = serde_json::to_string(&VisualizationFrameType::Timeseries).unwrap();
        assert_eq!(json, "\"timeseries\"");
        let json = serde_json::to_string(&VisualizationFrameType::Topk).unwrap();
        assert_eq!(json, "\"topk\"");
        let json = serde_json::to_string(&VisualizationFrameType::Distribution).unwrap();
        assert_eq!(json, "\"distribution\"");
    }

    #[test]
    fn field_role_kind_serializes_as_snake_case() {
        let json = serde_json::to_string(&FieldRoleKind::Bucket).unwrap();
        assert_eq!(json, "\"bucket\"");
        let json = serde_json::to_string(&FieldRoleKind::Time).unwrap();
        assert_eq!(json, "\"time\"");
    }

    // ── Provenance payload ────────────────────────────────────────────────────

    #[test]
    fn frame_contains_all_6_provenance_fields() {
        let frame = sample_frame();
        // Serialize and verify all 6 provenance fields are present
        let json = serde_json::to_string(&frame).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("nlq_ir").is_some(), "nlq_ir must be present");
        assert!(v.get("source_sql").is_some(), "source_sql must be present");
        assert!(v.get("time_range").is_some(), "time_range must be present");
        assert!(
            v.get("signal_types").is_some(),
            "signal_types must be present"
        );
        assert!(
            v.get("sample_rate").is_some(),
            "sample_rate must be present"
        );
        assert!(
            v.get("approximation_statement").is_some(),
            "approximation_statement must be present"
        );
    }

    #[test]
    fn approximation_statement_is_included_in_json() {
        let frame = sample_frame();
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("approximation_statement"));
        assert!(json.contains("sampled at 10%"));
    }

    #[test]
    fn sample_rate_none_serializes_as_null() {
        let mut frame = sample_frame();
        frame.sample_rate = None;
        let json = serde_json::to_string(&frame).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["sample_rate"], serde_json::Value::Null);
    }

    #[test]
    fn empty_data_array_is_valid() {
        let mut frame = sample_frame();
        frame.data = vec![];
        let json = serde_json::to_string(&frame).unwrap();
        let roundtripped: VisualizationFrame = serde_json::from_str(&json).unwrap();
        assert!(roundtripped.data.is_empty());
    }

    #[test]
    fn empty_field_roles_round_trips() {
        let mut frame = sample_frame();
        frame.field_roles = vec![];
        let json = serde_json::to_string(&frame).unwrap();
        let rt: VisualizationFrame = serde_json::from_str(&json).unwrap();
        assert!(rt.field_roles.is_empty());
    }

    // ── NlqVisualizationHint → VisualizationFrameType conversion ─────────────

    #[test]
    fn visualization_hint_to_frame_type_all_variants() {
        let pairs = [
            (
                NlqVisualizationHint::Timeseries,
                VisualizationFrameType::Timeseries,
            ),
            (
                NlqVisualizationHint::Histogram,
                VisualizationFrameType::Histogram,
            ),
            (
                NlqVisualizationHint::Heatmap,
                VisualizationFrameType::Heatmap,
            ),
            (NlqVisualizationHint::Table, VisualizationFrameType::Table),
            (NlqVisualizationHint::Topk, VisualizationFrameType::Topk),
            (
                NlqVisualizationHint::Flamegraph,
                VisualizationFrameType::Flamegraph,
            ),
            (
                NlqVisualizationHint::Distribution,
                VisualizationFrameType::Distribution,
            ),
        ];
        for (hint, expected) in pairs {
            assert_eq!(
                VisualizationFrameType::from(hint),
                expected,
                "hint {hint:?} must convert to {expected:?}"
            );
        }
    }

    // ── Histogram frame with field_roles ─────────────────────────────────────

    #[test]
    fn histogram_frame_with_bucket_field_roles() {
        let mut ir = sample_ir();
        ir.operation = NlqOperation::Histogram;
        ir.visualization_hint = Some(NlqVisualizationHint::Histogram);

        let frame = VisualizationFrame {
            frame_type: VisualizationFrameType::Histogram,
            x_field: Some("bound".into()),
            y_field: Some("count".into()),
            series_field: None,
            unit: Some("ms".into()),
            suggested_visualization: "barchart".into(),
            field_roles: vec![
                FieldRole {
                    name: "bound".into(),
                    role: FieldRoleKind::Bucket,
                },
                FieldRole {
                    name: "count".into(),
                    role: FieldRoleKind::Value,
                },
            ],
            data: vec![
                serde_json::json!({"bound": 1.0, "count": 10}),
                serde_json::json!({"bound": 5.0, "count": 42}),
                serde_json::json!({"bound": 10.0, "count": 8}),
            ],
            nlq_ir: ir,
            source_sql: "SELECT bound, sum(count_in_bucket) AS count FROM ...".into(),
            time_range: NlqTimeRange {
                from: "now-30m".into(),
                to: "now".into(),
            },
            signal_types: vec![NlqSignal::Metrics],
            sample_rate: None,
            approximation_statement: "Result covers the last 30 minutes. Histogram bucket counts \
                may include partial windows at boundaries."
                .into(),
        };

        let json = serde_json::to_string(&frame).unwrap();
        let rt: VisualizationFrame = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.frame_type, VisualizationFrameType::Histogram);
        assert_eq!(rt.field_roles.len(), 2);
        assert_eq!(rt.field_roles[0].role, FieldRoleKind::Bucket);
        assert_eq!(rt.data.len(), 3);
    }

    // ── Provenance multi-signal frame ─────────────────────────────────────────

    #[test]
    fn frame_supports_multiple_signal_types() {
        let mut ir = sample_ir();
        ir.signals = vec![NlqSignal::Metrics, NlqSignal::Traces, NlqSignal::Logs];
        let mut frame = sample_frame();
        frame.nlq_ir = ir;
        frame.signal_types = vec![NlqSignal::Metrics, NlqSignal::Traces, NlqSignal::Logs];

        let json = serde_json::to_string(&frame).unwrap();
        let rt: VisualizationFrame = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.signal_types.len(), 3);
    }

    // ── Filter with NlqFilter in provenance IR ────────────────────────────────

    #[test]
    fn frame_provenance_ir_preserves_filters() {
        let mut ir = sample_ir();
        ir.filters = vec![NlqFilter {
            field: "service_name".into(),
            op: NlqFilterOp::Eq,
            value: "payments".into(),
        }];
        let mut frame = sample_frame();
        frame.nlq_ir = ir;

        let json = serde_json::to_string(&frame).unwrap();
        let rt: VisualizationFrame = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.nlq_ir.filters.len(), 1);
        assert_eq!(rt.nlq_ir.filters[0].value, "payments");
    }
}
