use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NotificationChannelType {
    Webhook,
}

impl From<String> for NotificationChannelType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "webhook" => NotificationChannelType::Webhook,
            _ => NotificationChannelType::Webhook, // Fallback
        }
    }
}

#[derive(Serialize, Debug, Clone, sqlx::FromRow)]
pub struct NotificationChannelItem {
    pub channel_id: Uuid,
    pub name: String,
    #[sqlx(rename = "type")]
    pub channel_type: String,
    pub config: serde_json::Value,
}

#[derive(Serialize, Debug, Clone)]
pub struct NotificationChannelResponse {
    pub channel_id: Uuid,
    pub name: String,
    pub channel_type: NotificationChannelType,
    pub config: serde_json::Value,
}

impl From<NotificationChannelItem> for NotificationChannelResponse {
    fn from(item: NotificationChannelItem) -> Self {
        Self {
            channel_id: item.channel_id,
            name: item.name,
            channel_type: item.channel_type.into(),
            config: item.config,
        }
    }
}

#[derive(Deserialize)]
pub struct CreateChannelRequest {
    pub name: String,
    pub channel_type: NotificationChannelType,
    pub config: serde_json::Value,
}

pub async fn list_notification_channels(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
) -> Result<Vec<NotificationChannelResponse>, sqlx::Error> {
    let rows = sqlx::query_as::<_, NotificationChannelItem>(
        "SELECT channel_id, name, type, config FROM notification_channels WHERE tenant_id = $1 ORDER BY created_at DESC",
    )
    .bind(tenant_id)
    .fetch_all(db)
    .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn create_notification_channel(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    req: &CreateChannelRequest,
) -> Result<NotificationChannelResponse, sqlx::Error> {
    let type_str = match req.channel_type {
        NotificationChannelType::Webhook => "webhook",
    };

    let item = sqlx::query_as::<_, NotificationChannelItem>(
        "INSERT INTO notification_channels (tenant_id, name, type, config) \
         VALUES ($1, $2, $3::notification_channel_type, $4) \
         RETURNING channel_id, name, type, config",
    )
    .bind(tenant_id)
    .bind(&req.name)
    .bind(type_str)
    .bind(&req.config)
    .fetch_one(db)
    .await?;

    Ok(item.into())
}

pub async fn delete_notification_channel(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    channel_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result =
        sqlx::query("DELETE FROM notification_channels WHERE channel_id = $1 AND tenant_id = $2")
            .bind(channel_id)
            .bind(tenant_id)
            .execute(db)
            .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn handle_list_channels(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<Vec<NotificationChannelResponse>>, StatusCode> {
    let items = list_notification_channels(&state.db, ctx.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to list notification channels");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(items))
}

pub async fn handle_create_channel(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<CreateChannelRequest>,
) -> Result<(StatusCode, Json<NotificationChannelResponse>), StatusCode> {
    match create_notification_channel(&state.db, ctx.tenant_id, &req).await {
        Ok(item) => Ok((StatusCode::CREATED, Json(item))),
        Err(e) => {
            tracing::error!(error = %e, "failed to create notification channel");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_delete_channel(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(channel_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    match delete_notification_channel(&state.db, ctx.tenant_id, channel_id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to delete notification channel");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
