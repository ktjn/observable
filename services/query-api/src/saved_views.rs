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

pub async fn handle_list_saved_views(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<ListSavedViewsQuery>,
) -> Result<Json<SavedViewListResponse>, StatusCode> {
    if !VALID_SIGNAL_KINDS.contains(&params.signal_kind.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let items = list_saved_views(&state.db, ctx.tenant_id, ctx.user_id, &params.signal_kind)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to list saved views");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(SavedViewListResponse { items }))
}

pub async fn handle_create_saved_view(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<CreateSavedViewRequest>,
) -> Result<(StatusCode, Json<SavedViewItem>), StatusCode> {
    match create_saved_view(&state.db, ctx.tenant_id, &req, ctx.user_id).await {
        Ok(item) => Ok((StatusCode::CREATED, Json(item))),
        Err(SavedViewError::InvalidInput(msg)) => {
            tracing::warn!(message = %msg, "invalid saved view input");
            Err(StatusCode::BAD_REQUEST)
        }
        Err(SavedViewError::Db(e)) => {
            tracing::error!(error = %e, "failed to create saved view");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_get_saved_view(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(saved_view_id): Path<Uuid>,
) -> Result<Json<SavedViewItem>, StatusCode> {
    let item = match get_saved_view(&state.db, ctx.tenant_id, saved_view_id).await {
        Ok(Some(item)) => item,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to get saved view");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    if let Some(user_id) = ctx.user_id {
        let relation = fetch_relation(&state.db, user_id, saved_view_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to fetch grant");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !grant_satisfies_read(&item.visibility, relation.as_deref()) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(Json(item))
}

async fn saved_view_exists(
    db: &sqlx::PgPool,
    saved_view_id: Uuid,
    tenant_id: Uuid,
) -> Result<bool, StatusCode> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM saved_views WHERE saved_view_id = $1 AND tenant_id = $2)",
    )
    .bind(saved_view_id)
    .bind(tenant_id)
    .fetch_one(db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to check saved view existence");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub async fn handle_update_saved_view(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(saved_view_id): Path<Uuid>,
    Json(req): Json<UpdateSavedViewRequest>,
) -> Result<Json<SavedViewItem>, StatusCode> {
    if !saved_view_exists(&state.db, saved_view_id, ctx.tenant_id).await? {
        return Err(StatusCode::NOT_FOUND);
    }
    if let Some(user_id) = ctx.user_id {
        let relation = fetch_relation(&state.db, user_id, saved_view_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to fetch grant");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !grant_satisfies_write(&ctx.role, relation.as_deref()) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    match update_saved_view(&state.db, ctx.tenant_id, saved_view_id, &req).await {
        Ok(Some(item)) => Ok(Json(item)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(SavedViewError::InvalidInput(msg)) => {
            tracing::warn!(message = %msg, "invalid saved view update");
            Err(StatusCode::BAD_REQUEST)
        }
        Err(SavedViewError::Db(e)) => {
            tracing::error!(error = %e, "failed to update saved view");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_delete_saved_view(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(saved_view_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    if !saved_view_exists(&state.db, saved_view_id, ctx.tenant_id).await? {
        return Err(StatusCode::NOT_FOUND);
    }
    if let Some(user_id) = ctx.user_id {
        let relation = fetch_relation(&state.db, user_id, saved_view_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to fetch grant");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !grant_satisfies_delete(&ctx.role, relation.as_deref()) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    match delete_saved_view(&state.db, ctx.tenant_id, saved_view_id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to delete saved view");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_list_saved_view_grants(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(saved_view_id): Path<Uuid>,
) -> Result<Json<GrantListResponse>, StatusCode> {
    let user_id = ctx.user_id.ok_or(StatusCode::FORBIDDEN)?;
    if !saved_view_exists(&state.db, saved_view_id, ctx.tenant_id).await? {
        return Err(StatusCode::NOT_FOUND);
    }
    let relation = fetch_relation(&state.db, user_id, saved_view_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to fetch grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let is_admin = ctx.role == "tenant_admin";
    let is_owner = relation.as_deref() == Some("owner");
    if !is_admin && !is_owner {
        return Err(StatusCode::FORBIDDEN);
    }
    let grants = sqlx::query_as::<_, GrantItem>(
        "SELECT user_id, relation, granted_at \
         FROM saved_view_grants \
         WHERE saved_view_id = $1 \
         ORDER BY granted_at ASC",
    )
    .bind(saved_view_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to list grants");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(GrantListResponse { grants }))
}

pub async fn handle_add_saved_view_grant(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(saved_view_id): Path<Uuid>,
    Json(req): Json<AddGrantRequest>,
) -> Result<StatusCode, StatusCode> {
    if !matches!(req.relation.as_str(), "owner" | "editor" | "viewer") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let user_id = ctx.user_id.ok_or(StatusCode::FORBIDDEN)?;
    if !saved_view_exists(&state.db, saved_view_id, ctx.tenant_id).await? {
        return Err(StatusCode::NOT_FOUND);
    }
    let caller_relation = fetch_relation(&state.db, user_id, saved_view_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to fetch grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let is_admin = ctx.role == "tenant_admin";
    if !is_admin && caller_relation.as_deref() != Some("owner") {
        return Err(StatusCode::FORBIDDEN);
    }
    let target_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM user_tenant_roles WHERE user_id = $1 AND tenant_id = $2)",
    )
    .bind(req.user_id)
    .bind(ctx.tenant_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to verify target user");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !target_exists {
        return Err(StatusCode::NOT_FOUND);
    }
    sqlx::query(
        "INSERT INTO saved_view_grants (saved_view_id, user_id, relation) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (saved_view_id, user_id) DO UPDATE SET relation = EXCLUDED.relation",
    )
    .bind(saved_view_id)
    .bind(req.user_id)
    .bind(&req.relation)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to insert grant");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn handle_revoke_saved_view_grant(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path((saved_view_id, target_user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let user_id = ctx.user_id.ok_or(StatusCode::FORBIDDEN)?;
    if !saved_view_exists(&state.db, saved_view_id, ctx.tenant_id).await? {
        return Err(StatusCode::NOT_FOUND);
    }
    let caller_relation = fetch_relation(&state.db, user_id, saved_view_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to fetch caller grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let is_admin = ctx.role == "tenant_admin";
    let is_owner = caller_relation.as_deref() == Some("owner");
    if !is_admin && !is_owner {
        return Err(StatusCode::FORBIDDEN);
    }
    let target_relation = fetch_relation(&state.db, target_user_id, saved_view_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to fetch target grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    // Atomic guard against deleting the last owner — see dashboards::handle_revoke_grant
    // for the identical TOCTOU-race rationale.
    let result = if target_relation.as_deref() == Some("owner") {
        sqlx::query(
            "WITH guard AS ( \
               SELECT COUNT(*) AS remaining \
               FROM saved_view_grants \
               WHERE saved_view_id = $1 AND relation = 'owner' AND user_id != $2 \
             ) \
             DELETE FROM saved_view_grants \
             WHERE saved_view_id = $1 AND user_id = $2 \
               AND (SELECT remaining FROM guard) > 0",
        )
        .bind(saved_view_id)
        .bind(target_user_id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to delete grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    } else {
        sqlx::query("DELETE FROM saved_view_grants WHERE saved_view_id = $1 AND user_id = $2")
            .bind(saved_view_id)
            .bind(target_user_id)
            .execute(&state.db)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to delete grant");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
    };
    if result.rows_affected() == 0 {
        if target_relation.as_deref() == Some("owner") {
            return Err(StatusCode::CONFLICT);
        }
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
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
