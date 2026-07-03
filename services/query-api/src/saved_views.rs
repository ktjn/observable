use crate::dashboards::{grant_satisfies_delete, grant_satisfies_read, grant_satisfies_write};
use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const VALID_SIGNAL_KINDS: &[&str] = &["logs"];

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SavedViewItem {
    pub saved_view_id: Uuid,
    pub name: String,
    pub signal_kind: String,
    pub visibility: String,
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct SavedViewListResponse {
    pub items: Vec<SavedViewItem>,
}

#[derive(Deserialize)]
pub struct ListSavedViewsQuery {
    pub signal_kind: String,
}

#[derive(Deserialize)]
pub struct CreateSavedViewRequest {
    pub name: String,
    pub signal_kind: String,
    pub config: serde_json::Value,
}

#[derive(Deserialize)]
pub struct UpdateSavedViewRequest {
    pub name: String,
    pub config: serde_json::Value,
    #[serde(default)]
    pub visibility: Option<String>,
}

#[derive(Deserialize)]
pub struct AddGrantRequest {
    pub user_id: Uuid,
    pub relation: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct GrantItem {
    pub user_id: Uuid,
    pub relation: String,
    pub granted_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct GrantListResponse {
    pub grants: Vec<GrantItem>,
}

#[derive(Debug)]
pub enum SavedViewError {
    InvalidInput(String),
    Db(sqlx::Error),
}

impl std::fmt::Display for SavedViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SavedViewError::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            SavedViewError::Db(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for SavedViewError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SavedViewError::Db(e) => Some(e),
            SavedViewError::InvalidInput(_) => None,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SavedViewRow {
    saved_view_id: Uuid,
    name: String,
    signal_kind: String,
    visibility: String,
    config: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

fn row_to_item(row: SavedViewRow) -> SavedViewItem {
    SavedViewItem {
        saved_view_id: row.saved_view_id,
        name: row.name,
        signal_kind: row.signal_kind,
        visibility: row.visibility,
        config: row.config,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn validate_create_request(req: &CreateSavedViewRequest) -> Result<(), SavedViewError> {
    if req.name.trim().is_empty() {
        return Err(SavedViewError::InvalidInput("name is required".into()));
    }
    if !VALID_SIGNAL_KINDS.contains(&req.signal_kind.as_str()) {
        return Err(SavedViewError::InvalidInput(format!(
            "signal_kind must be one of: {}",
            VALID_SIGNAL_KINDS.join(", ")
        )));
    }
    if !req.config.is_object() {
        return Err(SavedViewError::InvalidInput(
            "config must be a JSON object".into(),
        ));
    }
    Ok(())
}

fn validate_update_request(req: &UpdateSavedViewRequest) -> Result<(), SavedViewError> {
    if req.name.trim().is_empty() {
        return Err(SavedViewError::InvalidInput("name is required".into()));
    }
    if !req.config.is_object() {
        return Err(SavedViewError::InvalidInput(
            "config must be a JSON object".into(),
        ));
    }
    if req
        .visibility
        .as_deref()
        .is_some_and(|v| !matches!(v, "public" | "private"))
    {
        return Err(SavedViewError::InvalidInput(
            "visibility must be 'public' or 'private'".into(),
        ));
    }
    Ok(())
}

/// Fetch the relation a specific user holds on a specific saved view, if any.
async fn fetch_relation(
    db: &sqlx::PgPool,
    user_id: Uuid,
    saved_view_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT relation FROM saved_view_grants \
         WHERE saved_view_id = $1 AND user_id = $2",
    )
    .bind(saved_view_id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}

pub async fn list_saved_views(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    user_id: Option<Uuid>,
    signal_kind: &str,
) -> Result<Vec<SavedViewItem>, sqlx::Error> {
    // Same visibility rule as dashboards::list_dashboards: API-key callers
    // (user_id = None) see every tenant row; session users see public rows
    // plus rows they hold any grant on.
    let rows = if let Some(uid) = user_id {
        sqlx::query_as::<_, SavedViewRow>(
            "SELECT saved_view_id, name, signal_kind, visibility, config, created_at, updated_at \
             FROM saved_views \
             WHERE tenant_id = $1 AND signal_kind = $2 \
               AND (visibility = 'public' \
                    OR EXISTS ( \
                        SELECT 1 FROM saved_view_grants \
                        WHERE saved_view_grants.saved_view_id = saved_views.saved_view_id \
                          AND user_id = $3 \
                    )) \
             ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .bind(signal_kind)
        .bind(uid)
        .fetch_all(db)
        .await?
    } else {
        sqlx::query_as::<_, SavedViewRow>(
            "SELECT saved_view_id, name, signal_kind, visibility, config, created_at, updated_at \
             FROM saved_views \
             WHERE tenant_id = $1 AND signal_kind = $2 \
             ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .bind(signal_kind)
        .fetch_all(db)
        .await?
    };
    Ok(rows.into_iter().map(row_to_item).collect())
}

pub async fn get_saved_view(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    saved_view_id: Uuid,
) -> Result<Option<SavedViewItem>, sqlx::Error> {
    let row = sqlx::query_as::<_, SavedViewRow>(
        "SELECT saved_view_id, name, signal_kind, visibility, config, created_at, updated_at \
         FROM saved_views \
         WHERE saved_view_id = $1 AND tenant_id = $2",
    )
    .bind(saved_view_id)
    .bind(tenant_id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(row_to_item))
}

pub async fn create_saved_view(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    req: &CreateSavedViewRequest,
    creator_user_id: Option<Uuid>,
) -> Result<SavedViewItem, SavedViewError> {
    validate_create_request(req)?;

    let mut tx = db.begin().await.map_err(SavedViewError::Db)?;
    let row = sqlx::query_as::<_, SavedViewRow>(
        "INSERT INTO saved_views (tenant_id, owner_user_id, name, signal_kind, config) \
         VALUES ($1, $2, $3, $4, $5) \
         RETURNING saved_view_id, name, signal_kind, visibility, config, created_at, updated_at",
    )
    .bind(tenant_id)
    .bind(creator_user_id)
    .bind(req.name.trim())
    .bind(&req.signal_kind)
    .bind(&req.config)
    .fetch_one(&mut *tx)
    .await
    .map_err(SavedViewError::Db)?;

    if let Some(user_id) = creator_user_id {
        sqlx::query(
            "INSERT INTO saved_view_grants (saved_view_id, user_id, relation) \
             VALUES ($1, $2, 'owner') \
             ON CONFLICT (saved_view_id, user_id) DO UPDATE SET relation = 'owner'",
        )
        .bind(row.saved_view_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(SavedViewError::Db)?;
    }

    tx.commit().await.map_err(SavedViewError::Db)?;
    Ok(row_to_item(row))
}

pub async fn update_saved_view(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    saved_view_id: Uuid,
    req: &UpdateSavedViewRequest,
) -> Result<Option<SavedViewItem>, SavedViewError> {
    validate_update_request(req)?;

    let row = if let Some(vis) = req.visibility.as_deref() {
        sqlx::query_as::<_, SavedViewRow>(
            "UPDATE saved_views SET name = $1, config = $2, visibility = $3, updated_at = NOW() \
             WHERE saved_view_id = $4 AND tenant_id = $5 \
             RETURNING saved_view_id, name, signal_kind, visibility, config, created_at, updated_at",
        )
        .bind(req.name.trim())
        .bind(&req.config)
        .bind(vis)
        .bind(saved_view_id)
        .bind(tenant_id)
        .fetch_optional(db)
        .await
        .map_err(SavedViewError::Db)?
    } else {
        sqlx::query_as::<_, SavedViewRow>(
            "UPDATE saved_views SET name = $1, config = $2, updated_at = NOW() \
             WHERE saved_view_id = $3 AND tenant_id = $4 \
             RETURNING saved_view_id, name, signal_kind, visibility, config, created_at, updated_at",
        )
        .bind(req.name.trim())
        .bind(&req.config)
        .bind(saved_view_id)
        .bind(tenant_id)
        .fetch_optional(db)
        .await
        .map_err(SavedViewError::Db)?
    };

    Ok(row.map(row_to_item))
}

pub async fn delete_saved_view(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    saved_view_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM saved_views WHERE saved_view_id = $1 AND tenant_id = $2")
        .bind(saved_view_id)
        .bind(tenant_id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_create_request_rejects_blank_name() {
        let req = CreateSavedViewRequest {
            name: "   ".into(),
            signal_kind: "logs".into(),
            config: serde_json::json!({}),
        };
        assert!(validate_create_request(&req).is_err());
    }

    #[test]
    fn validate_create_request_rejects_invalid_signal_kind() {
        let req = CreateSavedViewRequest {
            name: "My view".into(),
            signal_kind: "traces".into(),
            config: serde_json::json!({}),
        };
        assert!(validate_create_request(&req).is_err());
    }

    #[test]
    fn validate_create_request_rejects_non_object_config() {
        let req = CreateSavedViewRequest {
            name: "My view".into(),
            signal_kind: "logs".into(),
            config: serde_json::json!("not-an-object"),
        };
        assert!(validate_create_request(&req).is_err());
    }

    #[test]
    fn validate_create_request_accepts_valid_input() {
        let req = CreateSavedViewRequest {
            name: "My view".into(),
            signal_kind: "logs".into(),
            config: serde_json::json!({"severity_filter": "error"}),
        };
        assert!(validate_create_request(&req).is_ok());
    }

    #[test]
    fn validate_update_request_rejects_invalid_visibility() {
        let req = UpdateSavedViewRequest {
            name: "My view".into(),
            config: serde_json::json!({}),
            visibility: Some("everyone".into()),
        };
        assert!(validate_update_request(&req).is_err());
    }

    #[test]
    fn validate_update_request_accepts_valid_visibility() {
        let req = UpdateSavedViewRequest {
            name: "My view".into(),
            config: serde_json::json!({}),
            visibility: Some("public".into()),
        };
        assert!(validate_update_request(&req).is_ok());
    }
}
