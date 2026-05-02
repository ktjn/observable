use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{extract::State, http::StatusCode, Extension, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const VALID_QUERY_KINDS: &[&str] = &["logs", "traces", "metrics"];

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct DashboardPanelItem {
    pub panel_id: Uuid,
    pub title: String,
    pub query_kind: String,
    pub service: Option<String>,
    pub preset: Option<String>,
    pub filters: serde_json::Value,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct DashboardItem {
    pub dashboard_id: Uuid,
    pub name: String,
    pub panels: Vec<DashboardPanelItem>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct DashboardListResponse {
    pub items: Vec<DashboardItem>,
}

#[derive(Deserialize)]
pub struct DashboardPanelRequest {
    pub title: String,
    pub query_kind: String,
    pub service: Option<String>,
    pub preset: Option<String>,
    pub filters: serde_json::Value,
}

#[derive(Deserialize)]
pub struct CreateDashboardRequest {
    pub name: String,
    pub panels: Vec<DashboardPanelRequest>,
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
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct DashboardPanelRow {
    dashboard_id: Uuid,
    panel_id: Uuid,
    title: String,
    query_kind: String,
    service: Option<String>,
    preset: Option<String>,
    filters: serde_json::Value,
}

pub async fn list_dashboards(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
) -> Result<Vec<DashboardItem>, sqlx::Error> {
    let dashboards = sqlx::query_as::<_, DashboardRow>(
        "SELECT dashboard_id, name, created_at \
         FROM dashboards \
         WHERE tenant_id = $1 \
         ORDER BY created_at DESC",
    )
    .bind(tenant_id)
    .fetch_all(db)
    .await?;

    let dashboard_ids: Vec<Uuid> = dashboards.iter().map(|d| d.dashboard_id).collect();
    let panels = if dashboard_ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as::<_, DashboardPanelRow>(
            "SELECT dashboard_id, panel_id, title, query_kind, service, preset, filters \
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
                    query_kind: panel.query_kind.clone(),
                    service: panel.service.clone(),
                    preset: panel.preset.clone(),
                    filters: panel.filters.clone(),
                })
                .collect(),
            dashboard_id: dashboard.dashboard_id,
            name: dashboard.name,
            created_at: dashboard.created_at,
        })
        .collect())
}

pub async fn create_dashboard(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    req: &CreateDashboardRequest,
) -> Result<DashboardItem, CreateDashboardError> {
    validate_create_request(req)?;

    let mut tx = db.begin().await.map_err(CreateDashboardError::Db)?;
    let row = sqlx::query_as::<_, DashboardRow>(
        "INSERT INTO dashboards (tenant_id, name) VALUES ($1, $2) \
         RETURNING dashboard_id, name, created_at",
    )
    .bind(tenant_id)
    .bind(req.name.trim())
    .fetch_one(&mut *tx)
    .await
    .map_err(CreateDashboardError::Db)?;

    let mut panels = Vec::with_capacity(req.panels.len());
    for (position, panel) in req.panels.iter().enumerate() {
        let item = sqlx::query_as::<_, DashboardPanelRow>(
            "INSERT INTO dashboard_panels \
             (dashboard_id, title, query_kind, service, preset, filters, position) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             RETURNING dashboard_id, panel_id, title, query_kind, service, preset, filters",
        )
        .bind(row.dashboard_id)
        .bind(panel.title.trim())
        .bind(&panel.query_kind)
        .bind(panel.service.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()))
        .bind(panel.preset.as_deref())
        .bind(&panel.filters)
        .bind(position as i32)
        .fetch_one(&mut *tx)
        .await
        .map_err(CreateDashboardError::Db)?;

        panels.push(DashboardPanelItem {
            panel_id: item.panel_id,
            title: item.title,
            query_kind: item.query_kind,
            service: item.service,
            preset: item.preset,
            filters: item.filters,
        });
    }

    tx.commit().await.map_err(CreateDashboardError::Db)?;

    Ok(DashboardItem {
        dashboard_id: row.dashboard_id,
        name: row.name,
        panels,
        created_at: row.created_at,
    })
}

const VALID_PRESETS: &[&str] = &["5m", "15m", "30m", "1h", "3h", "12h"];

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
        if panel.title.trim().is_empty() {
            return Err(CreateDashboardError::InvalidInput(
                "panel title is required".into(),
            ));
        }
        if !VALID_QUERY_KINDS.contains(&panel.query_kind.as_str()) {
            return Err(CreateDashboardError::InvalidInput(format!(
                "query_kind must be one of: {}",
                VALID_QUERY_KINDS.join(", ")
            )));
        }
        if let Some(ref preset) = panel.preset {
            if !VALID_PRESETS.contains(&preset.as_str()) {
                return Err(CreateDashboardError::InvalidInput(format!(
                    "preset must be one of: {} (or omitted for global date range)",
                    VALID_PRESETS.join(", ")
                )));
            }
        }
    }
    Ok(())
}

pub async fn handle_list_dashboards(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<DashboardListResponse>, StatusCode> {
    let items = list_dashboards(&state.db, ctx.tenant_id)
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
    match create_dashboard(&state.db, ctx.tenant_id, &req).await {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_create_request_rejects_blank_name() {
        let req = CreateDashboardRequest {
            name: "   ".into(),
            panels: vec![DashboardPanelRequest {
                title: "Panel 1".into(),
                query_kind: "logs".into(),
                service: None,
                preset: None,
                filters: serde_json::json!({}),
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
                query_kind: "logs".into(),
                service: None,
                preset: None,
                filters: serde_json::json!({}),
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
                query_kind: "invalid".into(),
                service: None,
                preset: None,
                filters: serde_json::json!({}),
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
                query_kind: "logs".into(),
                service: None,
                preset: None,
                filters: serde_json::json!({}),
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
                query_kind: "traces".into(),
                service: None,
                preset: Some("1h".into()),
                filters: serde_json::json!({}),
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
                query_kind: "logs".into(),
                service: None,
                preset: Some("99m".into()),
                filters: serde_json::json!({}),
            }],
        };
        assert!(validate_create_request(&req).is_err());
    }
}
