use crate::AdminServiceAppState;
use crate::middleware::auth::TenantContext;
use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// API-key roles ('viewer'/'member'/'admin', from auth-service's api_keys-backed
/// validation) that may create or modify deployment markers and change events.
/// Mirrors ingest-gateway's TenantContext::can_ingest() -- these are
/// ingest-adjacent write routes moved here from ingest-gateway's platform
/// port, not generic admin-service CRUD, so they keep ingest's authorization
/// shape rather than admin-service's session-role `require_admin`
/// ('tenant_admin'), which checks a different role vocabulary entirely.
pub(crate) fn can_ingest(ctx: &TenantContext) -> bool {
    matches!(ctx.role.as_str(), "member" | "admin")
}

#[derive(Deserialize)]
pub struct ListDeploymentsParams {
    pub service_name: Option<String>,
    pub environment: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct DeploymentMarker {
    pub deployment_id: Uuid,
    pub tenant_id: Uuid,
    pub project_id: Option<Uuid>,
    pub service_name: String,
    pub environment: String,
    pub service_version: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub deployed_by: Option<String>,
    pub commit_sha: Option<String>,
    pub rollback_of: Option<Uuid>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct ListDeploymentsResponse {
    pub items: Vec<DeploymentMarker>,
}

/// GET /v1/deployments
pub async fn list_deployments(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<ListDeploymentsParams>,
) -> Result<Json<ListDeploymentsResponse>, StatusCode> {
    let limit = params.limit.unwrap_or(50).min(200);

    let items = sqlx::query_as::<_, DeploymentMarker>(
        "SELECT deployment_id, tenant_id, project_id, service_name, environment, \
         service_version, status, started_at, finished_at, deployed_by, \
         commit_sha, rollback_of, metadata \
         FROM deployment_markers \
         WHERE tenant_id = $1 \
           AND ($2::TEXT IS NULL OR service_name = $2) \
           AND ($3::TEXT IS NULL OR environment = $3) \
           AND ($4::TIMESTAMPTZ IS NULL OR started_at >= $4) \
           AND ($5::TIMESTAMPTZ IS NULL OR started_at <= $5) \
         ORDER BY started_at DESC \
         LIMIT $6",
    )
    .bind(ctx.tenant_id)
    .bind(&params.service_name)
    .bind(&params.environment)
    .bind(params.start_time)
    .bind(params.end_time)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to list deployment markers");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(ListDeploymentsResponse { items }))
}

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

/// POST /v1/deployments
pub async fn create_deployment(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<CreateDeploymentRequest>,
) -> Result<(StatusCode, Json<CreateDeploymentResponse>), StatusCode> {
    if !can_ingest(&ctx) {
        return Err(StatusCode::FORBIDDEN);
    }
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
    .fetch_one(&state.db)
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

/// PATCH /v1/deployments/{deployment_id}
pub async fn finish_deployment(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(deployment_id): Path<Uuid>,
    Json(req): Json<FinishDeploymentRequest>,
) -> Result<StatusCode, StatusCode> {
    if !can_ingest(&ctx) {
        return Err(StatusCode::FORBIDDEN);
    }
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
    .execute(&state.db)
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
    fn default_limit_is_50() {
        let params = ListDeploymentsParams {
            service_name: None,
            environment: None,
            start_time: None,
            end_time: None,
            limit: None,
        };
        assert_eq!(params.limit.unwrap_or(50).min(200), 50);
    }

    #[test]
    fn limit_is_capped_at_200() {
        let params = ListDeploymentsParams {
            service_name: Some("svc".into()),
            environment: None,
            start_time: None,
            end_time: None,
            limit: Some(999),
        };
        assert_eq!(params.limit.unwrap_or(50).min(200), 200);
    }

    #[test]
    fn marker_serializes_all_fields() {
        let id = Uuid::new_v4();
        let m = DeploymentMarker {
            deployment_id: id,
            tenant_id: Uuid::new_v4(),
            project_id: None,
            service_name: "shop-api".into(),
            environment: "staging".into(),
            service_version: "v1.2.0".into(),
            status: "success".into(),
            started_at: Utc::now(),
            finished_at: None,
            deployed_by: Some("ci-bot".into()),
            commit_sha: Some("abc123".into()),
            rollback_of: None,
            metadata: None,
        };
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["service_name"], "shop-api");
        assert_eq!(v["status"], "success");
        assert!(v["finished_at"].is_null());
    }

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

    #[test]
    fn member_can_ingest() {
        let ctx = TenantContext {
            tenant_id: Uuid::new_v4(),
            user_id: None,
            role: "member".into(),
        };
        assert!(can_ingest(&ctx));
    }

    #[test]
    fn admin_can_ingest() {
        let ctx = TenantContext {
            tenant_id: Uuid::new_v4(),
            user_id: None,
            role: "admin".into(),
        };
        assert!(can_ingest(&ctx));
    }

    #[test]
    fn viewer_cannot_ingest() {
        let ctx = TenantContext {
            tenant_id: Uuid::new_v4(),
            user_id: None,
            role: "viewer".into(),
        };
        assert!(!can_ingest(&ctx));
    }
}
