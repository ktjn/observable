use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

/// Canonical alert rule summary entity. Mirrors `alerts.AlertRule@1` in
/// `models/alerts.mdl` field-for-field.
/// `AlertRuleRow`/`AlertRuleDetailRow` (Postgres `sqlx::FromRow`
/// projections) and `AlertRuleListResponse`/`AlertRuleDetailResponse` (list
/// wrapper / join+firings aggregation) are NOT modeled — timestamp fields
/// stay `chrono::DateTime<Utc>` (Phase 1 backlog item 5: modelable's
/// `timestamp` emits as Rust `String`).
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AlertRuleItem {
    pub rule_id: Uuid,
    pub name: String,
    pub metric_name: String,
    pub operator: String,
    pub threshold: f64,
    pub severity: String,
    pub silenced: bool,
    pub state: String,
    pub firing: bool,
    pub last_fired_at: Option<DateTime<Utc>>,
    pub notification_channels: Vec<Uuid>,
    pub auto_trigger_incident: bool,
    pub service_name: Option<String>,
    pub suppressed: bool,
}

#[derive(Serialize)]
pub struct AlertRuleListResponse {
    pub items: Vec<AlertRuleItem>,
}

/// Canonical alert firing entity. Mirrors `alerts.Firing@1` in
/// `models/alerts.mdl` field-for-field.
/// `occurred_at`/`resolved_at` stay `chrono::DateTime<Utc>` (Phase 1
/// backlog item 5).
#[derive(Serialize)]
pub struct FiringItem {
    pub firing_id: Uuid,
    pub state: String,
    pub value: Option<f64>,
    pub occurred_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub suppressed_by_rule_name: Option<String>,
}

#[derive(Serialize)]
pub struct AlertRuleDetailResponse {
    pub rule_id: Uuid,
    pub name: String,
    pub severity: String,
    pub alert_type: String,
    pub condition: serde_json::Value,
    pub silenced: bool,
    pub firing: bool,
    pub firings: Vec<FiringItem>,
    pub runbook_url: Option<String>,
}

#[derive(sqlx::FromRow)]
struct AlertRuleDetailRow {
    rule_id: Uuid,
    name: String,
    severity: String,
    alert_type: String,
    condition: serde_json::Value,
    silenced: bool,
    firing: bool,
    runbook_url: Option<String>,
}

#[derive(sqlx::FromRow)]
struct FiringRow {
    firing_id: Uuid,
    state: String,
    value: Option<f64>,
    occurred_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    suppressed_by_rule_name: Option<String>,
}

#[derive(sqlx::FromRow)]
struct AlertRuleRow {
    rule_id: Uuid,
    name: String,
    condition: serde_json::Value,
    severity: String,
    silenced: bool,
    state: String,
    firing: bool,
    last_fired_at: Option<DateTime<Utc>>,
    notification_channels: Vec<Uuid>,
    auto_trigger_incident: bool,
    service_name: Option<String>,
    suppressed: bool,
}

fn condition_fields(condition: &serde_json::Value) -> Option<(String, String, f64)> {
    if let (Some(metric_name), Some(operator), Some(threshold)) = (
        condition.get("metric_name").and_then(|v| v.as_str()),
        condition.get("operator").and_then(|v| v.as_str()),
        condition.get("threshold").and_then(|v| v.as_f64()),
    ) {
        return Some((metric_name.to_string(), operator.to_string(), threshold));
    }
    // Disambiguates from the deadman shape below by checking threshold_percent's
    // presence; must stay ordered before the deadman check (and after the
    // threshold check above) since the three condition shapes are mutually
    // exclusive only by which of operator/threshold_percent/service_name they carry.
    if let (Some(metric_name), Some(threshold_percent)) = (
        condition.get("metric_name").and_then(|v| v.as_str()),
        condition.get("threshold_percent").and_then(|v| v.as_f64()),
    ) {
        return Some((
            metric_name.to_string(),
            "change_detection".to_string(),
            threshold_percent,
        ));
    }
    if let (Some(service_name), Some(window_secs)) = (
        condition.get("service_name").and_then(|v| v.as_str()),
        condition.get("window_secs").and_then(|v| v.as_f64()),
    ) {
        return Some((service_name.to_string(), "no_data".to_string(), window_secs));
    }
    None
}

pub async fn list_alert_rules(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
) -> Result<Vec<AlertRuleItem>, sqlx::Error> {
    let rows = sqlx::query_as::<_, AlertRuleRow>(
        "SELECT r.rule_id, r.name, r.condition, r.severity, r.silenced, r.service_name, \
         CASE \
             WHEN r.silenced THEN 'silenced' \
             ELSE COALESCE(( \
                 SELECT af.state FROM alert_firings af \
                 WHERE af.rule_id = r.rule_id AND af.tenant_id = r.tenant_id \
                 ORDER BY CASE WHEN af.state IN ('pending', 'active', 'suppressed') THEN 0 ELSE 1 END, \
                          af.occurred_at DESC \
                 LIMIT 1 \
             ), 'ok') \
         END AS state, \
         EXISTS( \
             SELECT 1 FROM alert_firings af \
             WHERE af.rule_id = r.rule_id AND af.tenant_id = r.tenant_id \
               AND af.state = 'active' AND r.silenced = false \
         ) AS firing, \
         (SELECT MAX(occurred_at) FROM alert_firings af \
          WHERE af.rule_id = r.rule_id AND af.tenant_id = r.tenant_id \
            AND af.state = 'active') AS last_fired_at, \
         r.notification_channels, r.auto_trigger_incident, \
         EXISTS( \
             SELECT 1 FROM alert_firings af \
             WHERE af.rule_id = r.rule_id AND af.tenant_id = r.tenant_id \
               AND af.state = 'suppressed' \
         ) AS suppressed \
         FROM alert_rules r \
         WHERE r.tenant_id = $1 AND r.alert_type IN ('threshold', 'deadman', 'change_detection') \
         ORDER BY r.created_at DESC",
    )
    .bind(tenant_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            match condition_fields(&row.condition) {
                Some((metric_name, operator, threshold)) => Some(AlertRuleItem {
                    rule_id: row.rule_id,
                    name: row.name,
                    metric_name,
                    operator,
                    threshold,
                    severity: row.severity,
                    silenced: row.silenced,
                    state: row.state,
                    firing: row.firing,
                    last_fired_at: row.last_fired_at,
                    notification_channels: row.notification_channels,
                    auto_trigger_incident: row.auto_trigger_incident,
                    service_name: row.service_name,
                    suppressed: row.suppressed,
                }),
                None => {
                    tracing::warn!(rule_id = %row.rule_id, "skipping alert rule with malformed condition JSONB");
                    None
                }
            }
        })
        .collect())
}

pub async fn get_alert_rule(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    rule_id: Uuid,
) -> Result<Option<AlertRuleDetailResponse>, sqlx::Error> {
    let row: Option<AlertRuleDetailRow> = sqlx::query_as(
        "SELECT r.rule_id, r.name, r.severity, r.alert_type, r.condition, r.silenced, r.runbook_url, \
         EXISTS( \
             SELECT 1 FROM alert_firings af \
             WHERE af.rule_id = r.rule_id AND af.tenant_id = r.tenant_id \
               AND af.state = 'active' AND r.silenced = false \
         ) AS firing \
         FROM alert_rules r \
         WHERE r.rule_id = $1 AND r.tenant_id = $2",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .fetch_optional(db)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let firings: Vec<FiringRow> = sqlx::query_as(
        "SELECT f.firing_id, f.state, f.value, f.occurred_at, f.resolved_at, \
         r_by.name AS suppressed_by_rule_name \
         FROM alert_firings f \
         LEFT JOIN alert_firings f_by ON f.suppressed_by_firing_id = f_by.firing_id \
         LEFT JOIN alert_rules r_by ON f_by.rule_id = r_by.rule_id \
         WHERE f.rule_id = $1 AND f.tenant_id = $2 \
         ORDER BY f.occurred_at DESC \
         LIMIT 20",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .fetch_all(db)
    .await?;

    Ok(Some(AlertRuleDetailResponse {
        rule_id: row.rule_id,
        name: row.name,
        severity: row.severity,
        alert_type: row.alert_type,
        condition: row.condition,
        silenced: row.silenced,
        firing: row.firing,
        runbook_url: row.runbook_url,
        firings: firings
            .into_iter()
            .map(|f| FiringItem {
                firing_id: f.firing_id,
                state: f.state,
                value: f.value,
                occurred_at: f.occurred_at,
                resolved_at: f.resolved_at,
                suppressed_by_rule_name: f.suppressed_by_rule_name,
            })
            .collect(),
    }))
}

pub async fn handle_get_rule(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(rule_id): Path<Uuid>,
) -> Result<Json<AlertRuleDetailResponse>, StatusCode> {
    match get_alert_rule(&state.db, ctx.tenant_id, rule_id).await {
        Ok(Some(detail)) => Ok(Json(detail)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to get alert rule");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_list_rules(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<AlertRuleListResponse>, StatusCode> {
    let items = list_alert_rules(&state.db, ctx.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to list alert rules");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(AlertRuleListResponse { items }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condition_fields_extracts_all_three_fields() {
        let cond = serde_json::json!({
            "metric_name": "error_rate",
            "operator": "gt",
            "threshold": 0.05
        });
        let (metric_name, operator, threshold) = condition_fields(&cond).unwrap();
        assert_eq!(metric_name, "error_rate");
        assert_eq!(operator, "gt");
        assert!((threshold - 0.05).abs() < f64::EPSILON);
    }

    #[test]
    fn condition_fields_returns_none_when_metric_name_missing() {
        let cond = serde_json::json!({"operator": "gt", "threshold": 1.0});
        assert!(condition_fields(&cond).is_none());
    }

    #[test]
    fn condition_fields_returns_none_when_threshold_not_number() {
        let cond = serde_json::json!({"metric_name": "m", "operator": "gt", "threshold": "bad"});
        assert!(condition_fields(&cond).is_none());
    }

    #[test]
    fn condition_fields_extracts_change_detection_shape_as_change_detection_operator() {
        let cond = serde_json::json!({
            "metric_name": "error_rate",
            "window_secs": 300,
            "baseline_offset_secs": 86400,
            "threshold_percent": 50.0
        });
        let (metric_name, operator, threshold) = condition_fields(&cond).unwrap();
        assert_eq!(metric_name, "error_rate");
        assert_eq!(operator, "change_detection");
        assert!((threshold - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn condition_fields_extracts_deadman_shape_as_no_data_operator() {
        let cond = serde_json::json!({"service_name": "checkout", "window_secs": 300});
        let (metric_name, operator, threshold) = condition_fields(&cond).unwrap();
        assert_eq!(metric_name, "checkout");
        assert_eq!(operator, "no_data");
        assert!((threshold - 300.0).abs() < f64::EPSILON);
    }

    #[test]
    fn condition_fields_returns_none_for_empty_object() {
        let cond = serde_json::json!({});
        assert!(condition_fields(&cond).is_none());
    }

    #[test]
    fn alert_rule_detail_response_includes_runbook_url() {
        let r = AlertRuleDetailResponse {
            rule_id: Uuid::nil(),
            name: "test".into(),
            severity: "warning".into(),
            alert_type: "threshold".into(),
            condition: serde_json::json!({}),
            silenced: false,
            firing: false,
            firings: vec![],
            runbook_url: Some("https://example.com/runbook".into()),
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["runbook_url"], "https://example.com/runbook");
    }

    #[test]
    fn alert_rule_item_serializes_to_expected_json_shape() {
        let id = Uuid::nil();
        let item = AlertRuleItem {
            rule_id: id,
            name: "High error rate".into(),
            metric_name: "error_rate".into(),
            operator: "gt".into(),
            threshold: 0.05,
            severity: "warning".into(),
            silenced: false,
            state: "active".into(),
            firing: true,
            last_fired_at: None,
            notification_channels: vec![],
            auto_trigger_incident: true,
            service_name: None,
            suppressed: false,
        };
        let v = serde_json::to_value(&item).unwrap();
        assert_eq!(v["name"], "High error rate");
        assert_eq!(v["metric_name"], "error_rate");
        assert_eq!(v["operator"], "gt");
        assert_eq!(v["state"], "active");
        assert_eq!(v["firing"], true);
        assert!(v["last_fired_at"].is_null());
    }

    #[test]
    fn alert_rule_item_has_service_name_field() {
        let item = AlertRuleItem {
            rule_id: Uuid::nil(),
            name: "test".into(),
            metric_name: "cpu".into(),
            operator: "gt".into(),
            threshold: 90.0,
            severity: "warning".into(),
            silenced: false,
            state: "ok".into(),
            firing: false,
            last_fired_at: None,
            notification_channels: vec![],
            auto_trigger_incident: false,
            service_name: Some("payments".into()),
            suppressed: false,
        };
        assert_eq!(item.service_name, Some("payments".into()));
        assert!(!item.suppressed);
    }

    #[test]
    fn firing_item_has_suppressed_by_rule_name_field() {
        let item = FiringItem {
            firing_id: Uuid::nil(),
            state: "suppressed".into(),
            value: Some(1.0),
            occurred_at: chrono::Utc::now(),
            resolved_at: None,
            suppressed_by_rule_name: Some("CPU critical \u{2013} payments".into()),
        };
        assert_eq!(
            item.suppressed_by_rule_name,
            Some("CPU critical \u{2013} payments".into())
        );
    }
}
