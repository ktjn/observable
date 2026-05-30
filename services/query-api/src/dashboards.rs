use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const VALID_QUERY_KINDS: &[&str] = &["logs", "traces", "metrics"];
const VALID_PANEL_KINDS: &[&str] = &["query", "text"];
const EXPORT_SCHEMA_VERSION: &str = "2";

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct DashboardPanelItem {
    pub panel_id: Uuid,
    pub title: String,
    pub panel_kind: String,
    pub query_kind: Option<String>,
    pub service: Option<String>,
    pub preset: Option<String>,
    pub filters: serde_json::Value,
    pub query_text: Option<String>,
    pub content: Option<String>,
    pub layout: serde_json::Value,
    pub time_range: serde_json::Value,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct DashboardItem {
    pub dashboard_id: Uuid,
    pub name: String,
    pub visibility: String,
    pub panels: Vec<DashboardPanelItem>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct DashboardListResponse {
    pub items: Vec<DashboardItem>,
}

#[derive(Deserialize, Clone)]
pub struct DashboardPanelRequest {
    #[serde(default)]
    pub panel_id: Option<Uuid>,
    pub title: String,
    #[serde(default)]
    pub panel_kind: Option<String>,
    #[serde(default)]
    pub query_kind: Option<String>,
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub preset: Option<String>,
    #[serde(default = "empty_object")]
    pub filters: serde_json::Value,
    #[serde(default)]
    pub query_text: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub layout: Option<serde_json::Value>,
    #[serde(default)]
    pub time_range: Option<serde_json::Value>,
}

impl Default for DashboardPanelRequest {
    fn default() -> Self {
        Self {
            panel_id: None,
            title: String::new(),
            panel_kind: None,
            query_kind: None,
            service: None,
            preset: None,
            filters: serde_json::json!({}),
            query_text: None,
            content: None,
            layout: None,
            time_range: None,
        }
    }
}

#[derive(Deserialize)]
pub struct CreateDashboardRequest {
    pub name: String,
    pub panels: Vec<DashboardPanelRequest>,
}

#[derive(Deserialize)]
pub struct UpdateDashboardRequest {
    pub name: String,
    pub panels: Vec<DashboardPanelRequest>,
    #[serde(default)]
    pub visibility: Option<String>,
}

#[derive(Debug)]
pub enum CreateDashboardError {
    InvalidInput(String),
    Db(sqlx::Error),
}

impl std::fmt::Display for CreateDashboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CreateDashboardError::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            CreateDashboardError::Db(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for CreateDashboardError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CreateDashboardError::Db(e) => Some(e),
            CreateDashboardError::InvalidInput(_) => None,
        }
    }
}

#[derive(sqlx::FromRow)]
struct DashboardRow {
    dashboard_id: Uuid,
    name: String,
    visibility: String,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct DashboardPanelRow {
    dashboard_id: Uuid,
    panel_id: Uuid,
    title: String,
    panel_kind: String,
    query_kind: Option<String>,
    service: Option<String>,
    preset: Option<String>,
    filters: serde_json::Value,
    query_text: Option<String>,
    content: Option<String>,
    layout: serde_json::Value,
    time_range: serde_json::Value,
}

/// True if the caller is allowed to read this dashboard.
/// Public dashboards are readable by any tenant member (RBAC already enforced by middleware).
/// Private dashboards require an explicit grant of any relation.
pub(crate) fn grant_satisfies_read(visibility: &str, relation: Option<&str>) -> bool {
    visibility == "public" || relation.is_some_and(|r| matches!(r, "owner" | "editor" | "viewer"))
}

/// True if the caller is allowed to write (update) this dashboard.
/// `tenant_admin` bypasses tuple checks.
pub(crate) fn grant_satisfies_write(tenant_role: &str, relation: Option<&str>) -> bool {
    tenant_role == "tenant_admin" || relation.is_some_and(|r| matches!(r, "owner" | "editor"))
}

/// True if the caller is allowed to delete this dashboard.
/// `tenant_admin` bypasses tuple checks.
pub(crate) fn grant_satisfies_delete(tenant_role: &str, relation: Option<&str>) -> bool {
    tenant_role == "tenant_admin" || relation.is_some_and(|r| r == "owner")
}

/// Fetch the relation a specific user holds on a specific dashboard, if any.
async fn fetch_relation(
    db: &sqlx::PgPool,
    user_id: uuid::Uuid,
    dashboard_id: uuid::Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT relation FROM dashboard_grants \
         WHERE dashboard_id = $1 AND user_id = $2",
    )
    .bind(dashboard_id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}

pub async fn list_dashboards(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    user_id: Option<uuid::Uuid>,
) -> Result<Vec<DashboardItem>, sqlx::Error> {
    // When user_id is None (API-key callers), all tenant dashboards are returned —
    // API keys carry tenant-level access and bypass ReBAC visibility filters.
    // When user_id is Some (session users), only public dashboards and dashboards
    // the user has an explicit grant for are returned.
    let dashboards = if let Some(uid) = user_id {
        sqlx::query_as::<_, DashboardRow>(
            "SELECT dashboard_id, name, visibility, created_at \
             FROM dashboards \
             WHERE tenant_id = $1 \
               AND (visibility = 'public' \
                    OR EXISTS ( \
                        SELECT 1 FROM dashboard_grants \
                        WHERE dashboard_grants.dashboard_id = dashboards.dashboard_id \
                          AND user_id = $2 \
                    )) \
             ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .bind(uid)
        .fetch_all(db)
        .await?
    } else {
        sqlx::query_as::<_, DashboardRow>(
            "SELECT dashboard_id, name, visibility, created_at \
             FROM dashboards \
             WHERE tenant_id = $1 \
             ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(db)
        .await?
    };

    let dashboard_ids: Vec<uuid::Uuid> = dashboards.iter().map(|d| d.dashboard_id).collect();
    let panels = if dashboard_ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as::<_, DashboardPanelRow>(
            "SELECT dashboard_id, panel_id, title, panel_kind, query_kind, service, preset, \
                    filters, query_text, content, layout, time_range \
             FROM dashboard_panels \
             WHERE dashboard_id = ANY($1) \
             ORDER BY dashboard_id, position ASC",
        )
        .bind(&dashboard_ids)
        .fetch_all(db)
        .await?
    };

    Ok(dashboards
        .into_iter()
        .map(|dashboard| DashboardItem {
            panels: panels
                .iter()
                .filter(|panel| panel.dashboard_id == dashboard.dashboard_id)
                .map(|panel| DashboardPanelItem {
                    panel_id: panel.panel_id,
                    title: panel.title.clone(),
                    panel_kind: panel.panel_kind.clone(),
                    query_kind: panel.query_kind.clone(),
                    service: panel.service.clone(),
                    preset: panel.preset.clone(),
                    filters: panel.filters.clone(),
                    query_text: panel.query_text.clone(),
                    content: panel.content.clone(),
                    layout: panel.layout.clone(),
                    time_range: panel.time_range.clone(),
                })
                .collect(),
            dashboard_id: dashboard.dashboard_id,
            name: dashboard.name,
            visibility: dashboard.visibility,
            created_at: dashboard.created_at,
        })
        .collect())
}

pub async fn get_dashboard(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    dashboard_id: Uuid,
) -> Result<Option<DashboardItem>, sqlx::Error> {
    let dashboard = sqlx::query_as::<_, DashboardRow>(
        "SELECT dashboard_id, name, visibility, created_at \
         FROM dashboards \
         WHERE dashboard_id = $1 AND tenant_id = $2",
    )
    .bind(dashboard_id)
    .bind(tenant_id)
    .fetch_optional(db)
    .await?;

    let Some(dashboard) = dashboard else {
        return Ok(None);
    };

    let panels = sqlx::query_as::<_, DashboardPanelRow>(
        "SELECT dashboard_id, panel_id, title, panel_kind, query_kind, service, preset, \
                filters, query_text, content, layout, time_range \
         FROM dashboard_panels \
         WHERE dashboard_id = $1 \
         ORDER BY position ASC",
    )
    .bind(dashboard.dashboard_id)
    .fetch_all(db)
    .await?;

    Ok(Some(DashboardItem {
        dashboard_id: dashboard.dashboard_id,
        name: dashboard.name,
        visibility: dashboard.visibility,
        panels: panels.into_iter().map(row_to_panel_item).collect(),
        created_at: dashboard.created_at,
    }))
}

pub async fn create_dashboard(
    db: &sqlx::PgPool,
    tenant_id: uuid::Uuid,
    req: &CreateDashboardRequest,
    creator_user_id: Option<uuid::Uuid>,
) -> Result<DashboardItem, CreateDashboardError> {
    validate_create_request(req)?;

    let mut tx = db.begin().await.map_err(CreateDashboardError::Db)?;
    let row = sqlx::query_as::<_, DashboardRow>(
        "INSERT INTO dashboards (tenant_id, name) VALUES ($1, $2) \
         RETURNING dashboard_id, name, visibility, created_at",
    )
    .bind(tenant_id)
    .bind(req.name.trim())
    .fetch_one(&mut *tx)
    .await
    .map_err(CreateDashboardError::Db)?;

    if let Some(user_id) = creator_user_id {
        sqlx::query(
            "INSERT INTO dashboard_grants (dashboard_id, user_id, relation) \
             VALUES ($1, $2, 'owner') \
             ON CONFLICT (dashboard_id, user_id) DO UPDATE SET relation = 'owner'",
        )
        .bind(row.dashboard_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(CreateDashboardError::Db)?;
    }

    let mut panels = Vec::with_capacity(req.panels.len());
    for (position, panel) in req.panels.iter().enumerate() {
        let panel_kind = panel_kind(panel);
        let layout = normalized_layout(panel.layout.as_ref(), position);
        let time_range = normalized_time_range(panel);
        let item = sqlx::query_as::<_, DashboardPanelRow>(
            "INSERT INTO dashboard_panels \
             (dashboard_id, title, panel_kind, query_kind, service, preset, filters, \
              query_text, content, layout, time_range, position) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
             RETURNING dashboard_id, panel_id, title, panel_kind, query_kind, service, preset, \
                       filters, query_text, content, layout, time_range",
        )
        .bind(row.dashboard_id)
        .bind(panel.title.trim())
        .bind(panel_kind)
        .bind(panel.query_kind.as_deref())
        .bind(
            panel
                .service
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty()),
        )
        .bind(panel.preset.as_deref())
        .bind(&panel.filters)
        .bind(
            panel
                .query_text
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty()),
        )
        .bind(
            panel
                .content
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty()),
        )
        .bind(&layout)
        .bind(&time_range)
        .bind(position as i32)
        .fetch_one(&mut *tx)
        .await
        .map_err(CreateDashboardError::Db)?;

        panels.push(row_to_panel_item(item));
    }

    tx.commit().await.map_err(CreateDashboardError::Db)?;

    Ok(DashboardItem {
        dashboard_id: row.dashboard_id,
        name: row.name,
        visibility: row.visibility,
        panels,
        created_at: row.created_at,
    })
}

pub async fn update_dashboard(
    db: &sqlx::PgPool,
    tenant_id: uuid::Uuid,
    dashboard_id: uuid::Uuid,
    req: &UpdateDashboardRequest,
) -> Result<Option<DashboardItem>, CreateDashboardError> {
    validate_update_request(req)?;

    let mut tx = db.begin().await.map_err(CreateDashboardError::Db)?;

    let row = if let Some(vis) = req.visibility.as_deref() {
        sqlx::query_as::<_, DashboardRow>(
            "UPDATE dashboards SET name = $1, visibility = $2 \
             WHERE dashboard_id = $3 AND tenant_id = $4 \
             RETURNING dashboard_id, name, visibility, created_at",
        )
        .bind(req.name.trim())
        .bind(vis)
        .bind(dashboard_id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(CreateDashboardError::Db)?
    } else {
        sqlx::query_as::<_, DashboardRow>(
            "UPDATE dashboards SET name = $1 \
             WHERE dashboard_id = $2 AND tenant_id = $3 \
             RETURNING dashboard_id, name, visibility, created_at",
        )
        .bind(req.name.trim())
        .bind(dashboard_id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(CreateDashboardError::Db)?
    };

    let Some(row) = row else {
        tx.rollback().await.map_err(CreateDashboardError::Db)?;
        return Ok(None);
    };

    sqlx::query("DELETE FROM dashboard_panels WHERE dashboard_id = $1")
        .bind(row.dashboard_id)
        .execute(&mut *tx)
        .await
        .map_err(CreateDashboardError::Db)?;

    let mut panels = Vec::with_capacity(req.panels.len());
    for (position, panel) in req.panels.iter().enumerate() {
        let panel_kind = panel_kind(panel);
        let layout = normalized_layout(panel.layout.as_ref(), position);
        let time_range = normalized_time_range(panel);
        let panel_id = panel.panel_id.unwrap_or_else(Uuid::new_v4);
        let item = sqlx::query_as::<_, DashboardPanelRow>(
            "INSERT INTO dashboard_panels \
             (panel_id, dashboard_id, title, panel_kind, query_kind, service, preset, filters, \
              query_text, content, layout, time_range, position) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) \
             RETURNING dashboard_id, panel_id, title, panel_kind, query_kind, service, preset, \
                       filters, query_text, content, layout, time_range",
        )
        .bind(panel_id)
        .bind(row.dashboard_id)
        .bind(panel.title.trim())
        .bind(panel_kind)
        .bind(panel.query_kind.as_deref())
        .bind(
            panel
                .service
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty()),
        )
        .bind(panel.preset.as_deref())
        .bind(&panel.filters)
        .bind(
            panel
                .query_text
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty()),
        )
        .bind(
            panel
                .content
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty()),
        )
        .bind(&layout)
        .bind(&time_range)
        .bind(position as i32)
        .fetch_one(&mut *tx)
        .await
        .map_err(CreateDashboardError::Db)?;
        panels.push(row_to_panel_item(item));
    }

    tx.commit().await.map_err(CreateDashboardError::Db)?;

    Ok(Some(DashboardItem {
        dashboard_id: row.dashboard_id,
        name: row.name,
        visibility: row.visibility,
        panels,
        created_at: row.created_at,
    }))
}

pub async fn delete_dashboard(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    dashboard_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM dashboards WHERE dashboard_id = $1 AND tenant_id = $2")
        .bind(dashboard_id)
        .bind(tenant_id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() > 0)
}

const VALID_PRESETS: &[&str] = &["5m", "15m", "30m", "1h", "3h", "12h"];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DashboardExportPanel {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub panel_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    pub filters: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_range: Option<serde_json::Value>,
}

/// Portable dashboard representation — no IDs, stable for version control.
/// Access-control fields (`visibility`, grants) are intentionally excluded from exports.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DashboardExport {
    pub schema_version: String,
    pub name: String,
    pub panels: Vec<DashboardExportPanel>,
}

pub async fn export_dashboard(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    dashboard_id: Uuid,
) -> Result<Option<DashboardExport>, sqlx::Error> {
    let dashboard = sqlx::query_as::<_, DashboardRow>(
        "SELECT dashboard_id, name, visibility, created_at \
         FROM dashboards \
         WHERE dashboard_id = $1 AND tenant_id = $2",
    )
    .bind(dashboard_id)
    .bind(tenant_id)
    .fetch_optional(db)
    .await?;

    let Some(dashboard) = dashboard else {
        return Ok(None);
    };

    let panels = sqlx::query_as::<_, DashboardPanelRow>(
        "SELECT dashboard_id, panel_id, title, panel_kind, query_kind, service, preset, \
                filters, query_text, content, layout, time_range \
         FROM dashboard_panels \
         WHERE dashboard_id = $1 \
         ORDER BY position ASC",
    )
    .bind(dashboard.dashboard_id)
    .fetch_all(db)
    .await?;

    Ok(Some(DashboardExport {
        schema_version: EXPORT_SCHEMA_VERSION.into(),
        name: dashboard.name,
        panels: panels
            .into_iter()
            .map(|p| DashboardExportPanel {
                title: p.title,
                panel_kind: Some(p.panel_kind),
                query_kind: p.query_kind,
                service: p.service,
                preset: p.preset,
                filters: p.filters,
                query_text: p.query_text,
                content: p.content,
                layout: Some(p.layout),
                time_range: Some(p.time_range),
            })
            .collect(),
    }))
}

pub async fn import_dashboard(
    db: &sqlx::PgPool,
    tenant_id: uuid::Uuid,
    export: &DashboardExport,
    creator_user_id: Option<uuid::Uuid>,
) -> Result<DashboardItem, CreateDashboardError> {
    let req = CreateDashboardRequest {
        name: export.name.clone(),
        panels: export
            .panels
            .iter()
            .map(|p| DashboardPanelRequest {
                panel_id: None,
                title: p.title.clone(),
                panel_kind: p.panel_kind.clone(),
                query_kind: p.query_kind.clone(),
                service: p.service.clone(),
                preset: p.preset.clone(),
                filters: p.filters.clone(),
                query_text: p.query_text.clone(),
                content: p.content.clone(),
                layout: p.layout.clone(),
                time_range: p.time_range.clone(),
            })
            .collect(),
    };
    create_dashboard(db, tenant_id, &req, creator_user_id).await
}

fn validate_create_request(req: &CreateDashboardRequest) -> Result<(), CreateDashboardError> {
    if req.name.trim().is_empty() {
        return Err(CreateDashboardError::InvalidInput(
            "name is required".into(),
        ));
    }
    if req.panels.is_empty() {
        return Err(CreateDashboardError::InvalidInput(
            "at least one panel is required".into(),
        ));
    }
    for panel in &req.panels {
        let kind = panel_kind(panel);
        if panel.title.trim().is_empty() {
            return Err(CreateDashboardError::InvalidInput(
                "panel title is required".into(),
            ));
        }
        if !VALID_PANEL_KINDS.contains(&kind) {
            return Err(CreateDashboardError::InvalidInput(format!(
                "panel_kind must be one of: {}",
                VALID_PANEL_KINDS.join(", ")
            )));
        }
        if kind == "query" && panel.query_kind.as_deref().is_none() {
            return Err(CreateDashboardError::InvalidInput(
                "query panels require query_kind".into(),
            ));
        }
        if kind == "text" && panel.content.as_ref().is_none_or(|s| s.trim().is_empty()) {
            return Err(CreateDashboardError::InvalidInput(
                "text panels require content".into(),
            ));
        }
        if let Some(query_kind) = &panel.query_kind
            && !VALID_QUERY_KINDS.contains(&query_kind.as_str())
        {
            return Err(CreateDashboardError::InvalidInput(format!(
                "query_kind must be one of: {}",
                VALID_QUERY_KINDS.join(", ")
            )));
        }
        if let Some(layout) = &panel.layout {
            validate_layout(layout)?;
        }
        if let Some(time_range) = &panel.time_range {
            validate_time_range(time_range)?;
        }
        if kind == "query"
            && panel.time_range.is_none()
            && panel
                .preset
                .as_ref()
                .is_some_and(|preset| !VALID_PRESETS.contains(&preset.as_str()))
        {
            return Err(CreateDashboardError::InvalidInput(format!(
                "preset must be one of: {} (or omitted for global date range)",
                VALID_PRESETS.join(", ")
            )));
        }
        if let Some(ref preset) = panel.preset
            && !VALID_PRESETS.contains(&preset.as_str())
        {
            return Err(CreateDashboardError::InvalidInput(format!(
                "preset must be one of: {} (or omitted for global date range)",
                VALID_PRESETS.join(", ")
            )));
        }
    }
    Ok(())
}

fn validate_update_request(req: &UpdateDashboardRequest) -> Result<(), CreateDashboardError> {
    if req.name.trim().is_empty() {
        return Err(CreateDashboardError::InvalidInput(
            "name is required".into(),
        ));
    }
    for panel in &req.panels {
        let kind = panel_kind(panel);
        if panel.title.trim().is_empty() {
            return Err(CreateDashboardError::InvalidInput(
                "panel title is required".into(),
            ));
        }
        if !VALID_PANEL_KINDS.contains(&kind) {
            return Err(CreateDashboardError::InvalidInput(format!(
                "panel_kind must be one of: {}",
                VALID_PANEL_KINDS.join(", ")
            )));
        }
        if kind == "query" && panel.query_kind.as_deref().is_none() {
            return Err(CreateDashboardError::InvalidInput(
                "query panels require query_kind".into(),
            ));
        }
        if kind == "text" && panel.content.as_ref().is_none_or(|s| s.trim().is_empty()) {
            return Err(CreateDashboardError::InvalidInput(
                "text panels require content".into(),
            ));
        }
        if let Some(query_kind) = &panel.query_kind
            && !VALID_QUERY_KINDS.contains(&query_kind.as_str())
        {
            return Err(CreateDashboardError::InvalidInput(format!(
                "query_kind must be one of: {}",
                VALID_QUERY_KINDS.join(", ")
            )));
        }
        if let Some(layout) = &panel.layout {
            validate_layout(layout)?;
        }
        if let Some(time_range) = &panel.time_range {
            validate_time_range(time_range)?;
        }
        if let Some(ref preset) = panel.preset
            && !VALID_PRESETS.contains(&preset.as_str())
        {
            return Err(CreateDashboardError::InvalidInput(format!(
                "preset must be one of: {} (or omitted for global date range)",
                VALID_PRESETS.join(", ")
            )));
        }
    }
    if let Some(vis) = &req.visibility {
        if !matches!(vis.as_str(), "public" | "private") {
            return Err(CreateDashboardError::InvalidInput(
                "visibility must be 'public' or 'private'".into(),
            ));
        }
    }
    Ok(())
}

fn row_to_panel_item(row: DashboardPanelRow) -> DashboardPanelItem {
    DashboardPanelItem {
        panel_id: row.panel_id,
        title: row.title,
        panel_kind: row.panel_kind,
        query_kind: row.query_kind,
        service: row.service,
        preset: row.preset,
        filters: row.filters,
        query_text: row.query_text,
        content: row.content,
        layout: row.layout,
        time_range: row.time_range,
    }
}

fn empty_object() -> serde_json::Value {
    serde_json::json!({})
}

fn panel_kind(panel: &DashboardPanelRequest) -> &str {
    panel.panel_kind.as_deref().unwrap_or("query")
}

fn normalized_layout(layout: Option<&serde_json::Value>, position: usize) -> serde_json::Value {
    layout.cloned().unwrap_or_else(
        || serde_json::json!({"x": (position % 2) * 6, "y": position / 2 * 4, "w": 6, "h": 4}),
    )
}

fn normalized_time_range(panel: &DashboardPanelRequest) -> serde_json::Value {
    panel.time_range.clone().unwrap_or_else(|| {
        panel
            .preset
            .as_ref()
            .map(|preset| serde_json::json!({"mode":"preset","preset":preset}))
            .unwrap_or_else(|| serde_json::json!({"mode":"global"}))
    })
}

fn validate_layout(layout: &serde_json::Value) -> Result<(), CreateDashboardError> {
    let x = layout.get("x").and_then(|v| v.as_i64());
    let y = layout.get("y").and_then(|v| v.as_i64());
    let w = layout.get("w").and_then(|v| v.as_i64());
    let h = layout.get("h").and_then(|v| v.as_i64());
    let Some((x, y, w, h)) = x.zip(y).zip(w).zip(h).map(|(((x, y), w), h)| (x, y, w, h)) else {
        return Err(CreateDashboardError::InvalidInput(
            "layout requires numeric x, y, w, h".into(),
        ));
    };
    if x < 0 || y < 0 || w < 1 || h < 1 || w > 12 || x + w > 12 {
        return Err(CreateDashboardError::InvalidInput(
            "layout must fit within 12 columns".into(),
        ));
    }
    Ok(())
}

fn validate_time_range(time_range: &serde_json::Value) -> Result<(), CreateDashboardError> {
    match time_range.get("mode").and_then(|v| v.as_str()) {
        Some("global") => Ok(()),
        Some("preset") => {
            let preset = time_range.get("preset").and_then(|v| v.as_str());
            if preset.is_some_and(|preset| VALID_PRESETS.contains(&preset)) {
                Ok(())
            } else {
                Err(CreateDashboardError::InvalidInput(
                    "preset time_range requires a valid preset".into(),
                ))
            }
        }
        Some("absolute") => {
            if time_range.get("from_ms").and_then(|v| v.as_i64()).is_some()
                && time_range.get("to_ms").and_then(|v| v.as_i64()).is_some()
            {
                Ok(())
            } else {
                Err(CreateDashboardError::InvalidInput(
                    "absolute time_range requires from_ms and to_ms".into(),
                ))
            }
        }
        _ => Err(CreateDashboardError::InvalidInput(
            "time_range mode must be global, preset, or absolute".into(),
        )),
    }
}

pub async fn handle_list_dashboards(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<DashboardListResponse>, StatusCode> {
    let items = list_dashboards(&state.db, ctx.tenant_id, ctx.user_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to list dashboards");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(DashboardListResponse { items }))
}

pub async fn handle_create_dashboard(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<CreateDashboardRequest>,
) -> Result<(StatusCode, Json<DashboardItem>), StatusCode> {
    match create_dashboard(&state.db, ctx.tenant_id, &req, ctx.user_id).await {
        Ok(item) => Ok((StatusCode::CREATED, Json(item))),
        Err(CreateDashboardError::InvalidInput(msg)) => {
            tracing::warn!(message = %msg, "invalid dashboard input");
            Err(StatusCode::BAD_REQUEST)
        }
        Err(CreateDashboardError::Db(e)) => {
            tracing::error!(error = %e, "failed to create dashboard");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_get_dashboard(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(dashboard_id): Path<Uuid>,
) -> Result<Json<DashboardItem>, StatusCode> {
    let item = match get_dashboard(&state.db, ctx.tenant_id, dashboard_id).await {
        Ok(Some(item)) => item,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to get dashboard");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    // API-key callers (user_id = None) bypass ReBAC — API keys are tenant-level
    // credentials with full tenant access, not user-scoped. ReBAC only applies to
    // browser/session-authenticated users who have a personal identity.
    if let Some(user_id) = ctx.user_id {
        let relation = fetch_relation(&state.db, user_id, dashboard_id)
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

pub async fn handle_update_dashboard(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(dashboard_id): Path<Uuid>,
    Json(req): Json<UpdateDashboardRequest>,
) -> Result<Json<DashboardItem>, StatusCode> {
    // Confirm the dashboard exists and belongs to this tenant before checking grants.
    // (Prevents existence enumeration: non-existent dashboards return 404, not 403.)
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM dashboards WHERE dashboard_id = $1 AND tenant_id = $2)",
    )
    .bind(dashboard_id)
    .bind(ctx.tenant_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to check dashboard existence");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }
    // API-key callers (user_id = None) bypass ReBAC — tenant-level access.
    // Session users must have owner or editor grant to update.
    if let Some(user_id) = ctx.user_id {
        let relation = fetch_relation(&state.db, user_id, dashboard_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to fetch grant");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !grant_satisfies_write(&ctx.role, relation.as_deref()) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    match update_dashboard(&state.db, ctx.tenant_id, dashboard_id, &req).await {
        Ok(Some(item)) => Ok(Json(item)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(CreateDashboardError::InvalidInput(msg)) => {
            tracing::warn!(message = %msg, "invalid dashboard update");
            Err(StatusCode::BAD_REQUEST)
        }
        Err(CreateDashboardError::Db(e)) => {
            tracing::error!(error = %e, "failed to update dashboard");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_delete_dashboard(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(dashboard_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    // Confirm the dashboard exists and belongs to this tenant before checking grants.
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM dashboards WHERE dashboard_id = $1 AND tenant_id = $2)",
    )
    .bind(dashboard_id)
    .bind(ctx.tenant_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to check dashboard existence");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }
    // API-key callers (user_id = None) bypass ReBAC — tenant-level access.
    // Session users must have owner grant (or tenant_admin role) to delete.
    if let Some(user_id) = ctx.user_id {
        let relation = fetch_relation(&state.db, user_id, dashboard_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to fetch grant");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !grant_satisfies_delete(&ctx.role, relation.as_deref()) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    match delete_dashboard(&state.db, ctx.tenant_id, dashboard_id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to delete dashboard");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_get_dashboard_export(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(dashboard_id): Path<Uuid>,
) -> Result<Json<DashboardExport>, StatusCode> {
    match export_dashboard(&state.db, ctx.tenant_id, dashboard_id).await {
        Ok(Some(export)) => Ok(Json(export)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to export dashboard");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_import_dashboard(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(export): Json<DashboardExport>,
) -> Result<(StatusCode, Json<DashboardItem>), StatusCode> {
    if export.schema_version != "1" && export.schema_version != EXPORT_SCHEMA_VERSION {
        tracing::warn!(schema_version = %export.schema_version, "unsupported dashboard export schema version");
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    match import_dashboard(&state.db, ctx.tenant_id, &export, ctx.user_id).await {
        Ok(item) => Ok((StatusCode::CREATED, Json(item))),
        Err(CreateDashboardError::InvalidInput(msg)) => {
            tracing::warn!(message = %msg, "invalid dashboard import");
            Err(StatusCode::BAD_REQUEST)
        }
        Err(CreateDashboardError::Db(e)) => {
            tracing::error!(error = %e, "failed to import dashboard");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_create_request_rejects_blank_name() {
        let req = CreateDashboardRequest {
            name: "   ".into(),
            panels: vec![DashboardPanelRequest {
                title: "Panel 1".into(),
                query_kind: Some("logs".into()),
                service: None,
                preset: None,
                filters: serde_json::json!({}),
                ..Default::default()
            }],
        };
        assert!(validate_create_request(&req).is_err());
    }

    #[test]
    fn validate_create_request_rejects_empty_panels() {
        let req = CreateDashboardRequest {
            name: "My dashboard".into(),
            panels: vec![],
        };
        assert!(validate_create_request(&req).is_err());
    }

    #[test]
    fn validate_create_request_rejects_blank_panel_title() {
        let req = CreateDashboardRequest {
            name: "My dashboard".into(),
            panels: vec![DashboardPanelRequest {
                title: "   ".into(),
                query_kind: Some("logs".into()),
                service: None,
                preset: None,
                filters: serde_json::json!({}),
                ..Default::default()
            }],
        };
        assert!(validate_create_request(&req).is_err());
    }

    #[test]
    fn validate_create_request_rejects_invalid_query_kind() {
        let req = CreateDashboardRequest {
            name: "My dashboard".into(),
            panels: vec![DashboardPanelRequest {
                title: "Panel 1".into(),
                query_kind: Some("invalid".into()),
                service: None,
                preset: None,
                filters: serde_json::json!({}),
                ..Default::default()
            }],
        };
        assert!(validate_create_request(&req).is_err());
    }

    #[test]
    fn validate_create_request_accepts_null_preset() {
        let req = CreateDashboardRequest {
            name: "My dashboard".into(),
            panels: vec![DashboardPanelRequest {
                title: "Panel 1".into(),
                query_kind: Some("logs".into()),
                service: None,
                preset: None,
                filters: serde_json::json!({}),
                ..Default::default()
            }],
        };
        assert!(validate_create_request(&req).is_ok());
    }

    #[test]
    fn validate_create_request_accepts_valid_preset() {
        let req = CreateDashboardRequest {
            name: "My dashboard".into(),
            panels: vec![DashboardPanelRequest {
                title: "Panel 1".into(),
                query_kind: Some("traces".into()),
                service: None,
                preset: Some("1h".into()),
                filters: serde_json::json!({}),
                ..Default::default()
            }],
        };
        assert!(validate_create_request(&req).is_ok());
    }

    #[test]
    fn validate_create_request_rejects_invalid_preset() {
        let req = CreateDashboardRequest {
            name: "My dashboard".into(),
            panels: vec![DashboardPanelRequest {
                title: "Panel 1".into(),
                query_kind: Some("logs".into()),
                service: None,
                preset: Some("99m".into()),
                filters: serde_json::json!({}),
                ..Default::default()
            }],
        };
        assert!(validate_create_request(&req).is_err());
    }

    // --- ReBAC pure-helper tests ---

    #[test]
    fn public_dashboard_visible_without_grant() {
        assert!(grant_satisfies_read("public", None));
    }

    #[test]
    fn private_dashboard_hidden_without_grant() {
        assert!(!grant_satisfies_read("private", None));
    }

    #[test]
    fn private_dashboard_visible_with_viewer_grant() {
        assert!(grant_satisfies_read("private", Some("viewer")));
    }

    #[test]
    fn private_dashboard_visible_with_editor_grant() {
        assert!(grant_satisfies_read("private", Some("editor")));
    }

    #[test]
    fn private_dashboard_visible_with_owner_grant() {
        assert!(grant_satisfies_read("private", Some("owner")));
    }

    #[test]
    fn viewer_grant_cannot_write() {
        assert!(!grant_satisfies_write("member", Some("viewer")));
    }

    #[test]
    fn editor_grant_can_write() {
        assert!(grant_satisfies_write("member", Some("editor")));
    }

    #[test]
    fn owner_grant_can_write() {
        assert!(grant_satisfies_write("member", Some("owner")));
    }

    #[test]
    fn tenant_admin_can_write_without_grant() {
        assert!(grant_satisfies_write("tenant_admin", None));
    }

    #[test]
    fn viewer_grant_cannot_delete() {
        assert!(!grant_satisfies_delete("member", Some("viewer")));
    }

    #[test]
    fn editor_grant_cannot_delete() {
        assert!(!grant_satisfies_delete("member", Some("editor")));
    }

    #[test]
    fn owner_grant_can_delete() {
        assert!(grant_satisfies_delete("member", Some("owner")));
    }

    #[test]
    fn tenant_admin_can_delete_without_grant() {
        assert!(grant_satisfies_delete("tenant_admin", None));
    }

    #[test]
    fn no_grant_cannot_write() {
        assert!(!grant_satisfies_write("member", None));
    }

    #[test]
    fn no_grant_cannot_delete() {
        assert!(!grant_satisfies_delete("member", None));
    }
}
