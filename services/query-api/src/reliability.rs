use crate::middleware::auth::TenantContext;
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

/// Deployment-marker row shape for this report's correlation query. Deployment
/// listing/management itself lives in admin-service (observable-control); this
/// is a local, read-only projection for correlating deployments with the
/// reliability window, not a shared type. Deserialized from admin-service's
/// `/internal/deployment-correlation` response (Phase 6's internal-endpoint
/// split by owner, docs/component-decomposition.md) -- field names must match
/// `admin_service::internal::ReliabilityDeploymentRow`.
#[derive(Serialize, Deserialize)]
pub struct DeploymentMarker {
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

/// SLO-definition row shape for this report's correlation query. SLO CRUD
/// itself lives in alert-evaluator (the alerting component); this is a local,
/// read-only projection for correlating SLOs with the reliability window,
/// not a shared type. Deserialized from alert-evaluator's
/// `/internal/alerting-correlation` response -- field names must match
/// `alert_evaluator::internal::ReliabilitySloRow`.
#[derive(Serialize, Deserialize)]
pub struct SloDefinitionItem {
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

/// Incident summary row shape for this report. Incident listing/detail itself
/// lives in alert-evaluator (the alerting component); this is a local,
/// read-only projection for correlating incidents with the reliability window,
/// not a shared type. Deserialized from alert-evaluator's
/// `/internal/alerting-correlation` response -- field names must match
/// `alert_evaluator::internal::ReliabilityIncidentRow`.
#[derive(Serialize, Deserialize)]
pub struct IncidentItem {
    pub incident_id: Uuid,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub triggered_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub triggered_by_rule_id: Option<Uuid>,
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

#[derive(Deserialize)]
struct AlertingCorrelationResponse {
    incidents: Vec<IncidentItem>,
    slos: Vec<SloDefinitionItem>,
}

#[derive(Deserialize)]
struct DeploymentCorrelationResponse {
    deployments: Vec<DeploymentMarker>,
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

/// Fetches the incident/SLO correlation from alert-evaluator's
/// `/internal/alerting-correlation` and the deployment correlation from
/// admin-service's `/internal/deployment-correlation` (Phase 6's
/// internal-endpoint split by owner, docs/component-decomposition.md) -- the
/// three joins that used to run as direct SQL here now happen once, in
/// Postgres, in the owning component; this is a single batched call to each,
/// not per-row.
pub async fn get_service_reliability_report(
    state: &AppState,
    tenant_id: Uuid,
    service_name: &str,
    query: &ReliabilityReportQuery,
) -> Result<Option<ReliabilityReportResponse>, reqwest::Error> {
    let mut query_params = vec![
        ("tenant_id", tenant_id.to_string()),
        ("service_name", service_name.to_string()),
        (
            "from",
            query
                .from
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        ),
        (
            "to",
            query
                .to
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        ),
    ];
    if let Some(env) = &query.environment {
        query_params.push(("environment", env.clone()));
    }

    let alerting: AlertingCorrelationResponse = state
        .http_client
        .get(format!(
            "{}/internal/alerting-correlation",
            state.alert_evaluator_url
        ))
        .query(&query_params)
        .header("X-Internal-Token", &state.internal_service_token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let deployments: DeploymentCorrelationResponse = state
        .http_client
        .get(format!(
            "{}/internal/deployment-correlation",
            state.admin_service_url
        ))
        .query(&query_params)
        .header("X-Internal-Token", &state.internal_service_token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(Some(ReliabilityReportResponse {
        service_name: service_name.to_string(),
        environment: query.environment.clone(),
        from: query.from,
        to: query.to,
        incident_summary: compute_incident_summary(query.from, query.to, &alerting.incidents),
        slo_summary: compute_slo_summary(&alerting.slos),
        deployment_summary: compute_deployment_summary(&deployments.deployments),
        incidents: alerting.incidents,
        slos: alerting.slos,
        deployments: deployments.deployments,
    }))
}

pub async fn handle_get_service_reliability_report(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(service_name): Path<String>,
    Query(query): Query<ReliabilityReportQuery>,
) -> Result<Json<ReliabilityReportResponse>, StatusCode> {
    match get_service_reliability_report(&state, ctx.tenant_id, &service_name, &query).await {
        Ok(Some(report)) => Ok(Json(report)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to get service reliability report");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
