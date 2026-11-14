//! Phase 1 P1 observability asset governance (OBS-34-02, OBS-34-04, OBS-34-05):
//! - Alert threshold versioning, comparison, and restore (`alert_rule_versions`)
//! - Approved critical-alert templates applied consistently to services (`alert_rule_templates`)
//! - Quarterly review of unviewed dashboards and never-triggered alerts with owners.

use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;
use uuid::Uuid;

use crate::AppState;
use crate::middleware::auth::TenantContext;

#[derive(Deserialize)]
pub struct UpdateThresholdRequest {
    pub threshold: f64,
    #[serde(default)]
    pub reason: String,
}

#[derive(Clone, Debug, FromRow, Serialize)]
pub struct RuleVersionRow {
    pub version_id: Uuid,
    pub version_number: i32,
    pub condition: Value,
    pub severity: String,
    pub changed_by: Uuid,
    pub change_reason: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct RuleVersionView {
    #[serde(flatten)]
    pub version: RuleVersionRow,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_threshold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_threshold: Option<f64>,
}

/// `PUT /v1/alerts/rules/{rule_id}/threshold` — updates an alert rule's threshold and records a
/// version snapshot so the previous value can be compared and restored (OBS-34-02).
pub async fn handle_update_threshold(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(rule_id): Path<Uuid>,
    Json(body): Json<UpdateThresholdRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    if !body.threshold.is_finite() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "threshold must be finite"})),
        ));
    }
    let actor = ctx.user_id.unwrap_or(ctx.tenant_id);
    let mut tx = state.db.begin().await.map_err(internal_err)?;

    let existing: Option<(Value, String)> = sqlx::query_as(
        "SELECT condition, severity FROM alert_rules WHERE tenant_id = $1 AND rule_id = $2",
    )
    .bind(ctx.tenant_id)
    .bind(rule_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_err)?;

    let Some((mut condition, severity)) = existing else {
        return Err((StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))));
    };

    let max_version: Option<i32> = sqlx::query_scalar(
        "SELECT MAX(version_number) FROM alert_rule_versions WHERE tenant_id = $1 AND rule_id = $2",
    )
    .bind(ctx.tenant_id)
    .bind(rule_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_err)?;

    let mut next_version = max_version.unwrap_or(0);
    if next_version == 0 {
        sqlx::query(
            "INSERT INTO alert_rule_versions \
             (tenant_id, rule_id, version_number, condition, severity, changed_by, change_reason) \
             VALUES ($1, $2, 1, $3, $4, $5, 'initial')",
        )
        .bind(ctx.tenant_id)
        .bind(rule_id)
        .bind(&condition)
        .bind(&severity)
        .bind(actor)
        .execute(&mut *tx)
        .await
        .map_err(internal_err)?;
        next_version = 1;
    }

    if let Some(obj) = condition.as_object_mut() {
        obj.insert("threshold".into(), json!(body.threshold));
    } else {
        condition = json!({"threshold": body.threshold});
    }

    sqlx::query("UPDATE alert_rules SET condition = $3 WHERE tenant_id = $1 AND rule_id = $2")
        .bind(ctx.tenant_id)
        .bind(rule_id)
        .bind(&condition)
        .execute(&mut *tx)
        .await
        .map_err(internal_err)?;

    let new_ver = next_version + 1;
    let row = sqlx::query_as::<_, RuleVersionRow>(
        "INSERT INTO alert_rule_versions \
         (tenant_id, rule_id, version_number, condition, severity, changed_by, change_reason) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         RETURNING version_id, version_number, condition, severity, changed_by, change_reason, created_at",
    )
    .bind(ctx.tenant_id)
    .bind(rule_id)
    .bind(new_ver)
    .bind(&condition)
    .bind(&severity)
    .bind(actor)
    .bind(body.reason.trim())
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_err)?;

    tx.commit().await.map_err(internal_err)?;
    Ok((StatusCode::OK, Json(row)))
}

/// `GET /v1/alerts/rules/{rule_id}/versions` — lists version history with threshold comparison (OBS-34-02).
pub async fn handle_list_rule_versions(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(rule_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_as::<_, RuleVersionRow>(
        "SELECT version_id, version_number, condition, severity, changed_by, change_reason, created_at \
         FROM alert_rule_versions \
         WHERE tenant_id = $1 AND rule_id = $2 \
         ORDER BY version_number ASC",
    )
    .bind(ctx.tenant_id)
    .bind(rule_id)
    .fetch_all(&state.db)
    .await
    .map_err(internal_err)?;

    let mut views = Vec::with_capacity(rows.len());
    let mut prev_threshold: Option<f64> = None;
    for r in rows {
        let cur = r.condition.get("threshold").and_then(Value::as_f64);
        views.push(RuleVersionView {
            version: r,
            previous_threshold: prev_threshold,
            current_threshold: cur,
        });
        prev_threshold = cur;
    }
    Ok(Json(json!({ "items": views })))
}

/// `POST /v1/alerts/rules/{rule_id}/versions/{version_number}/restore` — restores a rule to a
/// prior version and records the restoration as a new version (OBS-34-02).
pub async fn handle_restore_rule_version(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path((rule_id, version_number)): Path<(Uuid, i32)>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    let actor = ctx.user_id.unwrap_or(ctx.tenant_id);
    let mut tx = state.db.begin().await.map_err(internal_err)?;

    let target: Option<(Value, String)> = sqlx::query_as(
        "SELECT condition, severity FROM alert_rule_versions \
         WHERE tenant_id = $1 AND rule_id = $2 AND version_number = $3",
    )
    .bind(ctx.tenant_id)
    .bind(rule_id)
    .bind(version_number)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_err)?;

    let Some((condition, severity)) = target else {
        return Err((StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))));
    };

    sqlx::query(
        "UPDATE alert_rules SET condition = $3, severity = $4 WHERE tenant_id = $1 AND rule_id = $2",
    )
    .bind(ctx.tenant_id)
    .bind(rule_id)
    .bind(&condition)
    .bind(&severity)
    .execute(&mut *tx)
    .await
    .map_err(internal_err)?;

    let max_version: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_number), 0) FROM alert_rule_versions WHERE tenant_id = $1 AND rule_id = $2",
    )
    .bind(ctx.tenant_id)
    .bind(rule_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_err)?;

    let row = sqlx::query_as::<_, RuleVersionRow>(
        "INSERT INTO alert_rule_versions \
         (tenant_id, rule_id, version_number, condition, severity, changed_by, change_reason) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         RETURNING version_id, version_number, condition, severity, changed_by, change_reason, created_at",
    )
    .bind(ctx.tenant_id)
    .bind(rule_id)
    .bind(max_version + 1)
    .bind(&condition)
    .bind(&severity)
    .bind(actor)
    .bind(format!("restore version {version_number}"))
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_err)?;

    tx.commit().await.map_err(internal_err)?;
    Ok((StatusCode::OK, Json(row)))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TemplateRuleSpec {
    pub name: String,
    #[serde(default = "default_alert_type")]
    pub alert_type: String,
    #[serde(default = "default_severity")]
    pub severity: String,
    pub condition: Value,
}

fn default_alert_type() -> String {
    "threshold".into()
}

fn default_severity() -> String {
    "critical".into()
}

#[derive(Deserialize)]
pub struct CreateTemplateRequest {
    pub name: String,
    #[serde(default)]
    pub approved: bool,
    pub rules: Vec<TemplateRuleSpec>,
}

#[derive(Debug, FromRow, Serialize)]
pub struct TemplateRow {
    pub template_id: Uuid,
    pub name: String,
    pub approved: bool,
    pub approved_by: Option<Uuid>,
    pub rules: Value,
    pub created_at: DateTime<Utc>,
}

/// `POST /v1/alerts/templates` — creates an observability alert template (OBS-34-04).
pub async fn handle_create_template(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(body): Json<CreateTemplateRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    let name = body.name.trim();
    if name.is_empty() || body.rules.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "template name and rules are required"})),
        ));
    }
    let approved_by = body.approved.then(|| ctx.user_id.unwrap_or(ctx.tenant_id));
    let rules_json = serde_json::to_value(&body.rules).map_err(internal_err)?;
    let row = sqlx::query_as::<_, TemplateRow>(
        "INSERT INTO alert_rule_templates (tenant_id, name, approved, approved_by, rules) \
         VALUES ($1, $2, $3, $4, $5) \
         RETURNING template_id, name, approved, approved_by, rules, created_at",
    )
    .bind(ctx.tenant_id)
    .bind(name)
    .bind(body.approved)
    .bind(approved_by)
    .bind(rules_json)
    .fetch_one(&state.db)
    .await
    .map_err(internal_err)?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[derive(Deserialize)]
pub struct ApplyTemplateRequest {
    pub service_name: String,
    #[serde(default = "default_env")]
    pub environment: String,
}

fn default_env() -> String {
    "production".into()
}

/// `POST /v1/alerts/templates/{template_id}/apply` — applies an approved template's standard
/// critical-alert set to a service in a target environment (OBS-34-04).
pub async fn handle_apply_template(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(template_id): Path<Uuid>,
    Json(body): Json<ApplyTemplateRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    let svc = body.service_name.trim();
    if svc.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "service_name is required"})),
        ));
    }

    let tmpl: Option<(bool, Value)> = sqlx::query_as(
        "SELECT approved, rules FROM alert_rule_templates WHERE tenant_id = $1 AND template_id = $2",
    )
    .bind(ctx.tenant_id)
    .bind(template_id)
    .fetch_optional(&state.db)
    .await
    .map_err(internal_err)?;

    let Some((approved, rules_val)) = tmpl else {
        return Err((StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))));
    };
    if !approved {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "template_not_approved"})),
        ));
    }

    let specs: Vec<TemplateRuleSpec> = serde_json::from_value(rules_val).map_err(internal_err)?;
    let mut created_ids = Vec::with_capacity(specs.len());
    let mut tx = state.db.begin().await.map_err(internal_err)?;
    for spec in specs {
        let rule_name = format!("{svc} [{}]: {}", body.environment.trim(), spec.name);
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO alert_rules (tenant_id, name, alert_type, severity, condition, service_name) \
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING rule_id",
        )
        .bind(ctx.tenant_id)
        .bind(rule_name)
        .bind(&spec.alert_type)
        .bind(&spec.severity)
        .bind(&spec.condition)
        .bind(svc)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_err)?;
        created_ids.push(id);
    }
    tx.commit().await.map_err(internal_err)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "template_id": template_id,
            "service_name": svc,
            "environment": body.environment.trim(),
            "created_rule_ids": created_ids,
        })),
    ))
}

#[derive(FromRow, Serialize)]
pub struct UnviewedDashboardItem {
    pub dashboard_id: Uuid,
    pub name: String,
    pub owner_kind: Option<String>,
    pub owner_id: Option<Uuid>,
    pub last_viewed_at: Option<DateTime<Utc>>,
}

#[derive(FromRow, Serialize)]
pub struct NeverTriggeredAlertItem {
    pub rule_id: Uuid,
    pub name: String,
    pub service_name: Option<String>,
    pub owner_kind: Option<String>,
    pub owner_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// `GET /v1/alerts/assets/review` — quarterly review listing unviewed dashboards and
/// never-triggered alerts with their owners (OBS-34-05).
pub async fn handle_quarterly_asset_review(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    let unviewed_dashboards = sqlx::query_as::<_, UnviewedDashboardItem>(
        "SELECT dashboard_id, name, owner_kind, owner_id, last_viewed_at \
         FROM dashboards \
         WHERE tenant_id = $1 AND (last_viewed_at IS NULL OR last_viewed_at < now() - interval '90 days') \
         ORDER BY created_at ASC",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&state.db)
    .await
    .map_err(internal_err)?;

    let never_triggered_alerts = sqlx::query_as::<_, NeverTriggeredAlertItem>(
        "SELECT r.rule_id, r.name, r.service_name, r.owner_kind, r.owner_id, r.created_at \
         FROM alert_rules r \
         WHERE r.tenant_id = $1 \
           AND NOT EXISTS (SELECT 1 FROM alert_firings f WHERE f.rule_id = r.rule_id) \
         ORDER BY r.created_at ASC",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&state.db)
    .await
    .map_err(internal_err)?;

    Ok(Json(json!({
        "unviewed_dashboards": unviewed_dashboards,
        "never_triggered_alerts": never_triggered_alerts,
    })))
}

fn internal_err<E: std::fmt::Display>(err: E) -> (StatusCode, Json<Value>) {
    tracing::error!(error = %err, "asset governance error");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": "internal"})),
    )
}
