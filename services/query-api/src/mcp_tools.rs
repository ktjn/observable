// MCP Server schema lookup tools — used by the LLM during NLQ IR generation (Stage 1).
// These functions read from the Schema Registry (schema_entries + semantic_annotations)
// so the LLM can ground its NlqIr output in real field names and metric semantics.
//
// All functions carry tenant context. schema_entries is the structural source of truth;
// semantic_annotations is a tenant-scoped overlay on top of it.
//
// HTTP endpoints return 200 for all lookups (including misses); 404 is reserved for
// missing path segments, not for "field not found" control flow.
//
// See ADR-021 §Stage 1.
use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const VALID_SIGNAL_TYPES: &[&str] = &[
    "traces",
    "logs",
    "metrics",
    "profiles",
    "events",
    "deployments",
];

// ── Response types ────────────────────────────────────────────────────────────

/// Full metric schema: structural catalog entry + tenant annotation overlay.
/// `schema_complete` is true when both `metric_type` and `timestamp_column` are present —
/// the minimum required for the MCP server to generate correct time-series SQL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricSchema {
    pub field_name: String,
    pub field_type: String,
    pub otel_spec_version: Option<String>,
    // annotation overlay — None means no annotation was set for this tenant
    pub display_name: Option<String>,
    pub business_description: Option<String>,
    pub interpretation_rule: Option<String>,
    pub effective_sample_rate: Option<f64>,
    /// None when no annotation row exists; false when annotated but not_for_billing = false.
    pub not_for_billing: Option<bool>,
    pub metric_type: Option<String>,
    pub timestamp_column: Option<String>,
    pub unit: Option<String>,
    pub recommended_downsampling: Option<String>,
    /// True when metric_type and timestamp_column are both non-null (sufficient for Step 3 SQL).
    pub schema_complete: bool,
}

/// A field from the structural catalog plus the tenant's annotation overlay (if any).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalField {
    pub field_name: String,
    pub field_type: String,
    pub otel_spec_version: Option<String>,
    // annotation overlay (None if this tenant has no annotation for this field)
    pub display_name: Option<String>,
    pub business_description: Option<String>,
    pub interpretation_rule: Option<String>,
    pub effective_sample_rate: Option<f64>,
    pub metric_type: Option<String>,
    /// The time-series timestamp column for this metric (e.g. `timestamp_unix_nano`).
    /// Required for `schema_complete = true`.
    pub timestamp_column: Option<String>,
    pub unit: Option<String>,
    pub recommended_downsampling: Option<String>,
    /// True when both `metric_type` and `timestamp_column` are non-null — the minimum
    /// annotation required for the MCP server to generate correct time-series SQL.
    pub schema_complete: bool,
}

/// Outcome of `resolve_label_to_column`.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolveLabelResult {
    /// Exactly one canonical `field_name` found.
    Found(String),
    /// Multiple fields matched; the LLM or caller must disambiguate.
    Ambiguous(Vec<String>),
    /// No match found.
    NotFound,
}

// HTTP response shape for resolve_label — serialisable, LLM-friendly.
#[derive(Serialize)]
pub struct ResolveLabelResponse {
    /// The resolved canonical field name; null when not found or ambiguous.
    pub field_name: Option<String>,
    /// True when more than one field matched the given label.
    pub ambiguous: bool,
    /// Non-empty only when ambiguous is true.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<String>,
}

// ── sqlx FromRow intermediaries ───────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct MetricSchemaRow {
    field_name: String,
    field_type: String,
    otel_spec_version: Option<String>,
    display_name: Option<String>,
    business_description: Option<String>,
    interpretation_rule: Option<String>,
    effective_sample_rate: Option<f64>,
    // Option<bool> because LEFT JOIN may produce SQL NULL even though the column is NOT NULL
    not_for_billing: Option<bool>,
    metric_type: Option<String>,
    timestamp_column: Option<String>,
    unit: Option<String>,
    recommended_downsampling: Option<String>,
}

impl From<MetricSchemaRow> for MetricSchema {
    fn from(row: MetricSchemaRow) -> Self {
        let schema_complete = row.metric_type.is_some() && row.timestamp_column.is_some();
        Self {
            field_name: row.field_name,
            field_type: row.field_type,
            otel_spec_version: row.otel_spec_version,
            display_name: row.display_name,
            business_description: row.business_description,
            interpretation_rule: row.interpretation_rule,
            effective_sample_rate: row.effective_sample_rate,
            not_for_billing: row.not_for_billing,
            metric_type: row.metric_type,
            timestamp_column: row.timestamp_column,
            unit: row.unit,
            recommended_downsampling: row.recommended_downsampling,
            schema_complete,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SignalFieldRow {
    field_name: String,
    field_type: String,
    otel_spec_version: Option<String>,
    display_name: Option<String>,
    business_description: Option<String>,
    interpretation_rule: Option<String>,
    effective_sample_rate: Option<f64>,
    metric_type: Option<String>,
    timestamp_column: Option<String>,
    unit: Option<String>,
    recommended_downsampling: Option<String>,
}

impl From<SignalFieldRow> for SignalField {
    fn from(row: SignalFieldRow) -> Self {
        let schema_complete = row.metric_type.is_some() && row.timestamp_column.is_some();
        Self {
            field_name: row.field_name,
            field_type: row.field_type,
            otel_spec_version: row.otel_spec_version,
            display_name: row.display_name,
            business_description: row.business_description,
            interpretation_rule: row.interpretation_rule,
            effective_sample_rate: row.effective_sample_rate,
            metric_type: row.metric_type,
            timestamp_column: row.timestamp_column,
            unit: row.unit,
            recommended_downsampling: row.recommended_downsampling,
            schema_complete,
        }
    }
}

// ── Core data-access functions ────────────────────────────────────────────────

/// Returns the structural + annotation schema for one metric field.
///
/// `schema_entries` is the source of truth for structural existence.
/// The annotation overlay is LEFT JOINed and may be absent.
/// Returns `None` when `metric_name` does not exist in `schema_entries` at all.
pub async fn get_metric_schema(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    metric_name: &str,
) -> Result<Option<MetricSchema>, sqlx::Error> {
    let row = sqlx::query_as::<_, MetricSchemaRow>(
        "SELECT \
             se.field_name, se.field_type, se.otel_spec_version, \
             sa.display_name, sa.business_description, sa.interpretation_rule, \
             sa.effective_sample_rate, sa.not_for_billing, \
             sa.metric_type, sa.timestamp_column, sa.unit, sa.recommended_downsampling \
         FROM schema_entries se \
         LEFT JOIN semantic_annotations sa \
             ON  sa.signal_type = se.signal_type \
             AND sa.field_name  = se.field_name \
             AND sa.tenant_id   = $1 \
         WHERE se.signal_type = 'metrics' \
           AND se.field_name  = $2",
    )
    .bind(tenant_id)
    .bind(metric_name)
    .fetch_optional(db)
    .await?;

    Ok(row.map(MetricSchema::from))
}

/// Returns all fields in the structural catalog for a signal type, with the tenant's
/// annotation overlay merged in (LEFT JOIN — annotated fields have annotation data,
/// unannotated fields have None annotation columns).
/// `schema_complete` is set to true on fields where both `metric_type` and `timestamp_column`
/// are present — the minimum required for MCP SQL generation.
pub async fn list_signal_fields(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    signal_type: &str,
) -> Result<Vec<SignalField>, sqlx::Error> {
    let rows = sqlx::query_as::<_, SignalFieldRow>(
        "SELECT \
             se.field_name, se.field_type, se.otel_spec_version, \
             sa.display_name, sa.business_description, sa.interpretation_rule, \
             sa.effective_sample_rate, sa.metric_type, sa.timestamp_column, \
             sa.unit, sa.recommended_downsampling \
         FROM schema_entries se \
         LEFT JOIN semantic_annotations sa \
             ON  sa.signal_type = se.signal_type \
             AND sa.field_name  = se.field_name \
             AND sa.tenant_id   = $2 \
         WHERE se.signal_type = $1 \
         ORDER BY se.field_name",
    )
    .bind(signal_type)
    .bind(tenant_id)
    .fetch_all(db)
    .await?;

    Ok(rows.into_iter().map(SignalField::from).collect())
}

/// Resolves a human-readable label to a canonical `field_name`.
///
/// Resolution order:
/// 1. Exact match on `field_name` in `schema_entries` for the given signal type.
/// 2. Case-insensitive, whitespace-trimmed match on `display_name` in `semantic_annotations`
///    (restricted to fields that exist in `schema_entries`).
///
/// If multiple fields share a `display_name` that matches the label, returns
/// `Ambiguous` with all candidates so the caller can disambiguate.
pub async fn resolve_label_to_column(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    signal_type: &str,
    label: &str,
) -> Result<ResolveLabelResult, sqlx::Error> {
    // Step 1: exact field_name match (fast path, no annotation required)
    let exact: Option<String> = sqlx::query_scalar(
        "SELECT field_name FROM schema_entries \
         WHERE signal_type = $1 AND field_name = $2",
    )
    .bind(signal_type)
    .bind(label)
    .fetch_optional(db)
    .await?;

    if let Some(field) = exact {
        return Ok(ResolveLabelResult::Found(field));
    }

    // Step 2: case-insensitive display_name match (only for fields in the global catalog)
    let candidates: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT se.field_name \
         FROM schema_entries se \
         JOIN semantic_annotations sa \
             ON  sa.signal_type = se.signal_type \
             AND sa.field_name  = se.field_name \
             AND sa.tenant_id   = $3 \
         WHERE se.signal_type = $1 \
           AND LOWER(TRIM(sa.display_name)) = LOWER(TRIM($2)) \
         ORDER BY se.field_name",
    )
    .bind(signal_type)
    .bind(label)
    .bind(tenant_id)
    .fetch_all(db)
    .await?;

    match candidates.len() {
        0 => Ok(ResolveLabelResult::NotFound),
        1 => Ok(ResolveLabelResult::Found(
            candidates.into_iter().next().unwrap(),
        )),
        _ => Ok(ResolveLabelResult::Ambiguous(candidates)),
    }
}

/// Fetches the top-N distinct attribute keys present in metric_series for this tenant.
/// Combines native columns (service_name, environment, metric_name) with the most-frequent
/// JSON attribute keys extracted from the attributes column.
///
/// The ClickHouse dialect (`JSONExtractKeys`, `arrayJoin`) is encapsulated here and never
/// leaks to callers.
pub async fn fetch_label_keys(
    ch: &clickhouse::Client,
    tenant_id: uuid::Uuid,
    limit: usize,
) -> Result<Vec<String>, clickhouse::error::Error> {
    const NATIVE_COLS: &[&str] = &["service_name", "environment", "metric_name"];

    #[derive(clickhouse::Row, serde::Deserialize)]
    struct LabelKeyRow {
        label_key: String,
        #[allow(dead_code)]
        cnt: u64,
    }

    let sql = format!(
        "SELECT \
             arrayJoin(JSONExtractKeys(attributes)) AS label_key, \
             count() AS cnt \
         FROM observable.metric_series \
         WHERE tenant_id = '{tenant_id}' \
           AND attributes != '{{}}' \
         GROUP BY label_key \
         ORDER BY cnt DESC \
         LIMIT {limit}"
    );

    let rows: Vec<LabelKeyRow> = ch.query(&sql).fetch_all().await?;

    let mut keys: Vec<String> = NATIVE_COLS.iter().map(|s| s.to_string()).collect();
    let native_set: std::collections::HashSet<&str> = NATIVE_COLS.iter().copied().collect();

    for row in rows {
        if !native_set.contains(row.label_key.as_str()) && keys.len() < limit {
            keys.push(row.label_key);
        }
    }

    Ok(keys)
}

// ── HTTP handlers ─────────────────────────────────────────────────────────────

/// GET /v1/mcp/tools/metric-schema/:metric_name
///
/// Returns the structural + annotation schema for a single metric field.
/// Returns `null` (200) when the metric does not exist — miss is expected control flow.
pub async fn handle_get_metric_schema(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(metric_name): Path<String>,
) -> Result<Json<Option<MetricSchema>>, StatusCode> {
    match get_metric_schema(&state.db, ctx.tenant_id, &metric_name).await {
        Ok(schema) => Ok(Json(schema)),
        Err(e) => {
            tracing::error!(error = %e, metric_name, "get_metric_schema db error");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Deserialize)]
pub struct ListFieldsParams {
    // optional: filter to only fields that have a tenant annotation
    #[serde(default)]
    pub annotated_only: bool,
}

/// GET /v1/mcp/tools/signal-fields/:signal_type[?annotated_only=true]
///
/// Returns all structural fields for a signal type, with the tenant's annotation overlay.
/// Always returns 200; empty array when the signal_type has no entries yet.
pub async fn handle_list_signal_fields(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(signal_type): Path<String>,
    Query(params): Query<ListFieldsParams>,
) -> Result<Json<Vec<SignalField>>, StatusCode> {
    if !VALID_SIGNAL_TYPES.contains(&signal_type.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    match list_signal_fields(&state.db, ctx.tenant_id, &signal_type).await {
        Ok(mut fields) => {
            if params.annotated_only {
                fields.retain(|f| f.display_name.is_some() || f.metric_type.is_some());
            }
            Ok(Json(fields))
        }
        Err(e) => {
            tracing::error!(error = %e, signal_type, "list_signal_fields db error");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Deserialize)]
pub struct ResolveLabelParams {
    pub label: String,
}

/// GET /v1/mcp/tools/resolve-label/:signal_type?label=<label>
///
/// Resolves a human-readable label to a canonical field name.
/// Always returns 200; uses `ambiguous: true` and `candidates` for ambiguous matches,
/// `field_name: null` for misses — never 404 — since miss is expected control flow.
pub async fn handle_resolve_label(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(signal_type): Path<String>,
    Query(params): Query<ResolveLabelParams>,
) -> Result<Json<ResolveLabelResponse>, StatusCode> {
    if !VALID_SIGNAL_TYPES.contains(&signal_type.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    match resolve_label_to_column(&state.db, ctx.tenant_id, &signal_type, &params.label).await {
        Ok(ResolveLabelResult::Found(field)) => Ok(Json(ResolveLabelResponse {
            field_name: Some(field),
            ambiguous: false,
            candidates: vec![],
        })),
        Ok(ResolveLabelResult::NotFound) => Ok(Json(ResolveLabelResponse {
            field_name: None,
            ambiguous: false,
            candidates: vec![],
        })),
        Ok(ResolveLabelResult::Ambiguous(candidates)) => Ok(Json(ResolveLabelResponse {
            field_name: None,
            ambiguous: true,
            candidates,
        })),
        Err(e) => {
            tracing::error!(error = %e, signal_type, label = params.label, "resolve_label db error");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── MetricSchema ──────────────────────────────────────────────────────────

    #[test]
    fn schema_complete_requires_both_metric_type_and_timestamp_column() {
        let row = MetricSchemaRow {
            field_name: "latency".into(),
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
        };
        let schema: MetricSchema = row.into();
        assert!(schema.schema_complete, "both fields present → complete");
    }

    #[test]
    fn schema_incomplete_when_timestamp_column_missing() {
        let row = MetricSchemaRow {
            field_name: "latency".into(),
            field_type: "Float64".into(),
            otel_spec_version: None,
            display_name: None,
            business_description: None,
            interpretation_rule: None,
            effective_sample_rate: None,
            not_for_billing: None,
            metric_type: Some("gauge".into()),
            timestamp_column: None,
            unit: None,
            recommended_downsampling: None,
        };
        let schema: MetricSchema = row.into();
        assert!(
            !schema.schema_complete,
            "missing timestamp_column → incomplete"
        );
    }

    #[test]
    fn schema_incomplete_when_metric_type_missing() {
        let row = MetricSchemaRow {
            field_name: "latency".into(),
            field_type: "Float64".into(),
            otel_spec_version: None,
            display_name: None,
            business_description: None,
            interpretation_rule: None,
            effective_sample_rate: None,
            not_for_billing: None,
            metric_type: None,
            timestamp_column: Some("ts".into()),
            unit: None,
            recommended_downsampling: None,
        };
        let schema: MetricSchema = row.into();
        assert!(!schema.schema_complete, "missing metric_type → incomplete");
    }

    #[test]
    fn schema_incomplete_when_no_annotation() {
        let row = MetricSchemaRow {
            field_name: "latency".into(),
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
        };
        let schema: MetricSchema = row.into();
        assert!(!schema.schema_complete, "no annotation → incomplete");
        assert!(schema.not_for_billing.is_none(), "null LEFT JOIN → None");
    }

    // ── SignalField ───────────────────────────────────────────────────────────

    #[test]
    fn signal_field_without_annotation_has_none_overlay() {
        let row = SignalFieldRow {
            field_name: "trace_id".into(),
            field_type: "String".into(),
            otel_spec_version: Some("1.26.0".into()),
            display_name: None,
            business_description: None,
            interpretation_rule: None,
            effective_sample_rate: None,
            metric_type: None,
            timestamp_column: None,
            unit: None,
            recommended_downsampling: None,
        };
        let field: SignalField = row.into();
        assert_eq!(field.field_name, "trace_id");
        assert!(field.display_name.is_none());
        assert!(field.metric_type.is_none());
        assert!(
            !field.schema_complete,
            "no metric_type/timestamp_column → not complete"
        );
    }

    #[test]
    fn signal_field_with_annotation_has_overlay_data() {
        let row = SignalFieldRow {
            field_name: "latency_ms".into(),
            field_type: "Float64".into(),
            otel_spec_version: None,
            display_name: Some("Request Latency".into()),
            business_description: Some("P99 latency".into()),
            interpretation_rule: Some("higher_is_worse".into()),
            effective_sample_rate: Some(1.0),
            metric_type: Some("gauge".into()),
            timestamp_column: Some("timestamp_unix_nano".into()),
            unit: Some("ms".into()),
            recommended_downsampling: Some("1m".into()),
        };
        let field: SignalField = row.into();
        assert_eq!(field.display_name.as_deref(), Some("Request Latency"));
        assert_eq!(field.metric_type.as_deref(), Some("gauge"));
        assert_eq!(field.unit.as_deref(), Some("ms"));
        assert!(
            field.schema_complete,
            "metric_type + timestamp_column → complete"
        );
    }

    // ── ResolveLabelResponse serialisation ───────────────────────────────────

    #[test]
    fn resolve_response_found_serialises_correctly() {
        let resp = ResolveLabelResponse {
            field_name: Some("request_duration_ms".into()),
            ambiguous: false,
            candidates: vec![],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"field_name\":\"request_duration_ms\""));
        assert!(
            !json.contains("candidates"),
            "empty candidates should be omitted"
        );
    }

    #[test]
    fn resolve_response_not_found_has_null_field_name() {
        let resp = ResolveLabelResponse {
            field_name: None,
            ambiguous: false,
            candidates: vec![],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"field_name\":null"));
        assert!(!json.contains("candidates"));
    }

    #[test]
    fn resolve_response_ambiguous_includes_candidates() {
        let resp = ResolveLabelResponse {
            field_name: None,
            ambiguous: true,
            candidates: vec!["field_a".into(), "field_b".into()],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"ambiguous\":true"));
        assert!(json.contains("\"candidates\":[\"field_a\",\"field_b\"]"));
    }

    // ── Signal type validation ────────────────────────────────────────────────

    #[test]
    fn valid_signal_types_accepted() {
        for st in VALID_SIGNAL_TYPES {
            assert!(VALID_SIGNAL_TYPES.contains(st));
        }
    }

    #[test]
    fn invalid_signal_type_not_accepted() {
        assert!(!VALID_SIGNAL_TYPES.contains(&"events_stream"));
        assert!(!VALID_SIGNAL_TYPES.contains(&""));
    }

    // ── MetricSchema roundtrip JSON ───────────────────────────────────────────

    #[test]
    fn metric_schema_serializes_and_deserializes() {
        let original = MetricSchema {
            field_name: "latency_ms".into(),
            field_type: "Float64".into(),
            otel_spec_version: Some("1.26.0".into()),
            display_name: Some("Request Latency".into()),
            business_description: None,
            interpretation_rule: Some("higher_is_worse".into()),
            effective_sample_rate: Some(0.1),
            not_for_billing: Some(true),
            metric_type: Some("gauge".into()),
            timestamp_column: Some("ts".into()),
            unit: Some("ms".into()),
            recommended_downsampling: Some("1m".into()),
            schema_complete: true,
        };
        let json = serde_json::to_string(&original).unwrap();
        let roundtripped: MetricSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(original, roundtripped);
    }
}
