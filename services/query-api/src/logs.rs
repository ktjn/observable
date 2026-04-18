use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    Json,
};
use domain::{LogRecord, LogRow};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

    let mut rows = Vec::new();
    while let Some(row) = cursor.next().await.map_err(|e| {
        tracing::error!("ClickHouse fetch error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        rows.push(row);
    }

    validate_log_rows_for_tenant(&rows, ctx.tenant_id)?;

    let logs = rows.into_iter().map(LogRecord::from).collect();

    Ok(Json(LogListResponse { logs, total }))
}

fn validate_log_rows_for_tenant(rows: &[LogRow], tenant_id: Uuid) -> Result<(), StatusCode> {
    if rows.iter().all(|row| row.tenant_id == tenant_id) {
        return Ok(());
    }

    tracing::error!(
        expected_tenant_id = %tenant_id,
        "log query returned rows outside tenant context"
    );
    Err(StatusCode::INTERNAL_SERVER_ERROR)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_log_row(tenant_id: Uuid) -> LogRow {
        LogRow {
            tenant_id,
            log_id: Uuid::new_v4(),
            timestamp_unix_nano: 0,
            observed_timestamp_unix_nano: 0,
            severity_number: 0,
            severity_text: String::new(),
            body: "{}".into(),
            trace_id: None,
            span_id: None,
            attributes: "{}".into(),
            resource_attributes: "{}".into(),
            service_name: "svc".into(),
            environment: String::new(),
            host_id: String::new(),
            fingerprint: None,
        }
    }

    #[test]
    fn log_rows_validate_for_same_tenant() {
        let tenant_id = Uuid::new_v4();
        let rows = vec![make_log_row(tenant_id), make_log_row(tenant_id)];
        assert_eq!(validate_log_rows_for_tenant(&rows, tenant_id), Ok(()));
    }

    #[test]
    fn log_rows_reject_cross_tenant_result() {
        let tenant_id = Uuid::new_v4();
        let other = Uuid::new_v4();
        let rows = vec![make_log_row(tenant_id), make_log_row(other)];
        assert_eq!(
            validate_log_rows_for_tenant(&rows, tenant_id),
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        );
    }

    #[test]
    fn empty_log_rows_are_valid() {
        let tenant_id = Uuid::new_v4();
        assert_eq!(validate_log_rows_for_tenant(&[], tenant_id), Ok(()));
    }
}
