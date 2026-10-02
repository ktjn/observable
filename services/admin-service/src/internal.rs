// Internal, service-to-service-only endpoints (gated by
// `middleware::auth::require_internal_service`, not `require_tenant` --
// there is no end-user credential on these calls, only a trusted caller
// within the cluster). Callers pass `tenant_id` explicitly since there is no
// per-user session/API-key context to derive it from.
//
// These exist so query-api's discovery.rs/reliability.rs can stop reading
// alert_rules/alert_firings/slo_definitions/deployment_markers/incidents
// directly via their own SQL (Phase 4's "remove cross-owner SQL" follow-on,
// docs/component-decomposition.md) without adding per-call network latency
// to the hot aggregation queries -- the joins still happen once, in
// Postgres, here; query-api gets back the already-joined result.

use crate::AdminServiceAppState;
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ServiceCatalogEnrichmentParams {
    pub tenant_id: Uuid,
    pub environment: Option<String>,
}

#[derive(Serialize)]
pub struct ServiceCatalogEnrichmentItem {
    pub service_name: String,
    pub active_alert_count: i64,
    pub slo_breaching: bool,
    pub latest_deployment: Option<String>,
}

#[derive(Serialize)]
pub struct ServiceCatalogEnrichmentResponse {
    pub items: Vec<ServiceCatalogEnrichmentItem>,
}

/// GET /internal/service-catalog-enrichment
///
/// Moved verbatim (same SQL) from query-api's
/// `discovery::fetch_service_catalog_enrichment`.
pub async fn handle_service_catalog_enrichment(
    State(state): State<AdminServiceAppState>,
    Query(params): Query<ServiceCatalogEnrichmentParams>,
) -> Result<Json<ServiceCatalogEnrichmentResponse>, StatusCode> {
    #[derive(sqlx::FromRow)]
    struct SloAlertRow {
        service_name: String,
        active_alert_count: i64,
        slo_breaching: bool,
    }

    let slo_rows: Vec<SloAlertRow> = sqlx::query_as(
        "SELECT sd.service_name, \
                COUNT(af.firing_id) FILTER (WHERE af.state = 'active') AS active_alert_count, \
                COALESCE(BOOL_OR(af.state = 'active'), false) AS slo_breaching \
         FROM slo_definitions sd \
         LEFT JOIN alert_rules ar \
             ON ar.tenant_id = sd.tenant_id \
            AND ar.alert_type = 'slo_burn_rate' \
            AND ar.condition->>'slo_id' = sd.slo_id::text \
         LEFT JOIN alert_firings af ON af.rule_id = ar.rule_id \
         WHERE sd.tenant_id = $1 \
           AND ($2::TEXT IS NULL OR sd.environment = $2) \
         GROUP BY sd.service_name",
    )
    .bind(params.tenant_id)
    .bind(&params.environment)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to fetch slo/alert enrichment");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut items: std::collections::HashMap<String, ServiceCatalogEnrichmentItem> = slo_rows
        .into_iter()
        .map(|row| {
            (
                row.service_name.clone(),
                ServiceCatalogEnrichmentItem {
                    service_name: row.service_name,
                    active_alert_count: row.active_alert_count.max(0),
                    slo_breaching: row.slo_breaching,
                    latest_deployment: None,
                },
            )
        })
        .collect();

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

    for row in deployment_rows {
        items
            .entry(row.service_name.clone())
            .or_insert(ServiceCatalogEnrichmentItem {
                service_name: row.service_name,
                active_alert_count: 0,
                slo_breaching: false,
                latest_deployment: None,
            })
            .latest_deployment = Some(row.service_version);
    }

    Ok(Json(ServiceCatalogEnrichmentResponse {
        items: items.into_values().collect(),
    }))
}

#[derive(Deserialize)]
pub struct ReliabilityCorrelationParams {
    pub tenant_id: Uuid,
    pub service_name: String,
    pub environment: Option<String>,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ReliabilityIncidentRow {
    pub incident_id: Uuid,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub triggered_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub triggered_by_rule_id: Option<Uuid>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ReliabilitySloRow {
    pub slo_id: Uuid,
    pub service_name: String,
    pub environment: String,
    pub sli_type: String,
    pub target: f64,
    pub window_days: i32,
    pub burn_rate_fast_threshold: f64,
    pub burn_rate_slow_threshold: f64,
    pub description: String,
    pub firing: bool,
    pub last_fired_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
pub struct ReliabilityCorrelationResponse {
    pub incidents: Vec<ReliabilityIncidentRow>,
    pub slos: Vec<ReliabilitySloRow>,
    pub deployments: Vec<ReliabilityDeploymentRow>,
}

/// GET /internal/reliability-correlation
///
/// Moved verbatim (same three SQL queries) from query-api's
/// `reliability::get_service_reliability_report`. Summary computation
/// (`compute_incident_summary`/`compute_slo_summary`/
/// `compute_deployment_summary`) stays in query-api -- that's presentation
/// logic over the correlated data, not data-ownership logic.
pub async fn handle_reliability_correlation(
    State(state): State<AdminServiceAppState>,
    Query(params): Query<ReliabilityCorrelationParams>,
) -> Result<Json<ReliabilityCorrelationResponse>, StatusCode> {
    let incidents: Vec<ReliabilityIncidentRow> = sqlx::query_as(
        "SELECT i.incident_id, i.title, i.severity, i.status, i.triggered_at, i.resolved_at, i.triggered_by_rule_id \
         FROM incidents i \
         LEFT JOIN alert_rules r ON i.triggered_by_rule_id = r.rule_id \
         LEFT JOIN slo_definitions s \
                ON r.alert_type = 'slo_burn_rate' \
               AND (r.condition->>'slo_id')::uuid = s.slo_id \
               AND s.tenant_id = i.tenant_id \
         WHERE i.tenant_id = $1 \
           AND s.service_name = $2 \
           AND i.triggered_at <= $4 \
           AND (i.resolved_at IS NULL OR i.resolved_at >= $3) \
           AND ($5::TEXT IS NULL OR s.environment = $5) \
         ORDER BY i.triggered_at DESC",
    )
    .bind(params.tenant_id)
    .bind(&params.service_name)
    .bind(params.from)
    .bind(params.to)
    .bind(&params.environment)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to fetch reliability-correlated incidents");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let slos: Vec<ReliabilitySloRow> = sqlx::query_as(
        "SELECT slo_id, service_name, environment, sli_type, target, window_days, \
                burn_rate_fast_threshold, burn_rate_slow_threshold, description, \
                EXISTS( \
                    SELECT 1 FROM alert_rules ar \
                    JOIN alert_firings af ON af.rule_id = ar.rule_id \
                    WHERE ar.tenant_id = slo_definitions.tenant_id \
                      AND ar.alert_type = 'slo_burn_rate' \
                      AND ar.condition->>'slo_id' = slo_definitions.slo_id::text \
                      AND af.state = 'active' \
                ) AS firing, \
                (SELECT MAX(af.occurred_at) FROM alert_rules ar \
                 JOIN alert_firings af ON af.rule_id = ar.rule_id \
                 WHERE ar.tenant_id = slo_definitions.tenant_id \
                   AND ar.alert_type = 'slo_burn_rate' \
                   AND ar.condition->>'slo_id' = slo_definitions.slo_id::text \
                   AND af.state = 'active') AS last_fired_at, \
                created_at, updated_at \
         FROM slo_definitions \
         WHERE tenant_id = $1 \
           AND service_name = $2 \
           AND ($3::TEXT IS NULL OR environment = $3) \
         ORDER BY updated_at DESC",
    )
    .bind(params.tenant_id)
    .bind(&params.service_name)
    .bind(&params.environment)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to fetch reliability-correlated slos");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

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

    Ok(Json(ReliabilityCorrelationResponse {
        incidents,
        slos,
        deployments,
    }))
}
