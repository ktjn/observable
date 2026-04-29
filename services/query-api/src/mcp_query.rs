// End-to-end MCP server query handler (P8-S6 Step 5).
//
// Wires together Steps 2–4:
//   NlqIr (POST body) → Schema Registry lookup (PostgreSQL) → SQL generation
//   → ClickHouse execution → VisualizationFrame response
//
// Advisory-only invariants (ADR-021 §Consequences):
//   - All generated SQL is SELECT-only; the handler generates no mutations.
//   - Every response carries all 6 provenance fields (nlq_ir, source_sql, time_range,
//     signal_types, sample_rate, approximation_statement).
//   - The response is advisory; callers must not feed it into automated alert evaluation,
//     billing, or SLA enforcement.
use crate::mcp_tools::get_metric_schema;
use crate::middleware::auth::TenantContext;
use crate::sql_templates::{generate_sql, SchemaMetricType, SqlContext, SqlTemplateError};
use crate::traces::AppState;
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    Json,
};
use domain::{
    FieldRole, FieldRoleKind, NlqIr, NlqOperation, VisualizationFrame, VisualizationFrameType,
};
use uuid::Uuid;

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum McpQueryError {
    MissingMetric,
    UnknownMetric(String),
    SqlTemplate(SqlTemplateError),
    ClickHouse(clickhouse::error::Error),
    InvalidRow(serde_json::Error),
}

impl std::fmt::Display for McpQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingMetric => write!(f, "NlqIr.metric is required for metric queries"),
            Self::UnknownMetric(m) => write!(f, "metric '{m}' not found in schema_entries"),
            Self::SqlTemplate(e) => write!(f, "SQL template error: {e}"),
            Self::ClickHouse(e) => write!(f, "ClickHouse error: {e}"),
            Self::InvalidRow(e) => write!(f, "invalid JSON row: {e}"),
        }
    }
}

impl std::error::Error for McpQueryError {}

impl From<SqlTemplateError> for McpQueryError {
    fn from(e: SqlTemplateError) -> Self {
        Self::SqlTemplate(e)
    }
}

impl From<clickhouse::error::Error> for McpQueryError {
    fn from(e: clickhouse::error::Error) -> Self {
        Self::ClickHouse(e)
    }
}

// ── Core orchestration ────────────────────────────────────────────────────────

/// Executes an NLQ query end-to-end: Schema Registry → SQL → ClickHouse → VisualizationFrame.
///
/// Advisory-only: every returned frame has a non-empty `approximation_statement`.
/// Callers must not feed the result into automated alert evaluation, billing, or SLA enforcement.
pub async fn execute_mcp_query(
    db: &sqlx::PgPool,
    ch: &clickhouse::Client,
    tenant_id: Uuid,
    ir: &NlqIr,
) -> Result<VisualizationFrame, McpQueryError> {
    // Step 1: Resolve metric name and schema
    let metric_name = ir.metric.as_deref().ok_or(McpQueryError::MissingMetric)?;

    let schema_opt = get_metric_schema(db, tenant_id, metric_name)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, metric_name, "schema registry lookup failed");
            // Wrap as ClickHouse error won't do; treat as internal
            McpQueryError::SqlTemplate(SqlTemplateError::MissingMetricName)
        })?;

    let schema = schema_opt.ok_or_else(|| McpQueryError::UnknownMetric(metric_name.into()))?;

    let metric_type = schema
        .metric_type
        .as_deref()
        .map(SchemaMetricType::parse)
        .unwrap_or(SchemaMetricType::Unknown);

    // Step 2: Generate SQL
    let ctx = SqlContext {
        tenant_id,
        metric_name,
        metric_type,
        ir,
    };
    let sql = generate_sql(&ctx)?;

    tracing::debug!(
        tenant_id = %tenant_id,
        metric_name,
        operation = ?ir.operation,
        "MCP generated SQL"
    );

    // Step 3: Execute against ClickHouse
    let data = execute_query_as_json(ch, &sql).await?;

    // Step 4: Build VisualizationFrame with provenance
    let frame_type = derive_frame_type(ir);
    let (x_field, y_field, series_field, field_roles) = derive_field_layout(ir, &frame_type);
    let sample_rate = schema.effective_sample_rate;
    let approximation_statement = build_approximation_statement(ir, sample_rate, &schema);

    Ok(VisualizationFrame {
        frame_type,
        x_field,
        y_field,
        series_field,
        unit: schema.unit.clone(),
        suggested_visualization: suggested_panel(&frame_type),
        field_roles,
        data,
        nlq_ir: ir.clone(),
        source_sql: sql,
        time_range: ir.time_range.clone(),
        signal_types: ir.signals.clone(),
        sample_rate,
        approximation_statement,
    })
}

// ── ClickHouse execution ──────────────────────────────────────────────────────

/// Executes raw SQL against ClickHouse and returns each row as a `serde_json::Value`.
///
/// Uses `JSONEachRow` format: one JSON object per line in the response.
pub async fn execute_query_as_json(
    ch: &clickhouse::Client,
    sql: &str,
) -> Result<Vec<serde_json::Value>, McpQueryError> {
    let mut cursor = ch.query(sql).fetch_bytes("JSONEachRow")?;

    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = cursor.next().await? {
        buf.extend_from_slice(&chunk);
    }

    let mut rows = Vec::new();
    for line in buf.split(|&b| b == b'\n') {
        let line = line.trim_ascii_end();
        if line.is_empty() {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_slice(line).map_err(McpQueryError::InvalidRow)?;
        rows.push(value);
    }
    Ok(rows)
}

// ── HTTP handler ──────────────────────────────────────────────────────────────

/// POST /v1/mcp/query
///
/// Accepts an `NlqIr` and returns a `VisualizationFrame` with all provenance fields.
/// Advisory-only: the response must not feed automated alert evaluation, billing, or SLA.
pub async fn handle_mcp_query(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(ir): Json<NlqIr>,
) -> Result<Json<VisualizationFrame>, (StatusCode, Json<serde_json::Value>)> {
    match execute_mcp_query(&state.db, &state.ch, ctx.tenant_id, &ir).await {
        Ok(frame) => Ok(Json(frame)),
        Err(McpQueryError::MissingMetric) => Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "metric is required for this operation"})),
        )),
        Err(McpQueryError::UnknownMetric(m)) => Err((
            StatusCode::NOT_FOUND,
            Json(
                serde_json::json!({"error": format!("metric '{m}' not found in schema registry")}),
            ),
        )),
        Err(McpQueryError::SqlTemplate(e)) => {
            tracing::warn!(error = %e, "SQL template generation failed");
            Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({"error": format!("invalid query: {e}")})),
            ))
        }
        Err(e) => {
            tracing::error!(error = %e, tenant_id = %ctx.tenant_id, "MCP query execution failed");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "query execution failed"})),
            ))
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn derive_frame_type(ir: &NlqIr) -> VisualizationFrameType {
    if let Some(hint) = ir.visualization_hint {
        return VisualizationFrameType::from(hint);
    }
    match ir.operation {
        NlqOperation::Timeseries | NlqOperation::Rate | NlqOperation::Irate => {
            VisualizationFrameType::Timeseries
        }
        NlqOperation::Increase => VisualizationFrameType::Timeseries,
        NlqOperation::Histogram => VisualizationFrameType::Histogram,
        NlqOperation::Topk => VisualizationFrameType::Topk,
        NlqOperation::Table => VisualizationFrameType::Table,
        NlqOperation::Distribution => VisualizationFrameType::Distribution,
    }
}

fn suggested_panel(frame_type: &VisualizationFrameType) -> String {
    match frame_type {
        VisualizationFrameType::Timeseries => "timeseries",
        VisualizationFrameType::Histogram => "barchart",
        VisualizationFrameType::Heatmap => "heatmap",
        VisualizationFrameType::Table => "table",
        VisualizationFrameType::Topk => "barchart",
        VisualizationFrameType::Flamegraph => "flamegraph",
        VisualizationFrameType::Distribution => "barchart",
    }
    .into()
}

fn derive_field_layout(
    ir: &NlqIr,
    frame_type: &VisualizationFrameType,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Vec<FieldRole>,
) {
    match frame_type {
        VisualizationFrameType::Timeseries => (
            Some("bucket".into()),
            Some("value".into()),
            ir.group_by.first().cloned(),
            vec![
                FieldRole {
                    name: "bucket".into(),
                    role: FieldRoleKind::Time,
                },
                FieldRole {
                    name: "value".into(),
                    role: FieldRoleKind::Value,
                },
            ],
        ),
        VisualizationFrameType::Histogram => (
            Some("bound".into()),
            Some("count".into()),
            None,
            vec![
                FieldRole {
                    name: "bound".into(),
                    role: FieldRoleKind::Bucket,
                },
                FieldRole {
                    name: "count".into(),
                    role: FieldRoleKind::Value,
                },
            ],
        ),
        VisualizationFrameType::Topk => (
            Some("service_name".into()),
            Some("avg_value".into()),
            None,
            vec![
                FieldRole {
                    name: "service_name".into(),
                    role: FieldRoleKind::Label,
                },
                FieldRole {
                    name: "avg_value".into(),
                    role: FieldRoleKind::Value,
                },
            ],
        ),
        VisualizationFrameType::Distribution => (
            None,
            Some("p99".into()),
            None,
            vec![
                FieldRole {
                    name: "p50".into(),
                    role: FieldRoleKind::Value,
                },
                FieldRole {
                    name: "p99".into(),
                    role: FieldRoleKind::Value,
                },
            ],
        ),
        _ => (Some("ts".into()), Some("value".into()), None, vec![]),
    }
}

fn build_approximation_statement(
    ir: &NlqIr,
    sample_rate: Option<f64>,
    schema: &crate::mcp_tools::MetricSchema,
) -> String {
    let time_desc = format!("time range {} to {}", ir.time_range.from, ir.time_range.to);

    let sampling_desc = match sample_rate {
        Some(rate) if rate < 1.0 => format!(
            " Data is sampled at {:.0}% — values are approximate.",
            rate * 100.0
        ),
        Some(_) => " Data is not sampled.".into(),
        None => " Sampling rate is unknown; values may be approximate.".into(),
    };

    let completeness = if !schema.schema_complete {
        " Schema annotations are incomplete — metric type or timestamp column is unset; \
         SQL pattern may be suboptimal."
    } else {
        ""
    };

    format!(
        "Advisory result for {time_desc}.{sampling_desc}{completeness} \
         This result is approximate and must not be used for billing, SLA enforcement, \
         or regulatory compliance."
    )
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{NlqOperation, NlqSignal, NlqTimeRange, NlqVisualizationHint};

    fn base_ir(op: NlqOperation) -> NlqIr {
        NlqIr {
            operation: op,
            signals: vec![NlqSignal::Metrics],
            metric: Some("latency_ms".into()),
            window: None,
            filters: vec![],
            group_by: vec![],
            resolution: Some("1m".into()),
            time_range: NlqTimeRange {
                from: "now-1h".into(),
                to: "now".into(),
            },
            visualization_hint: None,
        }
    }

    // ── derive_frame_type ─────────────────────────────────────────────────────

    #[test]
    fn timeseries_op_maps_to_timeseries_frame() {
        let ir = base_ir(NlqOperation::Timeseries);
        assert_eq!(derive_frame_type(&ir), VisualizationFrameType::Timeseries);
    }

    #[test]
    fn rate_op_maps_to_timeseries_frame() {
        let ir = base_ir(NlqOperation::Rate);
        assert_eq!(derive_frame_type(&ir), VisualizationFrameType::Timeseries);
    }

    #[test]
    fn histogram_op_maps_to_histogram_frame() {
        let ir = base_ir(NlqOperation::Histogram);
        assert_eq!(derive_frame_type(&ir), VisualizationFrameType::Histogram);
    }

    #[test]
    fn visualization_hint_overrides_operation_derived_type() {
        let mut ir = base_ir(NlqOperation::Timeseries);
        ir.visualization_hint = Some(NlqVisualizationHint::Heatmap);
        assert_eq!(derive_frame_type(&ir), VisualizationFrameType::Heatmap);
    }

    // ── suggested_panel ───────────────────────────────────────────────────────

    #[test]
    fn timeseries_frame_suggests_timeseries_panel() {
        assert_eq!(
            suggested_panel(&VisualizationFrameType::Timeseries),
            "timeseries"
        );
    }

    #[test]
    fn topk_frame_suggests_barchart_panel() {
        assert_eq!(suggested_panel(&VisualizationFrameType::Topk), "barchart");
    }

    // ── build_approximation_statement ────────────────────────────────────────

    #[test]
    fn approximation_statement_includes_time_range() {
        use crate::mcp_tools::MetricSchema;
        let ir = base_ir(NlqOperation::Timeseries);
        let schema = MetricSchema {
            field_name: "latency_ms".into(),
            field_type: "Float64".into(),
            otel_spec_version: None,
            display_name: None,
            business_description: None,
            interpretation_rule: None,
            effective_sample_rate: None,
            not_for_billing: None,
            metric_type: Some("gauge".into()),
            timestamp_column: Some("ts".into()),
            unit: None,
            recommended_downsampling: None,
            schema_complete: true,
        };
        let stmt = build_approximation_statement(&ir, None, &schema);
        assert!(
            stmt.contains("now-1h"),
            "time range must appear in statement: {stmt}"
        );
        assert!(
            stmt.contains("billing"),
            "advisory disclaimer must be present: {stmt}"
        );
    }

    #[test]
    fn approximation_statement_mentions_sampling_rate_when_known() {
        use crate::mcp_tools::MetricSchema;
        let ir = base_ir(NlqOperation::Timeseries);
        let schema = MetricSchema {
            field_name: "latency_ms".into(),
            field_type: "Float64".into(),
            otel_spec_version: None,
            display_name: None,
            business_description: None,
            interpretation_rule: None,
            effective_sample_rate: Some(0.1),
            not_for_billing: None,
            metric_type: Some("gauge".into()),
            timestamp_column: Some("ts".into()),
            unit: None,
            recommended_downsampling: None,
            schema_complete: true,
        };
        let stmt = build_approximation_statement(&ir, Some(0.1), &schema);
        assert!(stmt.contains("10%"), "sampling rate percentage: {stmt}");
    }

    #[test]
    fn approximation_statement_includes_completeness_warning() {
        use crate::mcp_tools::MetricSchema;
        let ir = base_ir(NlqOperation::Timeseries);
        let schema = MetricSchema {
            field_name: "latency_ms".into(),
            field_type: "Float64".into(),
            otel_spec_version: None,
            display_name: None,
            business_description: None,
            interpretation_rule: None,
            effective_sample_rate: None,
            not_for_billing: None,
            metric_type: None,
            timestamp_column: None,
            unit: None,
            recommended_downsampling: None,
            schema_complete: false,
        };
        let stmt = build_approximation_statement(&ir, None, &schema);
        assert!(
            stmt.contains("incomplete"),
            "incomplete schema must appear in statement: {stmt}"
        );
    }

    // ── derive_field_layout ───────────────────────────────────────────────────

    #[test]
    fn timeseries_layout_has_bucket_and_value() {
        let ir = base_ir(NlqOperation::Timeseries);
        let (x, y, _, roles) = derive_field_layout(&ir, &VisualizationFrameType::Timeseries);
        assert_eq!(x.as_deref(), Some("bucket"));
        assert_eq!(y.as_deref(), Some("value"));
        assert!(roles.iter().any(|r| r.role == FieldRoleKind::Time));
        assert!(roles.iter().any(|r| r.role == FieldRoleKind::Value));
    }

    #[test]
    fn histogram_layout_has_bound_bucket_role() {
        let ir = base_ir(NlqOperation::Histogram);
        let (x, y, _, roles) = derive_field_layout(&ir, &VisualizationFrameType::Histogram);
        assert_eq!(x.as_deref(), Some("bound"));
        assert_eq!(y.as_deref(), Some("count"));
        assert!(roles.iter().any(|r| r.role == FieldRoleKind::Bucket));
    }

    #[test]
    fn timeseries_with_group_by_sets_series_field() {
        let mut ir = base_ir(NlqOperation::Timeseries);
        ir.group_by = vec!["service_name".into()];
        let (_, _, series, _) = derive_field_layout(&ir, &VisualizationFrameType::Timeseries);
        assert_eq!(series.as_deref(), Some("service_name"));
    }
}
