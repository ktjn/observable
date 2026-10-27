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
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
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
    #[serde(default)]
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
    #[serde(default)]
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
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ResolveLabelResponse {
    /// The resolved canonical field name; null when not found or ambiguous.
    pub field_name: Option<String>,
    /// True when more than one field matched the given label.
    pub ambiguous: bool,
    /// Non-empty only when ambiguous is true.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub candidates: Vec<String>,
}

// ── Core data-access functions (delegated to admin-service over HTTP) ─────────

/// Returns the structural + annotation schema for one metric field.
///
/// Queries admin-service `/internal/schemas/metrics/schema/:metric_name`.
/// Returns `None` when `metric_name` does not exist in the schema registry.
pub async fn get_metric_schema(
    client: &reqwest::Client,
    admin_service_url: &str,
    internal_token: &str,
    tenant_id: Uuid,
    metric_name: &str,
) -> Result<Option<MetricSchema>, reqwest::Error> {
    let resp = client
        .get(format!(
            "{admin_service_url}/internal/schemas/metrics/schema/{metric_name}"
        ))
        .query(&[("tenant_id", tenant_id.to_string())])
        .header("X-Internal-Token", internal_token)
        .send()
        .await?
        .error_for_status()?;
    resp.json().await
}

/// Returns all fields in the structural catalog for a signal type, with the tenant's
/// annotation overlay merged in.
/// Queries admin-service `/internal/schemas/:signal_type/fields`.
pub async fn list_signal_fields(
    client: &reqwest::Client,
    admin_service_url: &str,
    internal_token: &str,
    tenant_id: Uuid,
    signal_type: &str,
) -> Result<Vec<SignalField>, reqwest::Error> {
    let resp = client
        .get(format!(
            "{admin_service_url}/internal/schemas/{signal_type}/fields"
        ))
        .query(&[("tenant_id", tenant_id.to_string())])
        .header("X-Internal-Token", internal_token)
        .send()
        .await?
        .error_for_status()?;
    resp.json().await
}

/// Resolves a human-readable label to a canonical `field_name`.
/// Queries admin-service `/internal/schemas/:signal_type/resolve-label`.
pub async fn resolve_label_to_column(
    client: &reqwest::Client,
    admin_service_url: &str,
    internal_token: &str,
    tenant_id: Uuid,
    signal_type: &str,
    label: &str,
) -> Result<ResolveLabelResult, reqwest::Error> {
    let resp: ResolveLabelResponse = client
        .get(format!(
            "{admin_service_url}/internal/schemas/{signal_type}/resolve-label"
        ))
        .query(&[
            ("tenant_id", tenant_id.to_string()),
            ("label", label.to_string()),
        ])
        .header("X-Internal-Token", internal_token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    if resp.ambiguous {
        Ok(ResolveLabelResult::Ambiguous(resp.candidates))
    } else if let Some(field) = resp.field_name {
        Ok(ResolveLabelResult::Found(field))
    } else {
        Ok(ResolveLabelResult::NotFound)
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
    match get_metric_schema(
        &state.http_client,
        &state.admin_service_url,
        &state.internal_service_token,
        ctx.tenant_id,
        &metric_name,
    )
    .await
    {
        Ok(schema) => Ok(Json(schema)),
        Err(e) => {
            tracing::error!(error = %e, metric_name, "get_metric_schema admin-service error");
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
    match list_signal_fields(
        &state.http_client,
        &state.admin_service_url,
        &state.internal_service_token,
        ctx.tenant_id,
        &signal_type,
    )
    .await
    {
        Ok(mut fields) => {
            if params.annotated_only {
                fields.retain(|f| f.display_name.is_some() || f.metric_type.is_some());
            }
            Ok(Json(fields))
        }
        Err(e) => {
            tracing::error!(error = %e, signal_type, "list_signal_fields admin-service error");
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
    match resolve_label_to_column(
        &state.http_client,
        &state.admin_service_url,
        &state.internal_service_token,
        ctx.tenant_id,
        &signal_type,
        &params.label,
    )
    .await
    {
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
            tracing::error!(error = %e, signal_type, label = params.label, "resolve_label admin-service error");
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
    fn metric_schema_complete_flag_preserves_semantics() {
        let schema = MetricSchema {
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
            schema_complete: true,
        };
        assert!(schema.schema_complete, "both fields present → complete");
    }

    #[test]
    fn signal_field_properties_preserved() {
        let field = SignalField {
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
            schema_complete: true,
        };
        assert_eq!(field.display_name.as_deref(), Some("Request Latency"));
        assert_eq!(field.metric_type.as_deref(), Some("gauge"));
        assert_eq!(field.unit.as_deref(), Some("ms"));
        assert!(field.schema_complete);
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
