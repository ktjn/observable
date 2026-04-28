use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
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
    pub firing: bool,
    pub last_fired_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub struct AlertRuleListResponse {
    pub items: Vec<AlertRuleItem>,
}

#[derive(Deserialize)]
pub struct CreateRuleRequest {
    pub name: String,
    pub metric_name: String,
    pub operator: String,
    pub threshold: f64,
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

#[derive(sqlx::FromRow)]
struct AlertRuleRow {
    rule_id: Uuid,
    name: String,
    condition: serde_json::Value,
    severity: String,
    silenced: bool,
    firing: bool,
    last_fired_at: Option<DateTime<Utc>>,
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
         EXISTS( \
             SELECT 1 FROM alert_firings af \
             WHERE af.rule_id = r.rule_id AND af.state = 'active' \
         ) AS firing, \
         (SELECT MAX(occurred_at) FROM alert_firings af \
          WHERE af.rule_id = r.rule_id AND af.state = 'active') AS last_fired_at \
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
            let (metric_name, operator, threshold) = condition_fields(&row.condition)?;
            Some(AlertRuleItem {
                rule_id: row.rule_id,
                name: row.name,
                metric_name,
                operator,
                threshold,
                severity: row.severity,
                silenced: row.silenced,
                firing: row.firing,
                last_fired_at: row.last_fired_at,
            })
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

    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules (tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, $2, 'threshold', 'warning', $3) \
         RETURNING rule_id",
    )
    .bind(tenant_id)
    .bind(&req.name)
    .bind(&condition)
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
        firing: false,
        last_fired_at: None,
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
            firing: true,
            last_fired_at: None,
        };
        let v = serde_json::to_value(&item).unwrap();
        assert_eq!(v["name"], "High error rate");
        assert_eq!(v["metric_name"], "error_rate");
        assert_eq!(v["operator"], "gt");
        assert_eq!(v["firing"], true);
        assert!(v["last_fired_at"].is_null());
    }
}
