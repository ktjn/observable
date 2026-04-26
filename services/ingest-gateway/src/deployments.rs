use crate::auth::TenantContext;
use crate::AppState;
use axum::{
    extract::{Extension, Json, Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct CreateDeploymentRequest {
    pub service_name: String,
    pub environment: String,
    pub service_version: String,
    pub project_id: Option<Uuid>,
    pub deployed_by: Option<String>,
    pub commit_sha: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct CreateDeploymentResponse {
    pub deployment_id: Uuid,
}

#[derive(Deserialize)]
pub struct FinishDeploymentRequest {
    pub status: String,
    pub finished_at: Option<DateTime<Utc>>,
    pub rollback_of: Option<Uuid>,
}

pub async fn create_deployment(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<CreateDeploymentRequest>,
) -> Result<(StatusCode, Json<CreateDeploymentResponse>), StatusCode> {
    if req.service_name.trim().is_empty()
        || req.environment.trim().is_empty()
        || req.service_version.trim().is_empty()
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    let deployment_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_markers \
         (tenant_id, project_id, service_name, environment, service_version, \
          status, deployed_by, commit_sha, metadata) \
         VALUES ($1, $2, $3, $4, $5, 'in_progress', $6, $7, $8) \
         RETURNING deployment_id",
    )
    .bind(ctx.tenant_id)
    .bind(req.project_id)
    .bind(&req.service_name)
    .bind(&req.environment)
    .bind(&req.service_version)
    .bind(&req.deployed_by)
    .bind(&req.commit_sha)
    .bind(&req.metadata)
    .fetch_one(state.db.as_ref())
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to create deployment marker");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::CREATED,
        Json(CreateDeploymentResponse { deployment_id }),
    ))
}

pub async fn finish_deployment(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(deployment_id): Path<Uuid>,
    Json(req): Json<FinishDeploymentRequest>,
) -> Result<StatusCode, StatusCode> {
    let allowed = ["success", "failed", "rolled_back"];
    if !allowed.contains(&req.status.as_str()) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    let finished_at = req.finished_at.unwrap_or_else(Utc::now);

    let result = sqlx::query(
        "UPDATE deployment_markers \
         SET status = $1, finished_at = $2, rollback_of = $3 \
         WHERE deployment_id = $4 AND tenant_id = $5",
    )
    .bind(&req.status)
    .bind(finished_at)
    .bind(req.rollback_of)
    .bind(deployment_id)
    .bind(ctx.tenant_id)
    .execute(state.db.as_ref())
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to finish deployment marker");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_allows_success() {
        let allowed = ["success", "failed", "rolled_back"];
        assert!(allowed.contains(&"success"));
    }

    #[test]
    fn finish_rejects_in_progress() {
        let allowed = ["success", "failed", "rolled_back"];
        assert!(!allowed.contains(&"in_progress"));
    }

    #[test]
    fn finish_rejects_unknown_status() {
        let allowed = ["success", "failed", "rolled_back"];
        assert!(!allowed.contains(&"garbage"));
    }

    #[test]
    fn create_response_serializes_deployment_id() {
        let id = Uuid::new_v4();
        let resp = CreateDeploymentResponse { deployment_id: id };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["deployment_id"].as_str().unwrap(), id.to_string());
    }
}
