// Internal, service-to-service-only endpoints (gated by
// `middleware::auth::require_internal_service`, not `require_tenant` --
// there is no end-user credential on these calls, only a trusted caller
// within the cluster). Callers pass `tenant_id` explicitly since there is no
// per-user session/API-key context to derive it from.
//
// These carry the `deployment_markers` (control-owned) half of the joins
// query-api's discovery.rs/reliability.rs need. Phase 6 "consolidate alerting"
// (docs/component-decomposition.md) split the original combined endpoints by
// owner: the `slo_definitions`/`alert_rules`/`alert_firings`/`incidents` joins
// moved to alert-evaluator's `/internal/alerting-*` endpoints; only the
// deployment queries stay here. query-api calls both and merges.

use crate::AdminServiceAppState;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct DeploymentEnrichmentParams {
    pub tenant_id: Uuid,
    pub environment: Option<String>,
}

#[derive(Serialize)]
pub struct DeploymentEnrichmentItem {
    pub service_name: String,
    pub latest_deployment: Option<String>,
}

#[derive(Serialize)]
pub struct DeploymentEnrichmentResponse {
    pub items: Vec<DeploymentEnrichmentItem>,
}

/// GET /internal/deployment-enrichment
///
/// Latest deployment version per service. Moved verbatim from the
/// deployment half of the former `handle_service_catalog_enrichment`.
pub async fn handle_deployment_enrichment(
    State(state): State<AdminServiceAppState>,
    Query(params): Query<DeploymentEnrichmentParams>,
) -> Result<Json<DeploymentEnrichmentResponse>, StatusCode> {
    #[derive(sqlx::FromRow)]
    struct LatestDeploymentRow {
        service_name: String,
        service_version: String,
    }

    let deployment_rows: Vec<LatestDeploymentRow> = sqlx::query_as(
        "SELECT DISTINCT ON (service_name) service_name, service_version \
         FROM deployment_markers \
         WHERE tenant_id = $1 \
           AND ($2::TEXT IS NULL OR environment = $2) \
         ORDER BY service_name, started_at DESC",
    )
    .bind(params.tenant_id)
    .bind(&params.environment)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to fetch latest deployments");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let items = deployment_rows
        .into_iter()
        .map(|row| DeploymentEnrichmentItem {
            service_name: row.service_name,
            latest_deployment: Some(row.service_version),
        })
        .collect();

    Ok(Json(DeploymentEnrichmentResponse { items }))
}

#[derive(Deserialize)]
pub struct DeploymentCorrelationParams {
    pub tenant_id: Uuid,
    pub service_name: String,
    pub environment: Option<String>,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ReliabilityDeploymentRow {
    pub deployment_id: Uuid,
    pub tenant_id: Uuid,
    pub project_id: Option<Uuid>,
    pub service_name: String,
    pub environment: String,
    pub service_version: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub deployed_by: Option<String>,
    pub commit_sha: Option<String>,
    pub rollback_of: Option<Uuid>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct DeploymentCorrelationResponse {
    pub deployments: Vec<ReliabilityDeploymentRow>,
}

/// GET /internal/deployment-correlation
///
/// Deployments for a service/environment/interval. Moved verbatim from the
/// deployment query of the former `handle_reliability_correlation`.
pub async fn handle_deployment_correlation(
    State(state): State<AdminServiceAppState>,
    Query(params): Query<DeploymentCorrelationParams>,
) -> Result<Json<DeploymentCorrelationResponse>, StatusCode> {
    let deployments: Vec<ReliabilityDeploymentRow> = sqlx::query_as(
        "SELECT deployment_id, tenant_id, project_id, service_name, environment, \
                service_version, status, started_at, finished_at, deployed_by, \
                commit_sha, rollback_of, metadata \
         FROM deployment_markers \
         WHERE tenant_id = $1 \
           AND service_name = $2 \
           AND started_at <= $4 \
           AND (finished_at IS NULL OR finished_at >= $3) \
           AND ($5::TEXT IS NULL OR environment = $5) \
         ORDER BY started_at DESC",
    )
    .bind(params.tenant_id)
    .bind(&params.service_name)
    .bind(params.from)
    .bind(params.to)
    .bind(&params.environment)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to fetch reliability-correlated deployments");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(DeploymentCorrelationResponse { deployments }))
}

// ── NLQ Schema Registry & Platform Config Endpoints ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalLlmConfigResponse {
    pub api_key: Option<String>,
    pub url: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub webllm_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricSchema {
    pub field_name: String,
    pub field_type: String,
    pub otel_spec_version: Option<String>,
    pub display_name: Option<String>,
    pub business_description: Option<String>,
    pub interpretation_rule: Option<String>,
    pub effective_sample_rate: Option<f64>,
    pub not_for_billing: Option<bool>,
    pub metric_type: Option<String>,
    pub timestamp_column: Option<String>,
    pub unit: Option<String>,
    pub recommended_downsampling: Option<String>,
    pub schema_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalField {
    pub field_name: String,
    pub field_type: String,
    pub otel_spec_version: Option<String>,
    pub display_name: Option<String>,
    pub business_description: Option<String>,
    pub interpretation_rule: Option<String>,
    pub effective_sample_rate: Option<f64>,
    pub metric_type: Option<String>,
    pub timestamp_column: Option<String>,
    pub unit: Option<String>,
    pub recommended_downsampling: Option<String>,
    pub schema_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolveLabelResponse {
    pub field_name: Option<String>,
    pub ambiguous: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub candidates: Vec<String>,
}

#[derive(Deserialize)]
pub struct TenantQuery {
    pub tenant_id: Uuid,
}

#[derive(Deserialize)]
pub struct ResolveLabelQuery {
    pub tenant_id: Uuid,
    pub label: String,
}

#[derive(sqlx::FromRow)]
struct MetricSchemaRow {
    field_name: String,
    field_type: String,
    otel_spec_version: Option<String>,
    display_name: Option<String>,
    business_description: Option<String>,
    interpretation_rule: Option<String>,
    effective_sample_rate: Option<f64>,
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

/// GET /internal/config/llm
pub async fn handle_get_llm_config(
    State(state): State<AdminServiceAppState>,
) -> Result<Json<InternalLlmConfigResponse>, StatusCode> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT key, value FROM platform_config WHERE key IN ('llm_api_key', 'llm_url', 'llm_model', 'llm_provider', 'webllm_model')",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to fetch platform_config for LLM");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut api_key = None;
    let mut url = None;
    let mut model = None;
    let mut provider = None;
    let mut webllm_model = None;

    for (k, v) in rows {
        if v.is_empty() {
            continue;
        }
        match k.as_str() {
            "llm_api_key" => {
                let deobfuscated = crate::config::xor_deobfuscate(&v).unwrap_or(v);
                if !deobfuscated.is_empty() {
                    api_key = Some(deobfuscated);
                }
            }
            "llm_url" => url = Some(v),
            "llm_model" => model = Some(v),
            "llm_provider" => provider = Some(v),
            "webllm_model" => webllm_model = Some(v),
            _ => {}
        }
    }

    Ok(Json(InternalLlmConfigResponse {
        api_key,
        url,
        model,
        provider,
        webllm_model,
    }))
}

/// GET /internal/schemas/:signal_type/fields?tenant_id=...
pub async fn handle_list_signal_fields(
    State(state): State<AdminServiceAppState>,
    Path(signal_type): Path<String>,
    Query(params): Query<TenantQuery>,
) -> Result<Json<Vec<SignalField>>, StatusCode> {
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
    .bind(&signal_type)
    .bind(params.tenant_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, signal_type = %signal_type, "failed to list signal fields");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(rows.into_iter().map(SignalField::from).collect()))
}

/// GET /internal/schemas/metrics/schema/:metric_name?tenant_id=...
pub async fn handle_get_metric_schema(
    State(state): State<AdminServiceAppState>,
    Path(metric_name): Path<String>,
    Query(params): Query<TenantQuery>,
) -> Result<Json<Option<MetricSchema>>, StatusCode> {
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
    .bind(params.tenant_id)
    .bind(&metric_name)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, metric_name = %metric_name, "failed to get metric schema");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(row.map(MetricSchema::from)))
}

/// GET /internal/schemas/:signal_type/resolve-label?tenant_id=...&label=...
pub async fn handle_resolve_label(
    State(state): State<AdminServiceAppState>,
    Path(signal_type): Path<String>,
    Query(params): Query<ResolveLabelQuery>,
) -> Result<Json<ResolveLabelResponse>, StatusCode> {
    // Step 1: exact field_name match
    let exact: Option<String> = sqlx::query_scalar(
        "SELECT field_name FROM schema_entries \
         WHERE signal_type = $1 AND field_name = $2",
    )
    .bind(&signal_type)
    .bind(&params.label)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed checking exact field_name match");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if let Some(field) = exact {
        return Ok(Json(ResolveLabelResponse {
            field_name: Some(field),
            ambiguous: false,
            candidates: vec![],
        }));
    }

    // Step 2: case-insensitive display_name match
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
    .bind(&signal_type)
    .bind(&params.label)
    .bind(params.tenant_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed checking display_name match");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match candidates.len() {
        0 => Ok(Json(ResolveLabelResponse {
            field_name: None,
            ambiguous: false,
            candidates: vec![],
        })),
        1 => Ok(Json(ResolveLabelResponse {
            field_name: Some(candidates.into_iter().next().unwrap()),
            ambiguous: false,
            candidates: vec![],
        })),
        _ => Ok(Json(ResolveLabelResponse {
            field_name: None,
            ambiguous: true,
            candidates,
        })),
    }
}

// ── Query Audit Logging ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct RecordQueryAuditRequest {
    pub action: String,
    pub tenant_id: Uuid,
    pub result_count: i64,
}

/// POST /internal/query-audit
///
/// Records query audit entries in `control.query_audit_log` on behalf of
/// `query-api`.
pub async fn handle_record_query_audit(
    State(state): State<AdminServiceAppState>,
    Json(payload): Json<RecordQueryAuditRequest>,
) -> Result<StatusCode, StatusCode> {
    sqlx::query(
        "INSERT INTO query_audit_log (action, tenant_id, result_count) \
         VALUES ($1, $2, $3)",
    )
    .bind(&payload.action)
    .bind(payload.tenant_id)
    .bind(payload.result_count)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "failed to insert query audit log");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::NO_CONTENT)
}
