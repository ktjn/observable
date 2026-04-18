use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;

#[derive(Serialize)]
pub struct DiscoveryResponse {
    pub items: Vec<String>,
}

pub async fn list_services(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<DiscoveryResponse>, StatusCode> {
    let sql = "SELECT DISTINCT service_name FROM ( \
        SELECT DISTINCT service_name FROM spans WHERE tenant_id = ? \
        UNION DISTINCT \
        SELECT DISTINCT service_name FROM logs WHERE tenant_id = ? \
        UNION DISTINCT \
        SELECT DISTINCT service_name FROM metric_series WHERE tenant_id = ? \
    ) ORDER BY service_name";

    let rows: Vec<String> = state
        .ch
        .query(sql)
        .bind(ctx.tenant_id)
        .bind(ctx.tenant_id)
        .bind(ctx.tenant_id)
        .fetch_all()
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "ClickHouse discovery services error");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(DiscoveryResponse { items: rows }))
}

pub async fn list_environments(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<DiscoveryResponse>, StatusCode> {
    let sql = "SELECT DISTINCT environment FROM ( \
        SELECT DISTINCT environment FROM spans WHERE tenant_id = ? \
        UNION DISTINCT \
        SELECT DISTINCT environment FROM logs WHERE tenant_id = ? \
        UNION DISTINCT \
        SELECT DISTINCT environment FROM metric_series WHERE tenant_id = ? \
    ) WHERE environment != '' ORDER BY environment";

    let rows: Vec<String> = state
        .ch
        .query(sql)
        .bind(ctx.tenant_id)
        .bind(ctx.tenant_id)
        .bind(ctx.tenant_id)
        .fetch_all()
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "ClickHouse discovery environments error");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(DiscoveryResponse { items: rows }))
}
