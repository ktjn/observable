// Alert rule mutation endpoints.
//
// POST   /v1/admin/alerts/rules               — create alert rule
// PATCH  /v1/admin/alerts/rules/{id}/silence  — silence / unsilence
// PATCH  /v1/admin/alerts/rules/{id}/runbook  — update runbook URL
// PATCH  /v1/admin/alerts/rules/{id}          — update service_name

use crate::AdminServiceAppState;
use crate::middleware::auth::TenantContext;
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const VALID_OPERATORS: &[&str] = &["gt", "gte", "lt", "lte", "eq"];

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
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<CreateRuleRequest>,
) -> Result<(StatusCode, Json<CreateRuleResponse>), StatusCode> {
    if req.name.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

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
                  auto_trigger_incident, runbook_url, service_name) \
                 VALUES ($1, $2, 'threshold', 'warning', $3, $4, $5, $6, $7) \
                 RETURNING rule_id",
            )
            .bind(ctx.tenant_id)
            .bind(&req.name)
            .bind(&condition)
            .bind(&channels)
            .bind(auto_trigger)
            .bind(req.runbook_url.as_deref())
            .bind(req.service_name.as_deref())
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
                  auto_trigger_incident, runbook_url, service_name) \
                 VALUES ($1, $2, 'deadman', 'warning', $3, $4, $5, $6, $7) \
                 RETURNING rule_id",
            )
            .bind(ctx.tenant_id)
            .bind(&req.name)
            .bind(&condition)
            .bind(&channels)
            .bind(auto_trigger)
            .bind(req.runbook_url.as_deref())
            .bind(service_name.as_deref())
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
                  auto_trigger_incident, runbook_url, service_name) \
                 VALUES ($1, $2, 'change_detection', 'warning', $3, $4, $5, $6, $7) \
                 RETURNING rule_id",
            )
            .bind(ctx.tenant_id)
            .bind(&req.name)
            .bind(&condition)
            .bind(&channels)
            .bind(auto_trigger)
            .bind(req.runbook_url.as_deref())
            .bind(req.service_name.as_deref())
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
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(rule_id): Path<Uuid>,
    Json(req): Json<SilenceRequest>,
) -> Result<StatusCode, StatusCode> {
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
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(rule_id): Path<Uuid>,
    Json(req): Json<UpdateRunbookRequest>,
) -> Result<StatusCode, StatusCode> {
    if let Some(url) = &req.runbook_url
        && !url.starts_with("http://")
        && !url.starts_with("https://")
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let updated: Option<Uuid> = sqlx::query_scalar(
        "UPDATE alert_rules SET runbook_url = $1 \
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
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(rule_id): Path<Uuid>,
    Json(req): Json<UpdateRuleRequest>,
) -> Result<StatusCode, StatusCode> {
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
}
