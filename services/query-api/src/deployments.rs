use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    Json,
    extract::{Extension, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

pub async fn list_deployments(
    State(state): State<AppState>,
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
}
