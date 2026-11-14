// Internal, service-to-service-only endpoints for the alerting component
// (gated by `middleware::auth::require_internal_service`, not `require_tenant`
// -- there is no end-user credential on these calls, only a trusted caller
// within the cluster). Callers pass `tenant_id` explicitly.
//
// These are the alerting halves of the two joins query-api used to get from
// admin-service's combined `/internal/*` endpoints. Phase 6 "consolidate
// alerting" (docs/component-decomposition.md) split those by owner: the
// `slo_definitions`/`alert_rules`/`alert_firings`/`incidents` joins live here,
// while the `deployment_markers` joins stay in admin-service. query-api calls
// both and merges.

use crate::AppState;
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct AlertingEnrichmentParams {
    pub tenant_id: Uuid,
    pub environment: Option<String>,
}

#[derive(Serialize)]
pub struct AlertingEnrichmentItem {
    pub service_name: String,
    pub active_alert_count: i64,
    pub slo_breaching: bool,
}

#[derive(Serialize)]
pub struct AlertingEnrichmentResponse {
    pub items: Vec<AlertingEnrichmentItem>,
}

/// GET /internal/alerting-enrichment
///
/// SLO-linked active-alert counts per service. Moved verbatim from
/// admin-service's `internal::handle_service_catalog_enrichment` (same SQL).
pub async fn handle_alerting_enrichment(
    State(state): State<AppState>,
    Query(params): Query<AlertingEnrichmentParams>,
) -> Result<Json<AlertingEnrichmentResponse>, StatusCode> {
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
        tracing::error!(error = %e, "failed to fetch alerting enrichment");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let items = slo_rows
        .into_iter()
        .map(|row| AlertingEnrichmentItem {
            service_name: row.service_name,
            active_alert_count: row.active_alert_count.max(0),
            slo_breaching: row.slo_breaching,
        })
        .collect();

    Ok(Json(AlertingEnrichmentResponse { items }))
}

#[derive(Deserialize)]
pub struct AlertingCorrelationParams {
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
    pub sli_outcome: String,
    pub sli_scope: String,
    pub commitment_tier: String,
    pub approval_status: String,
    pub approved_by: Option<Uuid>,
    pub approved_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub struct AlertingCorrelationResponse {
    pub incidents: Vec<ReliabilityIncidentRow>,
    pub slos: Vec<ReliabilitySloRow>,
}

/// GET /internal/alerting-correlation
///
/// Incidents and SLOs for a service/environment/interval. Moved verbatim from
/// admin-service's `internal::handle_reliability_correlation` (same two SQL
/// queries; the deployment query stays in admin-service).
pub async fn handle_alerting_correlation(
    State(state): State<AppState>,
    Query(params): Query<AlertingCorrelationParams>,
) -> Result<Json<AlertingCorrelationResponse>, StatusCode> {
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
        tracing::error!(error = %e, "failed to fetch alerting-correlated incidents");
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
                created_at, updated_at, \
                sli_outcome, sli_scope, commitment_tier, approval_status, approved_by, approved_at \
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
        tracing::error!(error = %e, "failed to fetch alerting-correlated slos");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(AlertingCorrelationResponse { incidents, slos }))
}

/// Equivalent pre- and post-change evaluation windows around a change timestamp (OBS-11-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquivalentChangeWindows {
    pub pre_start: DateTime<Utc>,
    pub pre_end: DateTime<Utc>,
    pub post_start: DateTime<Utc>,
    pub post_end: DateTime<Utc>,
}

/// Builds symmetric pre- and post-change windows of identical duration (OBS-11-02).
pub fn build_equivalent_change_windows(
    change_time: DateTime<Utc>,
    window_minutes: i64,
) -> EquivalentChangeWindows {
    let span = chrono::Duration::minutes(window_minutes.max(1));
    EquivalentChangeWindows {
        pre_start: change_time - span,
        pre_end: change_time,
        post_start: change_time,
        post_end: change_time + span,
    }
}

/// Change/release event record used for regression attribution and change history (OBS-11-04, OBS-11-07).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceChangeEntry {
    pub change_id: String,
    pub service_name: String,
    pub change_kind: String, // "deployment" | "configuration_change" | "feature_activation"
    pub release_owner: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

/// Associates a controlled regression with the latest matching release/change owner (OBS-11-04).
pub fn attribute_controlled_regression<'a>(
    service_name: &str,
    changes: &'a [ServiceChangeEntry],
) -> Option<&'a ServiceChangeEntry> {
    changes
        .iter()
        .filter(|c| c.service_name == service_name && c.release_owner.is_some())
        .max_by_key(|c| c.occurred_at)
}

/// Merges deployments, configuration changes, and feature activations into unified history (OBS-11-07).
pub fn merge_service_change_history(
    service_name: &str,
    entries: &[ServiceChangeEntry],
) -> Vec<ServiceChangeEntry> {
    let mut filtered: Vec<ServiceChangeEntry> = entries
        .iter()
        .filter(|e| e.service_name == service_name)
        .cloned()
        .collect();
    filtered.sort_by_key(|e| e.occurred_at);
    filtered
}

#[cfg(test)]
mod p1_change_tracking_tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn obs_11_02_equivalent_pre_and_post_change_windows() {
        let t = Utc::now();
        let w = build_equivalent_change_windows(t, 30);
        assert_eq!(w.pre_end - w.pre_start, Duration::minutes(30));
        assert_eq!(w.post_end - w.post_start, Duration::minutes(30));
        assert_eq!(w.pre_end, w.post_start);
    }

    #[test]
    fn obs_11_04_controlled_regression_attributed_to_release_owner() {
        let t = Utc::now();
        let entries = vec![
            ServiceChangeEntry {
                change_id: "rel-1".to_string(),
                service_name: "checkout".to_string(),
                change_kind: "deployment".to_string(),
                release_owner: Some("alice@example.com".to_string()),
                occurred_at: t - Duration::minutes(20),
            },
            ServiceChangeEntry {
                change_id: "rel-2".to_string(),
                service_name: "checkout".to_string(),
                change_kind: "deployment".to_string(),
                release_owner: Some("bob@example.com".to_string()),
                occurred_at: t - Duration::minutes(5),
            },
        ];
        let attributed = attribute_controlled_regression("checkout", &entries).expect("owner");
        assert_eq!(attributed.change_id, "rel-2");
        assert_eq!(attributed.release_owner.as_deref(), Some("bob@example.com"));
    }

    #[test]
    fn obs_11_07_config_change_and_feature_activation_in_service_history() {
        let t = Utc::now();
        let history = merge_service_change_history(
            "checkout",
            &[
                ServiceChangeEntry {
                    change_id: "cfg-1".to_string(),
                    service_name: "checkout".to_string(),
                    change_kind: "configuration_change".to_string(),
                    release_owner: Some("ops".to_string()),
                    occurred_at: t - Duration::minutes(10),
                },
                ServiceChangeEntry {
                    change_id: "ff-1".to_string(),
                    service_name: "checkout".to_string(),
                    change_kind: "feature_activation".to_string(),
                    release_owner: Some("pm".to_string()),
                    occurred_at: t - Duration::minutes(5),
                },
            ],
        );
        let kinds: Vec<&str> = history.iter().map(|h| h.change_kind.as_str()).collect();
        assert_eq!(kinds, vec!["configuration_change", "feature_activation"]);
    }
}
