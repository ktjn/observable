use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    Json,
    extract::{Extension, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ListChangeEventsParams {
    pub service_name: Option<String>,
    pub environment: Option<String>,
    pub event_type: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ChangeEvent {
    pub change_event_id: Uuid,
    pub tenant_id: Uuid,
    pub project_id: Option<Uuid>,
    pub event_type: String,
    pub service_name: Option<String>,
    pub environment: String,
    pub title: String,
    pub description: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub source: Option<String>,
    pub created_by: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct ListChangeEventsResponse {
    pub items: Vec<ChangeEvent>,
}

pub async fn list_change_events(
    pool: &PgPool,
    tenant_id: Uuid,
    params: ListChangeEventsParams,
) -> Result<Vec<ChangeEvent>, sqlx::Error> {
    let limit = params.limit.unwrap_or(50).min(200);

    sqlx::query_as::<_, ChangeEvent>(
        "SELECT change_event_id, tenant_id, project_id, event_type, service_name, \
         environment, title, description, occurred_at, source, created_by, metadata \
         FROM change_events \
         WHERE tenant_id = $1 \
           AND ($2::TEXT IS NULL OR service_name = $2) \
           AND ($3::TEXT IS NULL OR environment = $3) \
           AND ($4::TEXT IS NULL OR event_type = $4) \
           AND ($5::TIMESTAMPTZ IS NULL OR occurred_at >= $5) \
           AND ($6::TIMESTAMPTZ IS NULL OR occurred_at <= $6) \
         ORDER BY occurred_at DESC \
         LIMIT $7",
    )
    .bind(tenant_id)
    .bind(&params.service_name)
    .bind(&params.environment)
    .bind(&params.event_type)
    .bind(params.start_time)
    .bind(params.end_time)
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn handle_list_change_events(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<ListChangeEventsParams>,
) -> Result<Json<ListChangeEventsResponse>, StatusCode> {
    let items = list_change_events(&state.db, ctx.tenant_id, params)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to list change events");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(ListChangeEventsResponse { items }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limit_is_50() {
        let params = ListChangeEventsParams {
            service_name: None,
            environment: None,
            event_type: None,
            start_time: None,
            end_time: None,
            limit: None,
        };
        assert_eq!(params.limit.unwrap_or(50).min(200), 50);
    }

    #[test]
    fn limit_is_capped_at_200() {
        let params = ListChangeEventsParams {
            service_name: None,
            environment: None,
            event_type: None,
            start_time: None,
            end_time: None,
            limit: Some(999),
        };
        assert_eq!(params.limit.unwrap_or(50).min(200), 200);
    }

    #[test]
    fn event_serializes_all_fields() {
        let id = Uuid::new_v4();
        let e = ChangeEvent {
            change_event_id: id,
            tenant_id: Uuid::new_v4(),
            project_id: None,
            event_type: "feature_flag".into(),
            service_name: Some("checkout".into()),
            environment: "production".into(),
            title: "Enabled new-checkout-flow".into(),
            description: None,
            occurred_at: Utc::now(),
            source: Some("launchdarkly".into()),
            created_by: None,
            metadata: None,
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["event_type"], "feature_flag");
        assert_eq!(v["service_name"], "checkout");
        assert!(v["description"].is_null());
    }
}
