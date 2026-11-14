//! Phase 2 P1 health assessment & data-quality insights:
//! - OBS-03-05: Per-region, channel, and customer-segment health breakdown
//! - OBS-03-06: Recurring degradation detection over a selected period
//! - OBS-03-07: Dependency contribution explanation (critical_path vs failover_redundant/non_blocking)
//! - OBS-03-08: Shared infrastructure/network health alongside supported services
//! - OBS-26-03: Evidence source, coverage, and confidence limitation qualifications
//! - OBS-26-05: Monthly review of unresolved data-quality issues and affected services
//! - OBS-26-06: Sampled/partial rare-event estimate disclosure (`estimated: true`)

#![allow(clippy::result_large_err)]

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;
use uuid::Uuid;

use crate::HealthState;
use crate::guard::caller;

fn err_resp(status: StatusCode, code: &str) -> Response {
    (status, Json(json!({ "error": code }))).into_response()
}

fn db_err<E: std::fmt::Display>(err: E) -> Response {
    tracing::error!(error = %err, "health p1 insights db error");
    err_resp(StatusCode::INTERNAL_SERVER_ERROR, "internal")
}

// ---------------------------------------------------------------------------
// OBS-03-05: Operating regions, channels, and customer segments
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct UpsertSegmentRequest {
    pub dimension: String,
    pub segment_value: String,
    pub health_state: String,
    #[serde(default)]
    pub incident_summary: String,
}

#[derive(FromRow, Serialize)]
pub struct SegmentAssessmentRow {
    pub dimension: String,
    pub segment_value: String,
    pub health_state: String,
    pub incident_summary: String,
    pub updated_at: DateTime<Utc>,
}

/// `PUT /v1/health/services/{id}/segments` — records regional/channel/segment health (OBS-03-05).
pub async fn put_segment_assessment(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(service_id): Path<Uuid>,
    Json(body): Json<UpsertSegmentRequest>,
) -> Result<Response, Response> {
    let caller = caller(&state, &headers).await?;
    if !matches!(body.dimension.as_str(), "region" | "channel" | "segment") {
        return Err(err_resp(StatusCode::BAD_REQUEST, "invalid_dimension"));
    }
    if !matches!(
        body.health_state.as_str(),
        "normal" | "degraded" | "unavailable" | "unknown" | "maintenance"
    ) {
        return Err(err_resp(StatusCode::BAD_REQUEST, "invalid_health_state"));
    }
    let seg_val = body.segment_value.trim();
    if seg_val.is_empty() {
        return Err(err_resp(StatusCode::BAD_REQUEST, "empty_segment_value"));
    }
    let row = sqlx::query_as::<_, SegmentAssessmentRow>(
        "INSERT INTO health.segment_assessments \
         (tenant_id, service_id, dimension, segment_value, health_state, incident_summary, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, now()) \
         ON CONFLICT (tenant_id, service_id, dimension, segment_value) DO UPDATE SET \
             health_state = EXCLUDED.health_state, \
             incident_summary = EXCLUDED.incident_summary, \
             updated_at = now() \
         RETURNING dimension, segment_value, health_state, incident_summary, updated_at",
    )
    .bind(caller.tenant_id)
    .bind(service_id)
    .bind(&body.dimension)
    .bind(seg_val)
    .bind(&body.health_state)
    .bind(body.incident_summary.trim())
    .fetch_one(&state.db)
    .await
    .map_err(db_err)?;

    Ok((StatusCode::OK, Json(row)).into_response())
}

/// `GET /v1/health/services/{id}/segments` — lists segment assessments without labeling
/// unaffected segments as affected (OBS-03-05).
pub async fn get_segment_assessments(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(service_id): Path<Uuid>,
) -> Result<Response, Response> {
    let caller = caller(&state, &headers).await?;
    let items = sqlx::query_as::<_, SegmentAssessmentRow>(
        "SELECT dimension, segment_value, health_state, incident_summary, updated_at \
         FROM health.segment_assessments \
         WHERE tenant_id = $1 AND service_id = $2 \
         ORDER BY dimension, segment_value",
    )
    .bind(caller.tenant_id)
    .bind(service_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    Ok(Json(json!({ "service_id": service_id, "items": items })).into_response())
}

// ---------------------------------------------------------------------------
// OBS-03-06: Recurring health problems over a selected period
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RecurringQuery {
    #[serde(default = "default_days")]
    pub days: i64,
}

fn default_days() -> i64 {
    30
}

#[derive(FromRow, Serialize)]
pub struct DegradationEpisode {
    pub at: DateTime<Utc>,
    pub from_state: String,
    pub to_state: String,
}

/// `GET /v1/health/services/{id}/recurring?days=30` — identifies repeated degradations (OBS-03-06).
pub async fn get_recurring_degradations(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(service_id): Path<Uuid>,
    Query(q): Query<RecurringQuery>,
) -> Result<Response, Response> {
    let caller = caller(&state, &headers).await?;
    let days = q.days.clamp(1, 365);
    let since = Utc::now() - Duration::days(days);
    let episodes = sqlx::query_as::<_, DegradationEpisode>(
        "SELECT at, from_state, to_state FROM health.transitions \
         WHERE tenant_id = $1 AND service_id = $2 AND at >= $3 \
           AND to_state IN ('degraded', 'unavailable') \
         ORDER BY at DESC",
    )
    .bind(caller.tenant_id)
    .bind(service_id)
    .bind(since)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    let degradation_count = episodes.len();
    Ok(Json(json!({
        "service_id": service_id,
        "window_days": days,
        "degradation_count": degradation_count,
        "recurring": degradation_count >= 2,
        "episodes": episodes,
    }))
    .into_response())
}

// ---------------------------------------------------------------------------
// OBS-03-07: Dependency contribution rules & explanation
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct UpsertDependencyRule {
    pub dependency_name: String,
    pub contribution_mode: String,
    #[serde(default)]
    pub rationale: String,
}

#[derive(FromRow, Serialize)]
pub struct DependencyRuleRow {
    pub dependency_name: String,
    pub contribution_mode: String,
    pub rationale: String,
    pub updated_at: DateTime<Utc>,
}

/// `PUT /v1/health/services/{id}/dependencies/explain` — defines how a dependency affects service health (OBS-03-07).
pub async fn put_dependency_rule(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(service_id): Path<Uuid>,
    Json(body): Json<UpsertDependencyRule>,
) -> Result<Response, Response> {
    let caller = caller(&state, &headers).await?;
    if !matches!(
        body.contribution_mode.as_str(),
        "critical_path" | "failover_redundant" | "non_blocking"
    ) {
        return Err(err_resp(
            StatusCode::BAD_REQUEST,
            "invalid_contribution_mode",
        ));
    }
    let dep = body.dependency_name.trim();
    if dep.is_empty() {
        return Err(err_resp(StatusCode::BAD_REQUEST, "empty_dependency_name"));
    }
    let row = sqlx::query_as::<_, DependencyRuleRow>(
        "INSERT INTO health.dependency_contribution_rules \
         (tenant_id, service_id, dependency_name, contribution_mode, rationale, updated_by, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, now()) \
         ON CONFLICT (tenant_id, service_id, dependency_name) DO UPDATE SET \
             contribution_mode = EXCLUDED.contribution_mode, \
             rationale = EXCLUDED.rationale, \
             updated_by = EXCLUDED.updated_by, \
             updated_at = now() \
         RETURNING dependency_name, contribution_mode, rationale, updated_at",
    )
    .bind(caller.tenant_id)
    .bind(service_id)
    .bind(dep)
    .bind(&body.contribution_mode)
    .bind(body.rationale.trim())
    .bind(caller.user_id)
    .fetch_one(&state.db)
    .await
    .map_err(db_err)?;

    Ok(Json(row).into_response())
}

/// `GET /v1/health/services/{id}/dependencies/explain` — explains whether each dependency
/// degrades the business service (OBS-03-07).
pub async fn get_dependency_explanation(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(service_id): Path<Uuid>,
) -> Result<Response, Response> {
    let caller = caller(&state, &headers).await?;
    let rows = sqlx::query_as::<_, DependencyRuleRow>(
        "SELECT dependency_name, contribution_mode, rationale, updated_at \
         FROM health.dependency_contribution_rules \
         WHERE tenant_id = $1 AND service_id = $2 \
         ORDER BY dependency_name",
    )
    .bind(caller.tenant_id)
    .bind(service_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    let items: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let impacts = r.contribution_mode == "critical_path";
            json!({
                "dependency_name": r.dependency_name,
                "contribution_mode": r.contribution_mode,
                "impacts_service_health": impacts,
                "rationale": r.rationale,
                "updated_at": r.updated_at,
            })
        })
        .collect();

    Ok(Json(json!({ "service_id": service_id, "items": items })).into_response())
}

// ---------------------------------------------------------------------------
// OBS-03-08: Shared platforms, networks, and infrastructure health
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct UpsertInfraHealth {
    pub infra_id: String,
    pub infra_kind: String,
    pub health_state: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub supported_service_ids: Vec<Uuid>,
}

#[derive(FromRow, Serialize)]
pub struct InfraHealthRow {
    pub infra_id: String,
    pub infra_kind: String,
    pub health_state: String,
    pub detail: String,
    pub supported_service_ids: Vec<Uuid>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct InfraQuery {
    pub service_id: Option<Uuid>,
}

/// `PUT /v1/health/infrastructure` — records shared platform/network health (OBS-03-08).
pub async fn put_shared_infrastructure(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Json(body): Json<UpsertInfraHealth>,
) -> Result<Response, Response> {
    let caller = caller(&state, &headers).await?;
    if !matches!(
        body.infra_kind.as_str(),
        "network" | "platform" | "compute" | "database" | "queue"
    ) {
        return Err(err_resp(StatusCode::BAD_REQUEST, "invalid_infra_kind"));
    }
    if !matches!(
        body.health_state.as_str(),
        "normal" | "degraded" | "unavailable" | "unknown" | "maintenance"
    ) {
        return Err(err_resp(StatusCode::BAD_REQUEST, "invalid_health_state"));
    }
    let id = body.infra_id.trim();
    if id.is_empty() {
        return Err(err_resp(StatusCode::BAD_REQUEST, "empty_infra_id"));
    }
    let row = sqlx::query_as::<_, InfraHealthRow>(
        "INSERT INTO health.shared_infrastructure_health \
         (tenant_id, infra_id, infra_kind, health_state, detail, supported_service_ids, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, now()) \
         ON CONFLICT (tenant_id, infra_id) DO UPDATE SET \
             infra_kind = EXCLUDED.infra_kind, \
             health_state = EXCLUDED.health_state, \
             detail = EXCLUDED.detail, \
             supported_service_ids = EXCLUDED.supported_service_ids, \
             updated_at = now() \
         RETURNING infra_id, infra_kind, health_state, detail, supported_service_ids, updated_at",
    )
    .bind(caller.tenant_id)
    .bind(id)
    .bind(&body.infra_kind)
    .bind(&body.health_state)
    .bind(body.detail.trim())
    .bind(&body.supported_service_ids)
    .fetch_one(&state.db)
    .await
    .map_err(db_err)?;

    Ok(Json(row).into_response())
}

/// `GET /v1/health/infrastructure?service_id=...` — lists shared infrastructure health alongside
/// the business services it supports (OBS-03-08).
pub async fn get_shared_infrastructure(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Query(q): Query<InfraQuery>,
) -> Result<Response, Response> {
    let caller = caller(&state, &headers).await?;
    let items = sqlx::query_as::<_, InfraHealthRow>(
        "SELECT infra_id, infra_kind, health_state, detail, supported_service_ids, updated_at \
         FROM health.shared_infrastructure_health \
         WHERE tenant_id = $1 AND ($2::uuid IS NULL OR $2 = ANY(supported_service_ids)) \
         ORDER BY infra_id",
    )
    .bind(caller.tenant_id)
    .bind(q.service_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    Ok(Json(json!({ "items": items })).into_response())
}

// ---------------------------------------------------------------------------
// OBS-26-03: Evidence source, coverage, and confidence qualifications
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RecordQualificationRequest {
    pub finding_id: String,
    pub evidence_source: String,
    pub coverage_pct: f64,
    pub confidence_level: String,
    #[serde(default)]
    pub limitations: String,
}

#[derive(FromRow, Serialize)]
pub struct QualificationRow {
    pub id: Uuid,
    pub finding_id: String,
    pub evidence_source: String,
    pub coverage_pct: f64,
    pub confidence_level: String,
    pub limitations: String,
    pub recorded_at: DateTime<Utc>,
}

/// `POST /v1/health/services/{id}/qualifications` — attaches source & coverage qualifications to a finding (OBS-26-03).
pub async fn record_qualification(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(service_id): Path<Uuid>,
    Json(body): Json<RecordQualificationRequest>,
) -> Result<Response, Response> {
    let caller = caller(&state, &headers).await?;
    if !matches!(body.confidence_level.as_str(), "high" | "medium" | "low") {
        return Err(err_resp(
            StatusCode::BAD_REQUEST,
            "invalid_confidence_level",
        ));
    }
    if !(0.0..=100.0).contains(&body.coverage_pct) {
        return Err(err_resp(StatusCode::BAD_REQUEST, "invalid_coverage_pct"));
    }
    let row = sqlx::query_as::<_, QualificationRow>(
        "INSERT INTO health.evidence_qualifications \
         (tenant_id, service_id, finding_id, evidence_source, coverage_pct, confidence_level, limitations) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         RETURNING id, finding_id, evidence_source, coverage_pct, confidence_level, limitations, recorded_at",
    )
    .bind(caller.tenant_id)
    .bind(service_id)
    .bind(body.finding_id.trim())
    .bind(body.evidence_source.trim())
    .bind(body.coverage_pct)
    .bind(&body.confidence_level)
    .bind(body.limitations.trim())
    .fetch_one(&state.db)
    .await
    .map_err(db_err)?;

    Ok((StatusCode::CREATED, Json(row)).into_response())
}

/// `GET /v1/health/services/{id}/qualifications` — traces findings to declared evidence sources and qualifications (OBS-26-03).
pub async fn get_qualifications(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(service_id): Path<Uuid>,
) -> Result<Response, Response> {
    let caller = caller(&state, &headers).await?;
    let items = sqlx::query_as::<_, QualificationRow>(
        "SELECT id, finding_id, evidence_source, coverage_pct, confidence_level, limitations, recorded_at \
         FROM health.evidence_qualifications \
         WHERE tenant_id = $1 AND service_id = $2 \
         ORDER BY recorded_at DESC",
    )
    .bind(caller.tenant_id)
    .bind(service_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    Ok(Json(json!({ "service_id": service_id, "items": items })).into_response())
}

// ---------------------------------------------------------------------------
// OBS-26-05: Monthly review of unresolved data-quality issues
// ---------------------------------------------------------------------------

#[derive(FromRow, Serialize)]
pub struct UnresolvedQualityIssueItem {
    pub id: Uuid,
    pub service_id: Uuid,
    pub environment_class: String,
    pub kind: String,
    pub service_name: String,
    pub environment: String,
    pub opened_at: DateTime<Utc>,
    pub detail: Value,
}

/// `GET /v1/health/issues/monthly-review` — identifies unresolved quality issues and affected services (OBS-26-05).
pub async fn monthly_quality_review(
    State(state): State<HealthState>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    let caller = caller(&state, &headers).await?;
    let items = sqlx::query_as::<_, UnresolvedQualityIssueItem>(
        "SELECT id, service_id, environment_class, kind, service_name, environment, opened_at, detail \
         FROM health.data_quality_issues \
         WHERE tenant_id = $1 AND resolved_at IS NULL \
         ORDER BY opened_at ASC",
    )
    .bind(caller.tenant_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    let affected_services: Vec<Uuid> = items
        .iter()
        .map(|i| i.service_id)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    Ok(Json(json!({
        "unresolved_count": items.len(),
        "affected_service_ids": affected_services,
        "items": items,
    }))
    .into_response())
}

// ---------------------------------------------------------------------------
// OBS-26-06: Sampled / partial evidence disclosure
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SampledEstimateRequest {
    pub observed_count: u64,
    pub sampling_rate: f64,
}

/// `POST /v1/health/services/{id}/sampled-estimate` — labels rare-event counts derived from
/// sampled evidence as `estimated` (OBS-26-06).
pub async fn evaluate_sampled_estimate(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(service_id): Path<Uuid>,
    Json(body): Json<SampledEstimateRequest>,
) -> Result<Response, Response> {
    let _caller = caller(&state, &headers).await?;
    if !body.sampling_rate.is_finite() || body.sampling_rate <= 0.0 || body.sampling_rate > 1.0 {
        return Err(err_resp(StatusCode::BAD_REQUEST, "invalid_sampling_rate"));
    }
    let estimated = body.sampling_rate < 1.0;
    let estimated_total = if estimated {
        (body.observed_count as f64 / body.sampling_rate).round() as u64
    } else {
        body.observed_count
    };
    Ok(Json(json!({
        "service_id": service_id,
        "observed_count": body.observed_count,
        "sampling_rate": body.sampling_rate,
        "estimated_total": estimated_total,
        "estimated": estimated,
        "label": if estimated { "estimated" } else { "exact" },
    }))
    .into_response())
}

// ---------------------------------------------------------------------------
// OBS-04-04: Performance regression segmentation by approved business dimension
// ---------------------------------------------------------------------------

const APPROVED_PERF_DIMENSIONS: &[&str] = &["customer_group", "transaction", "operating_period"];

#[derive(Deserialize)]
pub struct PerformanceSample {
    pub segment_value: String,
    pub baseline_p95_ms: f64,
    pub observed_p95_ms: f64,
}

#[derive(Deserialize)]
pub struct SegmentPerformanceRequest {
    pub dimension: String,
    pub samples: Vec<PerformanceSample>,
}

/// `POST /v1/health/services/{id}/performance-segments` — segments a performance regression
/// by an approved business dimension (OBS-04-04).
pub async fn segment_performance_regression(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(service_id): Path<Uuid>,
    Json(body): Json<SegmentPerformanceRequest>,
) -> Result<Response, Response> {
    let _caller = caller(&state, &headers).await?;
    if !APPROVED_PERF_DIMENSIONS.contains(&body.dimension.as_str()) {
        return Err(err_resp(StatusCode::BAD_REQUEST, "unapproved_dimension"));
    }
    let mut regressed: Vec<Value> = body
        .samples
        .iter()
        .map(|s| {
            let delta_ms = s.observed_p95_ms - s.baseline_p95_ms;
            let ratio = if s.baseline_p95_ms > 0.0 {
                s.observed_p95_ms / s.baseline_p95_ms
            } else {
                1.0
            };
            json!({
                "segment_value": s.segment_value,
                "baseline_p95_ms": s.baseline_p95_ms,
                "observed_p95_ms": s.observed_p95_ms,
                "delta_ms": delta_ms,
                "regressed": ratio >= 1.25,
            })
        })
        .collect();
    regressed.sort_by(|a, b| {
        let da = a["delta_ms"].as_f64().unwrap_or(0.0);
        let db = b["delta_ms"].as_f64().unwrap_or(0.0);
        db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(Json(json!({
        "service_id": service_id,
        "dimension": body.dimension,
        "segments": regressed,
    }))
    .into_response())
}

// ---------------------------------------------------------------------------
// OBS-04-05: Capacity readiness review before expected business peaks
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CapacityReadinessRequest {
    pub peak_event: String,
    pub forecast_demand_rps: f64,
    pub provisioned_capacity_rps: f64,
    pub accountable_owner: String,
    pub accountable_action: String,
}

/// `POST /v1/health/services/{id}/capacity-readiness` — evaluates forecast demand against
/// provisioned capacity and returns risk and accountable action (OBS-04-05).
pub async fn evaluate_capacity_readiness(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(service_id): Path<Uuid>,
    Json(body): Json<CapacityReadinessRequest>,
) -> Result<Response, Response> {
    let _caller = caller(&state, &headers).await?;
    if body.provisioned_capacity_rps <= 0.0 || body.forecast_demand_rps < 0.0 {
        return Err(err_resp(StatusCode::BAD_REQUEST, "invalid_capacity_inputs"));
    }
    let utilization = body.forecast_demand_rps / body.provisioned_capacity_rps;
    let risk = if utilization >= 1.0 {
        "high"
    } else if utilization >= 0.8 {
        "elevated"
    } else {
        "low"
    };
    if risk != "low" && body.accountable_action.trim().is_empty() {
        return Err(err_resp(
            StatusCode::UNPROCESSABLE_ENTITY,
            "accountable_action_required_for_capacity_risk",
        ));
    }
    Ok(Json(json!({
        "service_id": service_id,
        "peak_event": body.peak_event,
        "forecast_demand_rps": body.forecast_demand_rps,
        "provisioned_capacity_rps": body.provisioned_capacity_rps,
        "forecast_utilization": utilization,
        "risk": risk,
        "accountable_owner": body.accountable_owner,
        "accountable_action": body.accountable_action,
    }))
    .into_response())
}

// ---------------------------------------------------------------------------
// OBS-04-07: Resource limit, quota, and consumption ceiling warnings
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct LimitWarningRequest {
    pub resource_name: String,
    pub current_usage: f64,
    pub approved_limit: f64,
    #[serde(default = "default_warning_ratio")]
    pub warning_threshold_ratio: f64,
    pub accountable_owner: String,
}

fn default_warning_ratio() -> f64 {
    0.8
}

/// `POST /v1/health/services/{id}/limit-warnings` — warns accountable owner when controlled
/// approach to an approved limit crosses the warning threshold before exhaustion (OBS-04-07).
pub async fn evaluate_limit_warning(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(service_id): Path<Uuid>,
    Json(body): Json<LimitWarningRequest>,
) -> Result<Response, Response> {
    let _caller = caller(&state, &headers).await?;
    if body.approved_limit <= 0.0
        || body.current_usage < 0.0
        || !(0.0..1.0).contains(&body.warning_threshold_ratio)
    {
        return Err(err_resp(StatusCode::BAD_REQUEST, "invalid_limit_inputs"));
    }
    let ratio = body.current_usage / body.approved_limit;
    let status = if ratio >= 1.0 {
        "exhausted"
    } else if ratio >= body.warning_threshold_ratio {
        "approaching_limit"
    } else {
        "within_limit"
    };
    Ok(Json(json!({
        "service_id": service_id,
        "resource_name": body.resource_name,
        "utilization_ratio": ratio,
        "status": status,
        "warn_owner": status != "within_limit",
        "accountable_owner": body.accountable_owner,
    }))
    .into_response())
}

// ---------------------------------------------------------------------------
// OBS-04-08: Component, operation, and resource performance attribution
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AttributionCandidate {
    pub component: String,
    pub operation: String,
    pub resource: String,
    pub duration_ms: f64,
}

#[derive(Deserialize)]
pub struct PerformanceAttributionRequest {
    pub trace_or_span_id: String,
    pub candidates: Vec<AttributionCandidate>,
}

/// `POST /v1/health/services/{id}/performance-attribution` — attributes a slow operation
/// to the specific component and resource it consumes most (OBS-04-08).
pub async fn attribute_performance_bottleneck(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(service_id): Path<Uuid>,
    Json(body): Json<PerformanceAttributionRequest>,
) -> Result<Response, Response> {
    let _caller = caller(&state, &headers).await?;
    if body.candidates.is_empty() {
        return Err(err_resp(StatusCode::BAD_REQUEST, "no_candidates"));
    }
    let total_ms: f64 = body.candidates.iter().map(|c| c.duration_ms.max(0.0)).sum();
    let mut ranked: Vec<&AttributionCandidate> = body.candidates.iter().collect();
    ranked.sort_by(|a, b| {
        b.duration_ms
            .partial_cmp(&a.duration_ms)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let primary = ranked[0];
    let share = if total_ms > 0.0 {
        primary.duration_ms / total_ms
    } else {
        0.0
    };
    Ok(Json(json!({
        "service_id": service_id,
        "trace_or_span_id": body.trace_or_span_id,
        "primary_component": primary.component,
        "primary_operation": primary.operation,
        "dominant_resource": primary.resource,
        "duration_ms": primary.duration_ms,
        "contribution_share": share,
    }))
    .into_response())
}
