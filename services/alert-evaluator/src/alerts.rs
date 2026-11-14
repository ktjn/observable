// Alert rule read and mutation endpoints.
//
// GET    /v1/alerts/rules                      — list alert rules (lifecycle state)
// GET    /v1/alerts/rules/{id}                 — get alert rule detail with firings
// POST   /v1/admin/alerts/rules               — create alert rule
// PATCH  /v1/admin/alerts/rules/{id}/silence  — silence / unsilence
// PATCH  /v1/admin/alerts/rules/{id}/runbook  — update runbook URL
// PATCH  /v1/admin/alerts/rules/{id}          — update service_name

use crate::AppState;
use crate::middleware::auth::{TenantContext, require_admin};
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
    /// Accountable owner (OBS-34-01): `user` or `team`, or none when ownerless.
    pub owner_kind: Option<String>,
    pub owner_id: Option<Uuid>,
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
    pub runbook_reviewed_at: Option<DateTime<Utc>>,
    pub guidance_status: String,
    pub guidance_flagged_to_owner: bool,
    pub owner_kind: Option<String>,
    pub owner_id: Option<Uuid>,
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
    runbook_reviewed_at: Option<DateTime<Utc>>,
    owner_kind: Option<String>,
    owner_id: Option<Uuid>,
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
    owner_kind: Option<String>,
    owner_id: Option<Uuid>,
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
         ) AS suppressed, r.owner_kind, r.owner_id \
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
                    owner_kind: row.owner_kind,
                    owner_id: row.owner_id,
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
         r.runbook_reviewed_at, r.owner_kind, r.owner_id, \
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

    let (guidance_status, guidance_flagged_to_owner) = assess_guidance_currency(
        &row.severity,
        row.runbook_url.as_deref(),
        row.runbook_reviewed_at,
        Utc::now(),
        90,
    );

    Ok(Some(AlertRuleDetailResponse {
        rule_id: row.rule_id,
        name: row.name,
        severity: row.severity,
        alert_type: row.alert_type,
        condition: row.condition,
        silenced: row.silenced,
        firing: row.firing,
        runbook_url: row.runbook_url,
        runbook_reviewed_at: row.runbook_reviewed_at,
        guidance_status: guidance_status.to_string(),
        guidance_flagged_to_owner,
        owner_kind: row.owner_kind,
        owner_id: row.owner_id,
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

#[derive(Serialize)]
pub struct CreateRuleResponse {
    pub rule_id: Uuid,
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
    /// Defaults to the calling user; an API-key caller without one creates an ownerless rule.
    pub owner: Option<OwnerRef>,
}

/// An accountable owner principal (OBS-34-01). Whether the principal is still active is
/// reported by the catalog asset registry, which knows the directory.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct OwnerRef {
    pub principal_kind: String,
    pub principal_id: Uuid,
}

fn known_kind(o: &OwnerRef) -> bool {
    matches!(o.principal_kind.as_str(), "user" | "team")
}

/// The explicit owner, else the calling user. `Err` for an unknown principal kind.
pub fn resolve_owner(
    explicit: Option<&OwnerRef>,
    ctx: &TenantContext,
) -> Result<Option<OwnerRef>, StatusCode> {
    match explicit {
        Some(o) if known_kind(o) => Ok(Some(o.clone())),
        Some(_) => Err(StatusCode::BAD_REQUEST),
        None => Ok(ctx.user_id.map(|principal_id| OwnerRef {
            principal_kind: "user".into(),
            principal_id,
        })),
    }
}

#[derive(Deserialize)]
pub struct UpdateOwnerRequest {
    /// `null` clears the owner, which the catalog then reports as ownerless.
    pub owner: Option<OwnerRef>,
}

impl UpdateOwnerRequest {
    pub fn validate(&self) -> Result<(), StatusCode> {
        match &self.owner {
            Some(o) if !known_kind(o) => Err(StatusCode::BAD_REQUEST),
            _ => Ok(()),
        }
    }
}

#[derive(Deserialize)]
pub struct SilenceRequest {
    pub silenced: bool,
}

#[derive(Deserialize)]
pub struct UpdateRunbookRequest {
    pub runbook_url: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateRuleRequest {
    pub service_name: Option<String>,
}

pub async fn handle_create_rule(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<CreateRuleRequest>,
) -> Result<(StatusCode, Json<CreateRuleResponse>), StatusCode> {
    require_admin(&ctx)?;
    if req.name.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let owner = resolve_owner(req.owner.as_ref(), &ctx)?;
    let owner_kind = owner.as_ref().map(|o| o.principal_kind.as_str());
    let owner_id = owner.as_ref().map(|o| o.principal_id);

    let alert_type = req.alert_type.as_deref().unwrap_or("threshold");

    let rule_id: Result<Uuid, _> = match alert_type {
        "threshold" => {
            if req.metric_name.trim().is_empty() {
                return Err(StatusCode::BAD_REQUEST);
            }
            if !VALID_OPERATORS.contains(&req.operator.as_str()) {
                return Err(StatusCode::BAD_REQUEST);
            }
            if !req.threshold.is_finite() {
                return Err(StatusCode::BAD_REQUEST);
            }
            let condition = serde_json::json!({
                "metric_name": req.metric_name,
                "operator": req.operator,
                "threshold": req.threshold,
            });
            let channels = req.notification_channels.clone().unwrap_or_default();
            let auto_trigger = req.auto_trigger_incident.unwrap_or(true);
            sqlx::query_scalar(
                "INSERT INTO alert_rules \
                 (tenant_id, name, alert_type, severity, condition, notification_channels, \
                  auto_trigger_incident, runbook_url, service_name, owner_kind, owner_id) \
                 VALUES ($1, $2, 'threshold', 'warning', $3, $4, $5, $6, $7, $8, $9) \
                 RETURNING rule_id",
            )
            .bind(ctx.tenant_id)
            .bind(&req.name)
            .bind(&condition)
            .bind(&channels)
            .bind(auto_trigger)
            .bind(req.runbook_url.as_deref())
            .bind(req.service_name.as_deref())
            .bind(owner_kind)
            .bind(owner_id)
            .fetch_one(&state.db)
            .await
        }
        "deadman" => {
            let service_name = req
                .service_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let window_secs = req.window_secs.ok_or(StatusCode::BAD_REQUEST)?;
            if window_secs <= 0 {
                return Err(StatusCode::BAD_REQUEST);
            }
            let condition = serde_json::json!({
                "window_secs": window_secs,
            });
            let channels = req.notification_channels.clone().unwrap_or_default();
            let auto_trigger = req.auto_trigger_incident.unwrap_or(true);
            sqlx::query_scalar(
                "INSERT INTO alert_rules \
                 (tenant_id, name, alert_type, severity, condition, notification_channels, \
                  auto_trigger_incident, runbook_url, service_name, owner_kind, owner_id) \
                 VALUES ($1, $2, 'deadman', 'warning', $3, $4, $5, $6, $7, $8, $9) \
                 RETURNING rule_id",
            )
            .bind(ctx.tenant_id)
            .bind(&req.name)
            .bind(&condition)
            .bind(&channels)
            .bind(auto_trigger)
            .bind(req.runbook_url.as_deref())
            .bind(service_name.as_deref())
            .bind(owner_kind)
            .bind(owner_id)
            .fetch_one(&state.db)
            .await
        }
        "change_detection" => {
            if req.metric_name.trim().is_empty() {
                return Err(StatusCode::BAD_REQUEST);
            }
            let window_secs = req.window_secs.ok_or(StatusCode::BAD_REQUEST)?;
            let baseline_offset_secs = req.baseline_offset_secs.ok_or(StatusCode::BAD_REQUEST)?;
            let threshold_percent = req.threshold_percent.ok_or(StatusCode::BAD_REQUEST)?;
            if window_secs <= 0 || baseline_offset_secs <= 0 {
                return Err(StatusCode::BAD_REQUEST);
            }
            if !threshold_percent.is_finite() || threshold_percent < 0.0 {
                return Err(StatusCode::BAD_REQUEST);
            }
            let condition = serde_json::json!({
                "metric_name": req.metric_name,
                "window_secs": window_secs,
                "baseline_offset_secs": baseline_offset_secs,
                "threshold_percent": threshold_percent,
            });
            let channels = req.notification_channels.clone().unwrap_or_default();
            let auto_trigger = req.auto_trigger_incident.unwrap_or(true);
            sqlx::query_scalar(
                "INSERT INTO alert_rules \
                 (tenant_id, name, alert_type, severity, condition, notification_channels, \
                  auto_trigger_incident, runbook_url, service_name, owner_kind, owner_id) \
                 VALUES ($1, $2, 'change_detection', 'warning', $3, $4, $5, $6, $7, $8, $9) \
                 RETURNING rule_id",
            )
            .bind(ctx.tenant_id)
            .bind(&req.name)
            .bind(&condition)
            .bind(&channels)
            .bind(auto_trigger)
            .bind(req.runbook_url.as_deref())
            .bind(req.service_name.as_deref())
            .bind(owner_kind)
            .bind(owner_id)
            .fetch_one(&state.db)
            .await
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    match rule_id {
        Ok(rule_id) => Ok((StatusCode::CREATED, Json(CreateRuleResponse { rule_id }))),
        Err(e) => {
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
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;
    let updated: Option<Uuid> = sqlx::query_scalar(
        "UPDATE alert_rules SET silenced = $1 \
         WHERE rule_id = $2 AND tenant_id = $3 \
         RETURNING rule_id",
    )
    .bind(req.silenced)
    .bind(rule_id)
    .bind(ctx.tenant_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to silence alert rule");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match updated {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn handle_update_rule_runbook(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(rule_id): Path<Uuid>,
    Json(req): Json<UpdateRunbookRequest>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;
    if let Some(url) = &req.runbook_url
        && !url.starts_with("http://")
        && !url.starts_with("https://")
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let updated: Option<Uuid> = sqlx::query_scalar(
        "UPDATE alert_rules \
         SET runbook_url = $1, \
             runbook_reviewed_at = CASE WHEN $1 IS NOT NULL THEN NOW() ELSE NULL END \
         WHERE rule_id = $2 AND tenant_id = $3 \
         RETURNING rule_id",
    )
    .bind(req.runbook_url.as_deref())
    .bind(rule_id)
    .bind(ctx.tenant_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to update runbook URL");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match updated {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn handle_update_rule(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(rule_id): Path<Uuid>,
    Json(req): Json<UpdateRuleRequest>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;
    let updated: Option<Uuid> = sqlx::query_scalar(
        "UPDATE alert_rules SET service_name = $1 \
         WHERE rule_id = $2 AND tenant_id = $3 \
         RETURNING rule_id",
    )
    .bind(req.service_name.as_deref())
    .bind(rule_id)
    .bind(ctx.tenant_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to update alert rule");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match updated {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// PUT /v1/admin/alerts/rules/{rule_id}/owner (tenant_admin): reassigns or clears the owner.
pub async fn handle_update_rule_owner(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(rule_id): Path<Uuid>,
    Json(req): Json<UpdateOwnerRequest>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;
    req.validate()?;
    let updated: Option<Uuid> = sqlx::query_scalar(
        "UPDATE alert_rules SET owner_kind = $1, owner_id = $2 \
         WHERE rule_id = $3 AND tenant_id = $4 \
         RETURNING rule_id",
    )
    .bind(req.owner.as_ref().map(|o| o.principal_kind.as_str()))
    .bind(req.owner.as_ref().map(|o| o.principal_id))
    .bind(rule_id)
    .bind(ctx.tenant_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to update alert rule owner");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match updated {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err(StatusCode::NOT_FOUND),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_rule_request_deserializes() {
        let json = r#"{"name":"test","metric_name":"cpu","operator":"gt","threshold":90.0}"#;
        let req: CreateRuleRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "test");
        assert!(req.service_name.is_none());
    }

    #[test]
    fn update_rule_request_deserializes_service_name() {
        let json = r#"{"service_name":"payments"}"#;
        let req: UpdateRuleRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.service_name, Some("payments".into()));
    }

    #[test]
    fn update_rule_request_deserializes_null_service_name() {
        let json = r#"{"service_name":null}"#;
        let req: UpdateRuleRequest = serde_json::from_str(json).unwrap();
        assert!(req.service_name.is_none());
    }

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
            runbook_reviewed_at: None,
            guidance_status: "current".into(),
            guidance_flagged_to_owner: false,
            owner_kind: None,
            owner_id: None,
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
            owner_kind: None,
            owner_id: None,
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
            owner_kind: None,
            owner_id: None,
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

    #[test]
    fn route_critical_alert_fails_over_when_primary_unavailable() {
        let plan = RoutingPlan {
            primary: ResponderTarget {
                id: "oncall-primary".into(),
                group: "checkout-sre".into(),
                available: false,
            },
            fallback: ResponderTarget {
                id: "oncall-secondary".into(),
                group: "checkout-sre".into(),
                available: true,
            },
        };
        let routed = route_critical_alert(&plan).expect("should fail over to fallback");
        assert_eq!(routed.target.id, "oncall-secondary");
        assert_eq!(routed.failover_from.as_deref(), Some("oncall-primary"));
    }

    #[test]
    fn alert_payload_missing_service_or_support_route_is_rejected_before_delivery() {
        let missing_route = AlertNotificationPayload {
            service_name: "checkout".into(),
            severity: "critical".into(),
            detected_condition: "error_rate > 0.05".into(),
            occurred_at: Utc::now(),
            next_action: String::new(),
            support_route: String::new(),
        };
        let errs = validate_alert_payload_before_delivery(&missing_route).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.contains("next_action or support_route"))
        );
    }

    #[test]
    fn high_volume_low_impact_never_receives_critical_severity() {
        let sev = classify_alert_severity(BusinessImpact::Low, Urgency::Low, 1_000_000);
        assert_ne!(sev, "critical");
        assert_eq!(sev, "info");

        let crit = classify_alert_severity(BusinessImpact::CustomerCritical, Urgency::Immediate, 5);
        assert_eq!(crit, "critical");
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponderTarget {
    pub id: String,
    pub group: String,
    pub available: bool,
}

#[derive(Debug, Clone)]
pub struct RoutingPlan {
    pub primary: ResponderTarget,
    pub fallback: ResponderTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutedNotification {
    pub target: ResponderTarget,
    pub failover_from: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutingError {
    NoAccountableResponder,
}

/// Routes a critical alert to the designated primary responder/group, failing over to the
/// fallback accountable responder when the primary is unavailable (OBS-07-01).
pub fn route_critical_alert(plan: &RoutingPlan) -> Result<RoutedNotification, RoutingError> {
    if plan.primary.available {
        return Ok(RoutedNotification {
            target: plan.primary.clone(),
            failover_from: None,
        });
    }
    if plan.fallback.available {
        return Ok(RoutedNotification {
            target: plan.fallback.clone(),
            failover_from: Some(plan.primary.id.clone()),
        });
    }
    Err(RoutingError::NoAccountableResponder)
}

#[derive(Debug, Clone)]
pub struct AlertNotificationPayload {
    pub service_name: String,
    pub severity: String,
    pub detected_condition: String,
    pub occurred_at: DateTime<Utc>,
    pub next_action: String,
    pub support_route: String,
}

/// Validates that an alert states service, severity, condition, time, and next action/support
/// route before delivery (OBS-07-02).
pub fn validate_alert_payload_before_delivery(
    payload: &AlertNotificationPayload,
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if payload.service_name.trim().is_empty() {
        errors.push("missing service_name".into());
    }
    if payload.severity.trim().is_empty() {
        errors.push("missing severity".into());
    }
    if payload.detected_condition.trim().is_empty() {
        errors.push("missing detected_condition".into());
    }
    if payload.next_action.trim().is_empty() && payload.support_route.trim().is_empty() {
        errors.push("missing next_action or support_route".into());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusinessImpact {
    Low,
    Medium,
    High,
    CustomerCritical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Low,
    Normal,
    High,
    Immediate,
}

/// Classifies alert severity by business impact and urgency rather than raw event volume alone (OBS-07-03).
pub fn classify_alert_severity(
    impact: BusinessImpact,
    urgency: Urgency,
    _event_volume: u64,
) -> &'static str {
    match (impact, urgency) {
        (BusinessImpact::CustomerCritical, Urgency::High | Urgency::Immediate) => "critical",
        (BusinessImpact::CustomerCritical | BusinessImpact::High, _) => "warning",
        (BusinessImpact::Medium, Urgency::High | Urgency::Immediate) => "warning",
        _ => "info",
    }
}

/// Evaluates whether a critical alert's operational guidance is current, missing, or past review (OBS-33-01).
pub fn assess_guidance_currency(
    severity: &str,
    runbook_url: Option<&str>,
    runbook_reviewed_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    max_age_days: i64,
) -> (&'static str, bool) {
    let has_url = runbook_url.is_some_and(|u| !u.trim().is_empty());
    let is_critical = severity.eq_ignore_ascii_case("critical");
    if !has_url {
        return ("missing", is_critical);
    }
    match runbook_reviewed_at {
        Some(reviewed) if now.signed_duration_since(reviewed).num_days() <= max_age_days => {
            ("current", false)
        }
        _ => ("past_review", is_critical),
    }
}

#[cfg(test)]
mod guidance_tests {
    use super::*;

    #[test]
    fn critical_alert_missing_or_stale_guidance_is_flagged_to_owner() {
        let now = Utc::now();
        assert_eq!(
            assess_guidance_currency("critical", None, None, now, 90),
            ("missing", true)
        );
        let stale = now - chrono::Duration::days(120);
        assert_eq!(
            assess_guidance_currency(
                "critical",
                Some("https://runbooks.example.com/r"),
                Some(stale),
                now,
                90
            ),
            ("past_review", true)
        );
        let fresh = now - chrono::Duration::days(10);
        assert_eq!(
            assess_guidance_currency(
                "critical",
                Some("https://runbooks.example.com/r"),
                Some(fresh),
                now,
                90
            ),
            ("current", false)
        );
    }
}

// ---------------------------------------------------------------------------
// Phase 5 P1: OBS-07-05, OBS-07-06, OBS-07-09, OBS-07-10, OBS-07-11, OBS-33-03
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingNotification {
    pub dedup_key: String,
    pub service: String,
    pub environment: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeduplicatedNotification {
    pub dedup_key: String,
    pub service: String,
    pub environment: String,
    pub suppressed_duplicates: usize,
}

/// Collapses duplicate/related notifications sharing a `dedup_key` (OBS-07-05).
pub fn deduplicate_notifications(
    notifications: &[PendingNotification],
) -> Vec<DeduplicatedNotification> {
    let mut grouped: std::collections::BTreeMap<String, DeduplicatedNotification> =
        std::collections::BTreeMap::new();
    for n in notifications {
        grouped
            .entry(n.dedup_key.clone())
            .and_modify(|entry| entry.suppressed_duplicates += 1)
            .or_insert_with(|| DeduplicatedNotification {
                dedup_key: n.dedup_key.clone(),
                service: n.service.clone(),
                environment: n.environment.clone(),
                suppressed_duplicates: 0,
            });
    }
    grouped.into_values().collect()
}

#[derive(Debug, Clone)]
pub struct MaintenanceWindow {
    pub approved: bool,
    pub service: String,
    pub environment: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

/// Returns true only when an approved maintenance window covers the notification's
/// service, environment, and timestamp (OBS-07-06).
pub fn is_suppressed_by_maintenance(
    service: &str,
    environment: &str,
    at: DateTime<Utc>,
    windows: &[MaintenanceWindow],
) -> bool {
    windows.iter().any(|w| {
        w.approved
            && w.service == service
            && w.environment == environment
            && at >= w.starts_at
            && at <= w.ends_at
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationOutcome {
    GeneratedOnly,
    Delivered,
    Acknowledged,
    Unhandled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NotificationOutcomeSummary {
    pub generated: usize,
    pub delivered: usize,
    pub acknowledged: usize,
    pub unhandled: usize,
}

/// Summarizes generated, delivered, acknowledged, and unhandled notifications (OBS-07-09).
pub fn summarize_notification_outcomes(
    outcomes: &[NotificationOutcome],
) -> NotificationOutcomeSummary {
    let mut summary = NotificationOutcomeSummary {
        generated: outcomes.len(),
        ..Default::default()
    };
    for o in outcomes {
        match o {
            NotificationOutcome::GeneratedOnly => {}
            NotificationOutcome::Delivered => summary.delivered += 1,
            NotificationOutcome::Acknowledged => {
                summary.delivered += 1;
                summary.acknowledged += 1;
            }
            NotificationOutcome::Unhandled => {
                summary.delivered += 1;
                summary.unhandled += 1;
            }
        }
    }
    summary
}

/// Verifies an alert threshold condition against a controlled sample before activation (OBS-07-10).
pub fn dry_run_alert_threshold(samples: &[f64], threshold: f64) -> bool {
    samples.iter().any(|v| *v > threshold)
}

/// Selects primary delivery route or falls back to secondary when primary fails or is unacknowledged (OBS-07-11).
pub fn select_failover_delivery_route<'a>(
    primary_route: &'a str,
    secondary_route: &'a str,
    primary_blocked: bool,
    primary_unacknowledged: bool,
) -> &'a str {
    if primary_blocked || primary_unacknowledged {
        secondary_route
    } else {
        primary_route
    }
}

#[derive(Debug, Clone)]
pub struct KnownErrorRecord {
    pub symptom_keyword: String,
    pub title: String,
    pub workaround: String,
}

/// Surfaces documented workarounds matching a recurring symptom (OBS-33-03).
pub fn match_known_workarounds<'a>(
    symptom: &str,
    records: &'a [KnownErrorRecord],
) -> Vec<&'a KnownErrorRecord> {
    let needle = symptom.to_ascii_lowercase();
    records
        .iter()
        .filter(|r| needle.contains(&r.symptom_keyword.to_ascii_lowercase()))
        .collect()
}

#[cfg(test)]
mod p1_alerting_tests {
    use super::*;

    #[test]
    fn duplicate_failure_notifications_collapse_to_single_group() {
        let items = vec![
            PendingNotification {
                dedup_key: "checkout:db_pool".into(),
                service: "checkout".into(),
                environment: "prod".into(),
                message: "pool exhausted 1".into(),
            },
            PendingNotification {
                dedup_key: "checkout:db_pool".into(),
                service: "checkout".into(),
                environment: "prod".into(),
                message: "pool exhausted 2".into(),
            },
            PendingNotification {
                dedup_key: "checkout:db_pool".into(),
                service: "checkout".into(),
                environment: "prod".into(),
                message: "pool exhausted 3".into(),
            },
        ];
        let deduped = deduplicate_notifications(&items);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].suppressed_duplicates, 2);
    }

    #[test]
    fn approved_maintenance_suppresses_only_eligible_notifications() {
        let now = Utc::now();
        let windows = vec![MaintenanceWindow {
            approved: true,
            service: "checkout".into(),
            environment: "staging".into(),
            starts_at: now - chrono::Duration::minutes(10),
            ends_at: now + chrono::Duration::minutes(20),
        }];
        assert!(is_suppressed_by_maintenance(
            "checkout", "staging", now, &windows
        ));
        assert!(!is_suppressed_by_maintenance(
            "checkout", "prod", now, &windows
        ));
        assert!(!is_suppressed_by_maintenance(
            "payments", "staging", now, &windows
        ));
    }

    #[test]
    fn notification_review_distinguishes_generated_delivered_ack_and_unhandled() {
        let summary = summarize_notification_outcomes(&[
            NotificationOutcome::GeneratedOnly,
            NotificationOutcome::Delivered,
            NotificationOutcome::Acknowledged,
            NotificationOutcome::Unhandled,
        ]);
        assert_eq!(summary.generated, 4);
        assert_eq!(summary.delivered, 3);
        assert_eq!(summary.acknowledged, 1);
        assert_eq!(summary.unhandled, 1);
    }

    #[test]
    fn new_critical_alert_verified_on_controlled_condition_before_paging() {
        assert!(dry_run_alert_threshold(&[0.01, 0.02, 0.12], 0.05));
        assert!(!dry_run_alert_threshold(&[0.01, 0.02, 0.03], 0.05));
    }

    #[test]
    fn blocked_or_unacknowledged_primary_channel_routes_to_secondary() {
        assert_eq!(
            select_failover_delivery_route("pagerduty", "slack-fallback", true, false),
            "slack-fallback"
        );
        assert_eq!(
            select_failover_delivery_route("pagerduty", "slack-fallback", false, true),
            "slack-fallback"
        );
        assert_eq!(
            select_failover_delivery_route("pagerduty", "slack-fallback", false, false),
            "pagerduty"
        );
    }

    #[test]
    fn recurring_symptom_surfaces_documented_workaround() {
        let known = vec![KnownErrorRecord {
            symptom_keyword: "connection pool exhausted".into(),
            title: "PG Pool Saturation".into(),
            workaround: "Scale pgbouncer pool_size and recycle idle connections".into(),
        }];
        let matches = match_known_workarounds(
            "checkout-api: Postgres connection pool exhausted on primary",
            &known,
        );
        assert_eq!(matches.len(), 1);
        assert!(matches[0].workaround.contains("Scale pgbouncer"));
    }
}
