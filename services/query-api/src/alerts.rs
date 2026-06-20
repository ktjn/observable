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

/// Canonical alert rule summary entity. Mirrors `alerts.AlertRule@1` in
/// `models/alerts.mdl` field-for-field (see
/// `docs/superpowers/specs/2026-06-14-alerts-modelable-migration-design.md`).
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
}

#[derive(Serialize)]
pub struct AlertRuleListResponse {
    pub items: Vec<AlertRuleItem>,
}

/// Canonical alert firing entity. Mirrors `alerts.Firing@1` in
/// `models/alerts.mdl` field-for-field (see
/// `docs/superpowers/specs/2026-06-14-alerts-modelable-migration-design.md`).
/// `occurred_at`/`resolved_at` stay `chrono::DateTime<Utc>` (Phase 1
/// backlog item 5).
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
    pub runbook_url: Option<String>,
    pub alert_type: Option<String>,
    pub service_name: Option<String>,
    pub window_secs: Option<i64>,
    pub baseline_offset_secs: Option<i64>,
    pub threshold_percent: Option<f64>,
}

#[derive(Deserialize)]
pub struct SilenceRequest {
    pub silenced: bool,
}

#[derive(Deserialize)]
pub struct UpdateRunbookRequest {
    pub runbook_url: Option<String>,
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

    let alert_type = req.alert_type.as_deref().unwrap_or("threshold");

    match alert_type {
        "threshold" => {
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
                "INSERT INTO alert_rules \
                 (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident, runbook_url) \
                 VALUES ($1, $2, 'threshold', 'warning', $3, $4, $5, $6) \
                 RETURNING rule_id",
            )
            .bind(tenant_id)
            .bind(&req.name)
            .bind(&condition)
            .bind(&channels)
            .bind(auto_trigger)
            .bind(req.runbook_url.as_deref())
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
        "deadman" => {
            let service_name = req
                .service_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| CreateRuleError::InvalidInput("service_name is required".into()))?;
            let window_secs = req
                .window_secs
                .ok_or_else(|| CreateRuleError::InvalidInput("window_secs is required".into()))?;
            if window_secs <= 0 {
                return Err(CreateRuleError::InvalidInput(
                    "window_secs must be positive".into(),
                ));
            }

            let condition = serde_json::json!({
                "service_name": service_name,
                "window_secs": window_secs,
            });
            let channels = req.notification_channels.clone().unwrap_or_default();
            let auto_trigger = req.auto_trigger_incident.unwrap_or(true);

            let rule_id: Uuid = sqlx::query_scalar(
                "INSERT INTO alert_rules \
                 (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident, runbook_url) \
                 VALUES ($1, $2, 'deadman', 'warning', $3, $4, $5, $6) \
                 RETURNING rule_id",
            )
            .bind(tenant_id)
            .bind(&req.name)
            .bind(&condition)
            .bind(&channels)
            .bind(auto_trigger)
            .bind(req.runbook_url.as_deref())
            .fetch_one(db)
            .await
            .map_err(CreateRuleError::Db)?;

            Ok(AlertRuleItem {
                rule_id,
                name: req.name.clone(),
                metric_name: service_name.to_string(),
                operator: "no_data".into(),
                threshold: window_secs as f64,
                severity: "warning".into(),
                silenced: false,
                state: "ok".into(),
                firing: false,
                last_fired_at: None,
                notification_channels: channels,
                auto_trigger_incident: auto_trigger,
            })
        }
        "change_detection" => {
            if req.metric_name.trim().is_empty() {
                return Err(CreateRuleError::InvalidInput(
                    "metric_name is required".into(),
                ));
            }
            let window_secs = req
                .window_secs
                .ok_or_else(|| CreateRuleError::InvalidInput("window_secs is required".into()))?;
            if window_secs <= 0 {
                return Err(CreateRuleError::InvalidInput(
                    "window_secs must be positive".into(),
                ));
            }
            let baseline_offset_secs = req.baseline_offset_secs.ok_or_else(|| {
                CreateRuleError::InvalidInput("baseline_offset_secs is required".into())
            })?;
            if baseline_offset_secs <= 0 {
                return Err(CreateRuleError::InvalidInput(
                    "baseline_offset_secs must be positive".into(),
                ));
            }
            let threshold_percent = req.threshold_percent.ok_or_else(|| {
                CreateRuleError::InvalidInput("threshold_percent is required".into())
            })?;
            if !threshold_percent.is_finite() {
                return Err(CreateRuleError::InvalidInput(
                    "threshold_percent must be finite".into(),
                ));
            }
            if threshold_percent < 0.0 {
                return Err(CreateRuleError::InvalidInput(
                    "threshold_percent must be non-negative".into(),
                ));
            }

            let condition = serde_json::json!({
                "metric_name": req.metric_name,
                "window_secs": window_secs,
                "baseline_offset_secs": baseline_offset_secs,
                "threshold_percent": threshold_percent,
            });
            let channels = req.notification_channels.clone().unwrap_or_default();
            let auto_trigger = req.auto_trigger_incident.unwrap_or(true);

            let rule_id: Uuid = sqlx::query_scalar(
                "INSERT INTO alert_rules \
                 (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident, runbook_url) \
                 VALUES ($1, $2, 'change_detection', 'warning', $3, $4, $5, $6) \
                 RETURNING rule_id",
            )
            .bind(tenant_id)
            .bind(&req.name)
            .bind(&condition)
            .bind(&channels)
            .bind(auto_trigger)
            .bind(req.runbook_url.as_deref())
            .fetch_one(db)
            .await
            .map_err(CreateRuleError::Db)?;

            Ok(AlertRuleItem {
                rule_id,
                name: req.name.clone(),
                metric_name: req.metric_name.clone(),
                operator: "change_detection".into(),
                threshold: threshold_percent,
                severity: "warning".into(),
                silenced: false,
                state: "ok".into(),
                firing: false,
                last_fired_at: None,
                notification_channels: channels,
                auto_trigger_incident: auto_trigger,
            })
        }
        other => Err(CreateRuleError::InvalidInput(format!(
            "unknown alert_type: {other}"
        ))),
    }
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

pub async fn update_alert_rule_runbook(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    rule_id: Uuid,
    runbook_url: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let updated: Option<Uuid> = sqlx::query_scalar(
        "UPDATE alert_rules SET runbook_url = $1 \
         WHERE rule_id = $2 AND tenant_id = $3 \
         RETURNING rule_id",
    )
    .bind(runbook_url)
    .bind(rule_id)
    .bind(tenant_id)
    .fetch_optional(db)
    .await?;
    Ok(updated.is_some())
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

fn validate_runbook_url(url: &Option<String>) -> Result<(), String> {
    if let Some(u) = url
        && !u.starts_with("http://")
        && !u.starts_with("https://")
    {
        return Err("runbook_url must start with http:// or https://".into());
    }
    Ok(())
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

pub async fn handle_update_rule_runbook(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(rule_id): Path<Uuid>,
    Json(req): Json<UpdateRunbookRequest>,
) -> Result<StatusCode, StatusCode> {
    if let Err(msg) = validate_runbook_url(&req.runbook_url) {
        tracing::warn!(message = %msg, "invalid runbook URL");
        return Err(StatusCode::BAD_REQUEST);
    }
    match update_alert_rule_runbook(
        &state.db,
        ctx.tenant_id,
        rule_id,
        req.runbook_url.as_deref(),
    )
    .await
    {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to update runbook URL");
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

    #[tokio::test]
    async fn create_alert_rule_rejects_deadman_without_service_name() {
        // No DB needed: validation happens before any query.
        let pool = sqlx::PgPool::connect_lazy("postgres://invalid/invalid").unwrap();
        let req = CreateRuleRequest {
            name: "Silent service".into(),
            metric_name: String::new(),
            operator: String::new(),
            threshold: 0.0,
            notification_channels: None,
            auto_trigger_incident: None,
            runbook_url: None,
            alert_type: Some("deadman".into()),
            service_name: None,
            window_secs: Some(300),
            baseline_offset_secs: None,
            threshold_percent: None,
        };
        let err = create_alert_rule(&pool, Uuid::nil(), &req)
            .await
            .unwrap_err();
        assert!(matches!(err, CreateRuleError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn create_alert_rule_rejects_deadman_with_non_positive_window() {
        let pool = sqlx::PgPool::connect_lazy("postgres://invalid/invalid").unwrap();
        let req = CreateRuleRequest {
            name: "Silent service".into(),
            metric_name: String::new(),
            operator: String::new(),
            threshold: 0.0,
            notification_channels: None,
            auto_trigger_incident: None,
            runbook_url: None,
            alert_type: Some("deadman".into()),
            service_name: Some("checkout".into()),
            window_secs: Some(0),
            baseline_offset_secs: None,
            threshold_percent: None,
        };
        let err = create_alert_rule(&pool, Uuid::nil(), &req)
            .await
            .unwrap_err();
        assert!(matches!(err, CreateRuleError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn create_alert_rule_rejects_change_detection_without_threshold_percent() {
        // No DB needed: validation happens before any query.
        let pool = sqlx::PgPool::connect_lazy("postgres://invalid/invalid").unwrap();
        let req = CreateRuleRequest {
            name: "Error rate change".into(),
            metric_name: "error_rate".into(),
            operator: String::new(),
            threshold: 0.0,
            notification_channels: None,
            auto_trigger_incident: None,
            runbook_url: None,
            alert_type: Some("change_detection".into()),
            service_name: None,
            window_secs: Some(300),
            baseline_offset_secs: Some(86400),
            threshold_percent: None,
        };
        let err = create_alert_rule(&pool, Uuid::nil(), &req)
            .await
            .unwrap_err();
        assert!(matches!(err, CreateRuleError::InvalidInput(_)));
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
    fn validate_runbook_url_accepts_https() {
        assert!(super::validate_runbook_url(&Some("https://example.com/runbook".into())).is_ok());
    }

    #[test]
    fn validate_runbook_url_accepts_http() {
        assert!(super::validate_runbook_url(&Some("http://internal.example.com".into())).is_ok());
    }

    #[test]
    fn validate_runbook_url_rejects_missing_scheme() {
        assert!(super::validate_runbook_url(&Some("example.com/runbook".into())).is_err());
    }

    #[test]
    fn validate_runbook_url_accepts_none() {
        assert!(super::validate_runbook_url(&None).is_ok());
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
