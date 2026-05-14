use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{Extension, Json, extract::State, http::StatusCode};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
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

#[derive(Debug, Deserialize)]
pub struct CreateSloRequest {
    pub service_name: String,
    pub environment: String,
    pub target: f64,
    pub window_days: i32,
    pub burn_rate_fast_threshold: f64,
    pub burn_rate_slow_threshold: f64,
    pub description: Option<String>,
}

#[derive(Serialize)]
pub struct SloListResponse {
    pub items: Vec<SloDefinitionItem>,
}

#[derive(Debug)]
pub enum CreateSloError {
    InvalidInput(String),
    Db(sqlx::Error),
}

impl std::fmt::Display for CreateSloError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CreateSloError::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            CreateSloError::Db(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for CreateSloError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CreateSloError::Db(e) => Some(e),
            CreateSloError::InvalidInput(_) => None,
        }
    }
}

pub fn validate_create_slo(req: &CreateSloRequest) -> Result<(), String> {
    if req.service_name.trim().is_empty() {
        return Err("service_name is required".into());
    }
    if req.environment.trim().is_empty() {
        return Err("environment is required".into());
    }
    if !(req.target > 0.0 && req.target < 1.0) {
        return Err("target must be between 0 and 1".into());
    }
    if req.window_days <= 0 {
        return Err("window_days must be positive".into());
    }
    if req.burn_rate_fast_threshold <= 0.0 || req.burn_rate_slow_threshold <= 0.0 {
        return Err("burn rate thresholds must be positive".into());
    }
    Ok(())
}

pub async fn list_slos(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
) -> Result<Vec<SloDefinitionItem>, sqlx::Error> {
    sqlx::query_as::<_, SloDefinitionItem>(
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
         FROM slo_definitions WHERE tenant_id = $1 ORDER BY created_at DESC",
    )
    .bind(tenant_id)
    .fetch_all(db)
    .await
}

pub async fn create_slo(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    req: &CreateSloRequest,
) -> Result<SloDefinitionItem, CreateSloError> {
    validate_create_slo(req).map_err(CreateSloError::InvalidInput)?;

    let mut tx = db.begin().await.map_err(CreateSloError::Db)?;
    let description = req.description.as_deref().unwrap_or("");
    let slo_id: Uuid = sqlx::query_scalar(
        "INSERT INTO slo_definitions \
         (tenant_id, service_name, environment, sli_type, target, window_days, \
          burn_rate_fast_threshold, burn_rate_slow_threshold, description) \
         VALUES ($1, $2, $3, 'availability', $4, $5, $6, $7, $8) \
         RETURNING slo_id",
    )
    .bind(tenant_id)
    .bind(req.service_name.trim())
    .bind(req.environment.trim())
    .bind(req.target)
    .bind(req.window_days)
    .bind(req.burn_rate_fast_threshold)
    .bind(req.burn_rate_slow_threshold)
    .bind(description)
    .fetch_one(&mut *tx)
    .await
    .map_err(CreateSloError::Db)?;

    let condition = serde_json::json!({
        "slo_id": slo_id,
        "fast_window_minutes": 60,
        "slow_window_minutes": 360,
    });
    let rule_name = format!(
        "{} {} availability burn rate",
        req.service_name.trim(),
        req.environment.trim()
    );
    sqlx::query(
        "INSERT INTO alert_rules (tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, $2, 'slo_burn_rate', 'critical', $3)",
    )
    .bind(tenant_id)
    .bind(rule_name)
    .bind(condition)
    .execute(&mut *tx)
    .await
    .map_err(CreateSloError::Db)?;

    tx.commit().await.map_err(CreateSloError::Db)?;

    let items = list_slos(db, tenant_id).await.map_err(CreateSloError::Db)?;
    items
        .into_iter()
        .find(|item| item.slo_id == slo_id)
        .ok_or_else(|| CreateSloError::InvalidInput("created SLO was not found".into()))
}

pub async fn handle_list_slos(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<SloListResponse>, StatusCode> {
    let items = list_slos(&state.db, ctx.tenant_id).await.map_err(|e| {
        tracing::error!(error = %e, "failed to list SLO definitions");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(SloListResponse { items }))
}

pub async fn handle_create_slo(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<CreateSloRequest>,
) -> Result<(StatusCode, Json<SloDefinitionItem>), StatusCode> {
    match create_slo(&state.db, ctx.tenant_id, &req).await {
        Ok(item) => Ok((StatusCode::CREATED, Json(item))),
        Err(CreateSloError::InvalidInput(msg)) => {
            tracing::warn!(message = %msg, "invalid SLO input");
            Err(StatusCode::BAD_REQUEST)
        }
        Err(CreateSloError::Db(e)) => {
            tracing::error!(error = %e, "failed to create SLO definition");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
