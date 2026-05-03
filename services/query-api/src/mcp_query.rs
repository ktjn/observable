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
use crate::discovery::{
    all_infrastructure_entity_types, fetch_infrastructure_summaries, InfrastructureEntityType,
};
use crate::mcp_tools::get_metric_schema;
use crate::middleware::auth::TenantContext;
use crate::sql_templates::{
    generate_log_sql, generate_sql, LogSqlContext, SchemaMetricType, SqlContext, SqlTemplateError,
};
use crate::traces::AppState;
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    Json,
};
use domain::{
    FieldRole, FieldRoleKind, NlqFilterOp, NlqIr, NlqOperation, NlqSignal, VisualizationFrame,
    VisualizationFrameType,
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
    // Log signal routing — log queries bypass the metric schema entirely.
    if ir.signals == vec![NlqSignal::Logs] {
        return execute_log_query(ch, tenant_id, ir).await;
    }

    // Trace signal routing — trace queries bypass the metric schema.
    if ir.signals == vec![NlqSignal::Traces] {
        return execute_trace_query(ch, tenant_id, ir).await;
    }

    // Catalog operation: query series metadata; no metric or schema lookup needed.
    if ir.operation == NlqOperation::Catalog {
        return execute_catalog_query(ch, tenant_id, ir).await;
    }

    // Inventory operation: filter infrastructure entity table; no metric required.
    if ir.operation == NlqOperation::Inventory {
        return execute_inventory_query(ch, tenant_id, ir).await;
    }

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

// ── Catalog query ─────────────────────────────────────────────────────────────

/// Executes a catalog (distinct-values) query against `observable.metric_series`.
///
/// No metric schema lookup is required — catalog operations query dimension metadata
/// directly, not metric time-series points.
async fn execute_catalog_query(
    ch: &clickhouse::Client,
    tenant_id: Uuid,
    ir: &NlqIr,
) -> Result<VisualizationFrame, McpQueryError> {
    let ctx = SqlContext {
        tenant_id,
        metric_name: "",
        metric_type: SchemaMetricType::Unknown,
        ir,
    };
    let sql = generate_sql(&ctx)?;

    // The SQL now aliases the dimension column by its actual name (e.g. "service_name",
    // "metric_name") so frontends and eval tooling can identify columns semantically.
    let label_col = ir.catalog_field.as_deref().unwrap_or("value").to_string();

    tracing::debug!(
        tenant_id = %tenant_id,
        catalog_field = ?ir.catalog_field,
        "MCP executing catalog SQL"
    );

    let data = execute_query_as_json(ch, &sql).await?;

    let frame_type = VisualizationFrameType::Table;
    let field_roles = vec![
        FieldRole {
            name: label_col.clone(),
            role: FieldRoleKind::Label,
        },
        FieldRole {
            name: "series_count".into(),
            role: FieldRoleKind::Value,
        },
    ];

    Ok(VisualizationFrame {
        frame_type,
        x_field: Some(label_col),
        y_field: Some("series_count".into()),
        series_field: None,
        unit: None,
        suggested_visualization: suggested_panel(&frame_type),
        field_roles,
        data,
        nlq_ir: ir.clone(),
        source_sql: sql,
        time_range: ir.time_range.clone(),
        signal_types: ir.signals.clone(),
        sample_rate: None,
        approximation_statement: "Advisory result. Data is not sampled. \
            This result is approximate and must not be used for billing, \
            SLA enforcement, or regulatory compliance."
            .into(),
    })
}

// ── Log query ─────────────────────────────────────────────────────────────────

/// Executes a log search query against `observable.logs`.
///
/// Supports free-text body search via `ir.query` and structured filters on
/// direct columns (service_name, severity_text, environment, trace_id, span_id)
/// or JSON attribute extraction.
async fn execute_log_query(
    ch: &clickhouse::Client,
    tenant_id: Uuid,
    ir: &NlqIr,
) -> Result<VisualizationFrame, McpQueryError> {
    let ctx = LogSqlContext { tenant_id, ir };
    let sql = generate_log_sql(&ctx)?;

    tracing::debug!(
        tenant_id = %tenant_id,
        query = ?ir.query,
        "MCP executing log search SQL"
    );

    let data = execute_query_as_json(ch, &sql).await?;

    let field_roles = vec![
        FieldRole {
            name: "ts".into(),
            role: FieldRoleKind::Time,
        },
        FieldRole {
            name: "body".into(),
            role: FieldRoleKind::Value,
        },
        FieldRole {
            name: "service_name".into(),
            role: FieldRoleKind::Label,
        },
        FieldRole {
            name: "severity_text".into(),
            role: FieldRoleKind::Label,
        },
    ];

    Ok(VisualizationFrame {
        frame_type: VisualizationFrameType::Table,
        x_field: Some("ts".into()),
        y_field: None,
        series_field: None,
        unit: None,
        suggested_visualization: "table".into(),
        field_roles,
        data,
        nlq_ir: ir.clone(),
        source_sql: sql,
        time_range: ir.time_range.clone(),
        signal_types: ir.signals.clone(),
        sample_rate: None,
        approximation_statement: "Advisory result — log search. Logs are not sampled \
            but may be incomplete under backpressure. This result must not be used for \
            billing, SLA enforcement, or regulatory compliance."
            .into(),
    })
}

// ── Trace query ───────────────────────────────────────────────────────────────

/// Executes a trace search query against `observable.spans`.
///
/// Returns root spans (those with an empty `parent_span_id`) as distributed traces.
/// Supported IR filters: `service_name`, `status_code` (OK/ERROR/UNSET),
/// `environment`, free-text via `ir.query` on `operation_name`.
/// Time range is derived from `ir.time_range` (same `parse_relative_minutes` helper as logs).
async fn execute_trace_query(
    ch: &clickhouse::Client,
    tenant_id: Uuid,
    ir: &NlqIr,
) -> Result<VisualizationFrame, McpQueryError> {
    let lookback_minutes = parse_relative_minutes(&ir.time_range.from).unwrap_or(60);
    let db = format!("tenant_{}", tenant_id.as_simple());

    let filter_val = |field: &str| -> Option<String> {
        let want = field.to_lowercase();
        ir.filters
            .iter()
            .find(|f| f.field.to_lowercase() == want && f.op == NlqFilterOp::Eq)
            .map(|f| f.value.clone())
    };

    let service_name = filter_val("service_name");
    let status_code = filter_val("status_code");
    let environment = filter_val("environment");
    let operation_text = ir.query.as_deref().unwrap_or("");

    let mut where_clauses: Vec<String> = vec![
        "(parent_span_id = '' OR parent_span_id IS NULL)".into(),
        format!(
            "start_time_unix_nano >= toUnixTimestamp64Nano(now() - INTERVAL {lookback_minutes} MINUTE)"
        ),
    ];

    if let Some(svc) = &service_name {
        let escaped = svc.replace('\'', "\\'");
        where_clauses.push(format!("service_name = '{escaped}'"));
    }
    if let Some(sc) = &status_code {
        let escaped = sc.replace('\'', "\\'");
        where_clauses.push(format!("status_code = '{escaped}'"));
    }
    if let Some(env) = &environment {
        let escaped = env.replace('\'', "\\'");
        where_clauses.push(format!(
            "JSONExtractString(resource_attributes, 'deployment.environment') = '{escaped}'"
        ));
    }
    if !operation_text.is_empty() {
        let escaped = operation_text.replace('\'', "\\'");
        where_clauses.push(format!("operation_name ILIKE '%{escaped}%'"));
    }

    let where_sql = where_clauses.join(" AND ");
    let sql = format!(
        "SELECT \
           trace_id, \
           service_name AS root_service, \
           operation_name AS root_operation, \
           intDiv(duration_ns, 1000000) AS duration_ms, \
           status_code, \
           JSONExtractString(resource_attributes, 'deployment.environment') AS environment, \
           start_time_unix_nano \
         FROM {db}.spans \
         WHERE {where_sql} \
         ORDER BY start_time_unix_nano DESC \
         LIMIT 500"
    );

    tracing::debug!(
        tenant_id = %tenant_id,
        "MCP executing trace search SQL"
    );

    let data = execute_query_as_json(ch, &sql).await?;

    let field_roles = vec![
        FieldRole {
            name: "start_time_unix_nano".into(),
            role: FieldRoleKind::Time,
        },
        FieldRole {
            name: "root_service".into(),
            role: FieldRoleKind::Label,
        },
        FieldRole {
            name: "root_operation".into(),
            role: FieldRoleKind::Label,
        },
        FieldRole {
            name: "duration_ms".into(),
            role: FieldRoleKind::Value,
        },
    ];

    Ok(VisualizationFrame {
        frame_type: VisualizationFrameType::Table,
        x_field: Some("start_time_unix_nano".into()),
        y_field: None,
        series_field: None,
        unit: None,
        suggested_visualization: "table".into(),
        field_roles,
        data,
        nlq_ir: ir.clone(),
        source_sql: sql,
        time_range: ir.time_range.clone(),
        signal_types: ir.signals.clone(),
        sample_rate: None,
        approximation_statement: "Advisory result — trace search. Traces are not sampled \
            but may be incomplete under backpressure. This result must not be used for \
            billing, SLA enforcement, or regulatory compliance."
            .into(),
    })
}

// ── Inventory query ───────────────────────────────────────────────────────────

/// Executes an infrastructure entity inventory query.
///
/// Extracts entity attribute filters from the IR (`entity_type`, `environment`,
/// `service_name`, `display_name`/`name` for text search), delegates to
/// `fetch_infrastructure_summaries` for each entity type, and returns a
/// `VisualizationFrame(table)` where every data row is a serialised
/// `InfrastructureEntitySummary` with the same field names as the REST API.
async fn execute_inventory_query(
    ch: &clickhouse::Client,
    tenant_id: Uuid,
    ir: &NlqIr,
) -> Result<VisualizationFrame, McpQueryError> {
    // Extract filter values from the IR filter list.
    let filter_val = |field: &str| -> Option<String> {
        let want = field.to_lowercase();
        ir.filters
            .iter()
            .find(|f| f.field.to_lowercase() == want && f.op == NlqFilterOp::Eq)
            .map(|f| f.value.clone())
    };

    let environment = filter_val("environment")
        .or_else(|| filter_val("deployment.environment"))
        .or_else(|| filter_val("resource.environment"));
    let entity_type_str = filter_val("entity_type").or_else(|| filter_val("type"));
    let service = filter_val("service_name")
        .or_else(|| filter_val("service.name"))
        .or_else(|| filter_val("service"));
    let search = filter_val("display_name")
        .or_else(|| filter_val("name"))
        .or_else(|| filter_val("search"));

    // Determine lookback from time_range (e.g., "now-1h" → 60 min, "now-30m" → 30).
    let lookback_minutes = parse_relative_minutes(&ir.time_range.from).unwrap_or(60);

    // Determine which entity types to query.
    let entity_types: Vec<InfrastructureEntityType> = match entity_type_str.as_deref() {
        Some(s) => match InfrastructureEntityType::try_from(s) {
            Ok(et) => vec![et],
            Err(_) => {
                tracing::warn!(entity_type = %s, "inventory IR: unknown entity_type filter — querying all");
                all_infrastructure_entity_types().to_vec()
            }
        },
        None => all_infrastructure_entity_types().to_vec(),
    };

    let mut items = Vec::new();
    for entity_type in entity_types {
        let rows = fetch_infrastructure_summaries(
            ch,
            tenant_id,
            entity_type,
            environment.as_deref(),
            service.as_deref(),
            search.as_deref(),
            lookback_minutes,
        )
        .await
        .map_err(|_| {
            McpQueryError::ClickHouse(clickhouse::error::Error::Custom(
                "infrastructure inventory query failed".into(),
            ))
        })?;
        items.extend(rows);
    }

    // Sort by last_seen desc, then name asc — same ordering as the REST API.
    items.sort_by(|a, b| {
        b.last_seen_unix_nano
            .cmp(&a.last_seen_unix_nano)
            .then_with(|| a.display_name.cmp(&b.display_name))
    });

    let data: Vec<serde_json::Value> = items
        .into_iter()
        .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
        .collect();

    let source_sql = format!(
        "-- inventory: entity_type={:?} environment={:?} service={:?} search={:?} lookback={}m",
        entity_type_str, environment, service, search, lookback_minutes
    );

    Ok(VisualizationFrame {
        frame_type: VisualizationFrameType::Table,
        x_field: None,
        y_field: None,
        series_field: None,
        unit: None,
        suggested_visualization: "table".into(),
        field_roles: vec![],
        data,
        nlq_ir: ir.clone(),
        source_sql,
        time_range: ir.time_range.clone(),
        signal_types: ir.signals.clone(),
        sample_rate: None,
        approximation_statement: format!(
            "Advisory result — infrastructure entity inventory for the last {lookback_minutes} \
             minutes. Entities not seen within the lookback window are excluded. \
             This result must not be used for billing, SLA enforcement, or regulatory compliance."
        ),
    })
}

/// Parses a relative time expression of the form `now-{N}h`, `now-{N}m`, or `now-{N}d`
/// and returns the duration in minutes. Returns `None` for unrecognised formats.
fn parse_relative_minutes(from: &str) -> Option<u32> {
    let s = from.trim().to_lowercase();
    let s = s.strip_prefix("now-")?;
    if let Some(hours) = s.strip_suffix('h') {
        hours.parse::<u32>().ok().map(|h| h * 60)
    } else if let Some(mins) = s.strip_suffix('m') {
        mins.parse::<u32>().ok()
    } else if let Some(days) = s.strip_suffix('d') {
        days.parse::<u32>().ok().map(|d| d * 24 * 60)
    } else {
        None
    }
}

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
        let normalised = normalise_ch_denormals(line);
        let value: serde_json::Value =
            serde_json::from_slice(&normalised).map_err(McpQueryError::InvalidRow)?;
        rows.push(value);
    }
    Ok(rows)
}

/// Replaces bare ClickHouse NaN/Inf float literals with JSON `null`.
///
/// ClickHouse's default `output_format_json_quote_denormals = 0` emits `nan`,
/// `-nan`, `inf`, and `-inf` as bare tokens inside JSON objects.  These are
/// not valid JSON and cause `serde_json::from_slice` to fail.  We substitute
/// them with `null` so callers receive a well-formed document and the frontend
/// can render "—" for missing values.
///
/// The scanner tracks string context to avoid corrupting field names or string
/// values that happen to contain the substrings "nan" or "inf".
fn normalise_ch_denormals(src: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    // Fast-path: skip allocation when no denormal tokens are present.
    let lower = src.to_ascii_lowercase();
    if !lower.windows(3).any(|w| w == b"nan" || w == b"inf") {
        return std::borrow::Cow::Borrowed(src);
    }

    let mut out: Vec<u8> = Vec::with_capacity(src.len() + 16);
    let mut i = 0;
    let mut in_string = false;

    while i < src.len() {
        let b = src[i];

        if in_string {
            out.push(b);
            if b == b'\\' {
                // Consume escaped character verbatim.
                i += 1;
                if i < src.len() {
                    out.push(src[i]);
                }
            } else if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        if b == b'"' {
            in_string = true;
            out.push(b);
            i += 1;
            continue;
        }

        // Outside strings: check for -nan, nan, -inf, inf (case-insensitive).
        let rest = &lower[i..];
        if rest.starts_with(b"-nan") || rest.starts_with(b"-inf") {
            out.extend_from_slice(b"null");
            i += 4;
        } else if rest.starts_with(b"nan") || rest.starts_with(b"inf") {
            out.extend_from_slice(b"null");
            i += 3;
        } else {
            out.push(b);
            i += 1;
        }
    }

    std::borrow::Cow::Owned(out)
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
        NlqOperation::Catalog => VisualizationFrameType::Table,
        NlqOperation::Inventory => VisualizationFrameType::Table,
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
        VisualizationFrameType::Distribution => {
            // Use the requested percentiles from the IR when available; fall
            // back to a sensible default set otherwise.
            let default_stats: Vec<String> = vec![
                "p50".into(),
                "p90".into(),
                "p95".into(),
                "p99".into(),
                "min".into(),
                "max".into(),
            ];
            let stats = match &ir.percentiles {
                Some(p) if !p.is_empty() => p,
                _ => &default_stats,
            };
            let y_field = stats.first().cloned().unwrap_or_else(|| "p95".into());
            let field_roles = stats
                .iter()
                .map(|s| FieldRole {
                    name: s.clone(),
                    role: FieldRoleKind::Value,
                })
                .collect();
            (None, Some(y_field), None, field_roles)
        }
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
            percentiles: None,
            catalog_field: None,
            limit: None,
            query: None,
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

    // ── Distribution field layout ─────────────────────────────────────────────

    #[test]
    fn distribution_layout_uses_first_requested_percentile_as_y_field() {
        let mut ir = base_ir(NlqOperation::Distribution);
        ir.percentiles = Some(vec!["p95".into(), "median".into(), "average".into()]);
        let (x, y, series, _) = derive_field_layout(&ir, &VisualizationFrameType::Distribution);
        assert_eq!(x, None);
        assert_eq!(y.as_deref(), Some("p95"));
        assert_eq!(series, None);
    }

    #[test]
    fn distribution_layout_generates_one_role_per_requested_stat() {
        let mut ir = base_ir(NlqOperation::Distribution);
        ir.percentiles = Some(vec!["p95".into(), "median".into(), "average".into()]);
        let (_, _, _, roles) = derive_field_layout(&ir, &VisualizationFrameType::Distribution);
        assert_eq!(roles.len(), 3);
        assert_eq!(roles[0].name, "p95");
        assert_eq!(roles[1].name, "median");
        assert_eq!(roles[2].name, "average");
        assert!(roles.iter().all(|r| r.role == FieldRoleKind::Value));
    }

    #[test]
    fn distribution_layout_defaults_when_percentiles_absent() {
        let ir = base_ir(NlqOperation::Distribution);
        let (_, y, _, roles) = derive_field_layout(&ir, &VisualizationFrameType::Distribution);
        // Defaults must include p50/p95/p99 in roles.
        assert!(roles.iter().any(|r| r.name == "p50"));
        assert!(roles.iter().any(|r| r.name == "p99"));
        // y_field must be the first default stat.
        assert_eq!(y.as_deref(), Some("p50"));
    }

    // ── normalise_ch_denormals ────────────────────────────────────────────────

    #[test]
    fn nan_literal_is_replaced_with_null() {
        let input = br#"{"p95":nan,"median":4.5}"#;
        let out = normalise_ch_denormals(input);
        let v: serde_json::Value = serde_json::from_slice(&out).expect("must parse");
        assert!(v["p95"].is_null(), "nan must become null");
        assert_eq!(v["median"], serde_json::json!(4.5));
    }

    #[test]
    fn negative_nan_is_replaced_with_null() {
        let input = br#"{"p95":-nan}"#;
        let out = normalise_ch_denormals(input);
        let v: serde_json::Value = serde_json::from_slice(&out).expect("must parse");
        assert!(v["p95"].is_null());
    }

    #[test]
    fn inf_literal_is_replaced_with_null() {
        let input = br#"{"p99":inf,"p50":-inf,"avg":2.1}"#;
        let out = normalise_ch_denormals(input);
        let v: serde_json::Value = serde_json::from_slice(&out).expect("must parse");
        assert!(v["p99"].is_null());
        assert!(v["p50"].is_null());
        assert_eq!(v["avg"], serde_json::json!(2.1));
    }

    #[test]
    fn nan_inside_string_value_is_preserved() {
        let input = br#"{"metric":"latency_nan_ms","p95":nan}"#;
        let out = normalise_ch_denormals(input);
        let v: serde_json::Value = serde_json::from_slice(&out).expect("must parse");
        assert_eq!(v["metric"], serde_json::json!("latency_nan_ms"));
        assert!(v["p95"].is_null());
    }

    #[test]
    fn clean_row_is_borrowed_without_allocation() {
        let input = br#"{"p95":4.237,"median":3.1,"average":3.5}"#;
        let result = normalise_ch_denormals(input);
        assert!(
            matches!(result, std::borrow::Cow::Borrowed(_)),
            "clean row must not allocate"
        );
    }
}
