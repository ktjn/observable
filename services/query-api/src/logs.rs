use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    Json,
};
use domain::{LogRecord, LogRow};
use serde::{Deserialize, Serialize};

use crate::middleware::auth::TenantContext;
use crate::traces::AppState;

#[derive(Serialize)]
pub struct LogListResponse {
    pub logs: Vec<LogRecord>,
    pub total: u64,
}

#[derive(Deserialize)]
pub struct LogSearchParams {
    pub service: Option<String>,
    pub severity: Option<i32>,
    pub limit: Option<u32>,
}

pub async fn search_logs(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<LogSearchParams>,
) -> Result<Json<LogListResponse>, StatusCode> {
    let limit = params.limit.unwrap_or(50).min(500);

    // Count total matching logs.
    let mut count_sql = "SELECT count() FROM logs WHERE tenant_id = ?".to_string();
    if params.service.is_some() {
        count_sql.push_str(" AND service_name = ?");
    }
    if params.severity.is_some() {
        count_sql.push_str(" AND severity_number >= ?");
    }
    let mut count_query = state.ch.query(&count_sql).bind(ctx.tenant_id);
    if let Some(service) = &params.service {
        count_query = count_query.bind(service);
    }
    if let Some(severity) = params.severity {
        count_query = count_query.bind(severity);
    }
    let total: u64 = count_query.fetch_one().await.map_err(|e| {
        tracing::error!("ClickHouse count error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Fetch logs.
    let mut query = "SELECT ?fields FROM logs WHERE tenant_id = ?".to_string();
    if params.service.is_some() {
        query.push_str(" AND service_name = ?");
    }
    if params.severity.is_some() {
        query.push_str(" AND severity_number >= ?");
    }
    query.push_str(" ORDER BY timestamp_unix_nano DESC LIMIT ?");

    let mut ch_query = state.ch.query(&query).bind(ctx.tenant_id);

    if let Some(service) = &params.service {
        ch_query = ch_query.bind(service);
    }
    if let Some(severity) = params.severity {
        ch_query = ch_query.bind(severity);
    }

    let mut cursor = ch_query.bind(limit).fetch::<LogRow>().map_err(|e| {
        tracing::error!("ClickHouse query error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut logs = Vec::new();
    while let Some(row) = cursor.next().await.map_err(|e| {
        tracing::error!("ClickHouse fetch error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        logs.push(LogRecord::from(row));
    }

    Ok(Json(LogListResponse { logs, total }))
}
