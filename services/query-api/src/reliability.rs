use crate::deployments::DeploymentMarker;
use crate::incidents::IncidentItem;
use crate::middleware::auth::TenantContext;
use crate::slos::SloDefinitionItem;
use crate::traces::AppState;
use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ReliabilityReportQuery {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub environment: Option<String>,
}

#[derive(Serialize)]
pub struct IncidentSummary {
    pub total: usize,
    pub open: usize,
    pub resolved: usize,
    pub mean_time_to_resolve_minutes: Option<f64>,
}

#[derive(Serialize)]
pub struct SloSummary {
    pub total: usize,
    pub firing: usize,
}

#[derive(Serialize)]
pub struct DeploymentSummary {
    pub total: usize,
}

#[derive(Serialize)]
pub struct ReliabilityReportResponse {
    pub service_name: String,
    pub environment: Option<String>,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub incident_summary: IncidentSummary,
    pub slo_summary: SloSummary,
    pub deployment_summary: DeploymentSummary,
    pub incidents: Vec<IncidentItem>,
    pub slos: Vec<SloDefinitionItem>,
    pub deployments: Vec<DeploymentMarker>,
}

#[derive(sqlx::FromRow)]
struct ReliabilityIncidentRow {
    incident_id: Uuid,
    title: String,
    severity: String,
    status: String,
    triggered_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    triggered_by_rule_id: Option<Uuid>,
}

fn compute_incident_summary(
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    incidents: &[IncidentItem],
) -> IncidentSummary {
    let total = incidents.len();
    let open = incidents
        .iter()
        .filter(|incident| incident.status != "resolved")
        .count();
    let resolved = incidents
        .iter()
        .filter(|incident| incident.resolved_at.is_some())
        .count();

    let mut total_minutes = 0.0;
    let mut resolved_in_window = 0usize;
    for incident in incidents {
        let Some(resolved_at) = incident.resolved_at else {
            continue;
        };
        if resolved_at < from || resolved_at > to {
            continue;
        }
        resolved_in_window += 1;
        total_minutes += (resolved_at - incident.triggered_at).num_seconds() as f64 / 60.0;
    }

    IncidentSummary {
        total,
        open,
        resolved,
        mean_time_to_resolve_minutes: if resolved_in_window > 0 {
            Some(total_minutes / resolved_in_window as f64)
        } else {
            None
        },
    }
}

fn compute_slo_summary(slos: &[SloDefinitionItem]) -> SloSummary {
    SloSummary {
        total: slos.len(),
        firing: slos.iter().filter(|slo| slo.firing).count(),
    }
}

fn compute_deployment_summary(deployments: &[DeploymentMarker]) -> DeploymentSummary {
    DeploymentSummary {
        total: deployments.len(),
    }
}

pub async fn get_service_reliability_report(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    service_name: &str,
    query: &ReliabilityReportQuery,
) -> Result<Option<ReliabilityReportResponse>, sqlx::Error> {
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
    .bind(tenant_id)
    .bind(service_name)
    .bind(query.from)
    .bind(query.to)
    .bind(query.environment.as_deref())
    .fetch_all(db)
    .await?;

    let slos: Vec<SloDefinitionItem> = sqlx::query_as(
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
    .bind(tenant_id)
    .bind(service_name)
    .bind(query.environment.as_deref())
    .fetch_all(db)
    .await?;

    let deployments: Vec<DeploymentMarker> = sqlx::query_as(
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
    .bind(tenant_id)
    .bind(service_name)
    .bind(query.from)
    .bind(query.to)
    .bind(query.environment.as_deref())
    .fetch_all(db)
    .await?;

    let incidents: Vec<IncidentItem> = incidents
        .into_iter()
        .map(|row| IncidentItem {
            incident_id: row.incident_id,
            title: row.title,
            severity: row.severity,
            status: row.status,
            triggered_at: row.triggered_at,
            resolved_at: row.resolved_at,
            triggered_by_rule_id: row.triggered_by_rule_id,
        })
        .collect();

    Ok(Some(ReliabilityReportResponse {
        service_name: service_name.to_string(),
        environment: query.environment.clone(),
        from: query.from,
        to: query.to,
        incident_summary: compute_incident_summary(query.from, query.to, &incidents),
        slo_summary: compute_slo_summary(&slos),
        deployment_summary: compute_deployment_summary(&deployments),
        incidents,
        slos,
        deployments,
    }))
}

pub async fn handle_get_service_reliability_report(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(service_name): Path<String>,
    Query(query): Query<ReliabilityReportQuery>,
) -> Result<Json<ReliabilityReportResponse>, StatusCode> {
    match get_service_reliability_report(&state.db, ctx.tenant_id, &service_name, &query).await {
        Ok(Some(report)) => Ok(Json(report)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to get service reliability report");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
