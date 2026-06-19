use crate::AppState;
use crate::auth::TenantContext;
use axum::{
    extract::{Extension, Json, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const ALLOWED_EVENT_TYPES: &[&str] = &[
    "config_change",
    "feature_flag",
    "migration",
    "incident",
    "other",
];

#[derive(Deserialize)]
pub struct CreateChangeEventRequest {
    pub event_type: String,
    pub environment: String,
    pub title: String,
    pub service_name: Option<String>,
    pub description: Option<String>,
    pub occurred_at: Option<DateTime<Utc>>,
    pub source: Option<String>,
    pub created_by: Option<String>,
    pub project_id: Option<Uuid>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct CreateChangeEventResponse {
    pub change_event_id: Uuid,
}

pub async fn create_change_event(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<CreateChangeEventRequest>,
) -> Result<(StatusCode, Json<CreateChangeEventResponse>), StatusCode> {
    if !ALLOWED_EVENT_TYPES.contains(&req.event_type.as_str()) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    if req.environment.trim().is_empty() || req.title.trim().is_empty() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    let occurred_at = req.occurred_at.unwrap_or_else(Utc::now);

    let change_event_id: Uuid = sqlx::query_scalar(
        "INSERT INTO change_events \
         (tenant_id, project_id, event_type, service_name, environment, title, \
          description, occurred_at, source, created_by, metadata) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
         RETURNING change_event_id",
    )
    .bind(ctx.tenant_id)
    .bind(req.project_id)
    .bind(&req.event_type)
    .bind(&req.service_name)
    .bind(&req.environment)
    .bind(&req.title)
    .bind(&req.description)
    .bind(occurred_at)
    .bind(&req.source)
    .bind(&req.created_by)
    .bind(&req.metadata)
    .fetch_one(state.db.as_ref())
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to create change event");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::CREATED,
        Json(CreateChangeEventResponse { change_event_id }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_known_event_types() {
        for t in ALLOWED_EVENT_TYPES {
            assert!(ALLOWED_EVENT_TYPES.contains(t));
        }
    }

    #[test]
    fn rejects_unknown_event_type() {
        assert!(!ALLOWED_EVENT_TYPES.contains(&"deploy"));
        assert!(!ALLOWED_EVENT_TYPES.contains(&"garbage"));
    }

    #[test]
    fn create_response_serializes_change_event_id() {
        let id = Uuid::new_v4();
        let resp = CreateChangeEventResponse {
            change_event_id: id,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["change_event_id"].as_str().unwrap(), id.to_string());
    }
}
