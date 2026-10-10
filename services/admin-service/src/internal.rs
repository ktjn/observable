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
    extract::{Query, State},
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
