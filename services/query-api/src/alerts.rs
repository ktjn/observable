use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const VALID_OPERATORS: &[&str] = &["gt", "gte", "lt", "lte", "eq"];

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
}

#[derive(Serialize)]
pub struct AlertRuleListResponse {
    pub items: Vec<AlertRuleItem>,
}

#[derive(Serialize)]
pub struct FiringItem {
    pub firing_id: Uuid,
    pub state: String,
    pub value: Option<f64>,
    pub occurred_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
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
}

#[derive(Deserialize)]
pub struct CreateRuleRequest {
    pub name: String,
    pub metric_name: String,
    pub operator: String,
    pub threshold: f64,
    pub notification_channels: Option<Vec<Uuid>>,
    pub auto_trigger_incident: Option<bool>,
}

#[derive(Deserialize)]
pub struct SilenceRequest {
    pub silenced: bool,
}

#[derive(Debug)]
pub enum CreateRuleError {
    InvalidInput(String),
    Db(sqlx::Error),
}

impl std::fmt::Display for CreateRuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CreateRuleError::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            CreateRuleError::Db(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for CreateRuleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CreateRuleError::Db(e) => Some(e),
            CreateRuleError::InvalidInput(_) => None,
        }
    }
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
}

fn condition_fields(condition: &serde_json::Value) -> Option<(String, String, f64)> {
    let metric_name = condition.get("metric_name")?.as_str()?.to_string();
    let operator = condition.get("operator")?.as_str()?.to_string();
    let threshold = condition.get("threshold")?.as_f64()?;
    Some((metric_name, operator, threshold))
}

pub async fn list_alert_rules(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
) -> Result<Vec<AlertRuleItem>, sqlx::Error> {
    let rows = sqlx::query_as::<_, AlertRuleRow>(
        "SELECT r.rule_id, r.name, r.condition, r.severity, r.silenced, \
         CASE \
             WHEN r.silenced THEN 'silenced' \
             ELSE COALESCE(( \
                 SELECT af.state FROM alert_firings af \
                 WHERE af.rule_id = r.rule_id AND af.tenant_id = r.tenant_id \
                 ORDER BY CASE WHEN af.state IN ('pending', 'active') THEN 0 ELSE 1 END, \
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
         r.notification_channels, r.auto_trigger_incident \
         FROM alert_rules r \
         WHERE r.tenant_id = $1 AND r.alert_type = 'threshold' \
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
                }),
                None => {
                    tracing::warn!(rule_id = %row.rule_id, "skipping alert rule with malformed condition JSONB");
                    None
                }
            }
        })
        .collect())
}

pub async fn create_alert_rule(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    req: &CreateRuleRequest,
) -> Result<AlertRuleItem, CreateRuleError> {
    if req.name.trim().is_empty() {
        return Err(CreateRuleError::InvalidInput("name is required".into()));
    }
    if req.metric_name.trim().is_empty() {
        return Err(CreateRuleError::InvalidInput(
            "metric_name is required".into(),
        ));
    }
    if !VALID_OPERATORS.contains(&req.operator.as_str()) {
        return Err(CreateRuleError::InvalidInput(format!(
            "operator must be one of: {}",
            VALID_OPERATORS.join(", ")
        )));
    }
    if !req.threshold.is_finite() {
        return Err(CreateRuleError::InvalidInput(
            "threshold must be finite".into(),
        ));
    }

    let condition = serde_json::json!({
        "metric_name": req.metric_name,
        "operator": req.operator,
        "threshold": req.threshold,
    });

    let channels = req.notification_channels.clone().unwrap_or_default();
    let auto_trigger = req.auto_trigger_incident.unwrap_or(true);

    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, $2, 'threshold', 'warning', $3, $4, $5) \
         RETURNING rule_id",
    )
    .bind(tenant_id)
    .bind(&req.name)
    .bind(&condition)
    .bind(&channels)
    .bind(auto_trigger)
    .fetch_one(db)
    .await
    .map_err(CreateRuleError::Db)?;

    Ok(AlertRuleItem {
        rule_id,
        name: req.name.clone(),
        metric_name: req.metric_name.clone(),
        operator: req.operator.clone(),
        threshold: req.threshold,
        severity: "warning".into(),
        silenced: false,
        state: "ok".into(),
        firing: false,
        last_fired_at: None,
        notification_channels: channels,
        auto_trigger_incident: auto_trigger,
    })
}

pub async fn silence_alert_rule(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    rule_id: Uuid,
    silenced: bool,
) -> Result<Option<AlertRuleItem>, sqlx::Error> {
    let updated: Option<Uuid> = sqlx::query_scalar(
        "UPDATE alert_rules SET silenced = $1 \
         WHERE rule_id = $2 AND tenant_id = $3 \
         RETURNING rule_id",
    )
    .bind(silenced)
    .bind(rule_id)
    .bind(tenant_id)
    .fetch_optional(db)
    .await?;

    if updated.is_none() {
        return Ok(None);
    }

    let rules = list_alert_rules(db, tenant_id).await?;
    Ok(rules.into_iter().find(|r| r.rule_id == rule_id))
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
        "SELECT firing_id, state, value, occurred_at, resolved_at \
         FROM alert_firings \
         WHERE rule_id = $1 AND tenant_id = $2 \
         ORDER BY occurred_at DESC \
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

pub async fn handle_create_rule(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<CreateRuleRequest>,
) -> Result<(StatusCode, Json<AlertRuleItem>), StatusCode> {
    match create_alert_rule(&state.db, ctx.tenant_id, &req).await {
        Ok(item) => Ok((StatusCode::CREATED, Json(item))),
        Err(CreateRuleError::InvalidInput(msg)) => {
            tracing::warn!(message = %msg, "invalid alert rule input");
            Err(StatusCode::BAD_REQUEST)
        }
        Err(CreateRuleError::Db(e)) => {
            tracing::error!(error = %e, "failed to create alert rule");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_silence_rule(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(rule_id): Path<Uuid>,
    Json(req): Json<SilenceRequest>,
) -> Result<Json<AlertRuleItem>, StatusCode> {
    match silence_alert_rule(&state.db, ctx.tenant_id, rule_id, req.silenced).await {
        Ok(Some(item)) => Ok(Json(item)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to silence alert rule");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
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
    fn all_five_operators_are_valid() {
        for op in ["gt", "gte", "lt", "lte", "eq"] {
            assert!(
                VALID_OPERATORS.contains(&op),
                "{op} should be a valid operator"
            );
        }
    }

    #[test]
    fn unknown_operator_is_not_valid() {
        assert!(!VALID_OPERATORS.contains(&"neq"));
        assert!(!VALID_OPERATORS.contains(&">"));
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
        };
        let v = serde_json::to_value(&item).unwrap();
        assert_eq!(v["name"], "High error rate");
        assert_eq!(v["metric_name"], "error_rate");
        assert_eq!(v["operator"], "gt");
        assert_eq!(v["state"], "active");
        assert_eq!(v["firing"], true);
        assert!(v["last_fired_at"].is_null());
    }
}
