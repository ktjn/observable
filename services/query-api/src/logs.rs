use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use domain::{LogRecord, LogRow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::middleware::auth::TenantContext;
use crate::traces::AppState;

#[derive(Serialize)]
pub struct FacetValue {
    pub value: String,
    pub count: u64,
}

#[derive(Serialize)]
pub struct LogListResponse {
    pub logs: Vec<LogRecord>,
    pub total: u64,
    pub facets: HashMap<String, Vec<FacetValue>>,
}

#[derive(Deserialize)]
pub struct LogSearchParams {
    pub service: Option<String>,
    pub severity: Option<i32>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub limit: Option<u32>,
    pub facets: Option<String>, // Comma-separated list of fields to facet
}

#[derive(Deserialize)]
pub struct LogTailParams {
    pub service: Option<String>,
    pub severity: Option<i32>,
    pub since_unix_nano: Option<u64>,
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
    if params.trace_id.is_some() {
        count_sql.push_str(" AND trace_id = ?");
    }
    if params.span_id.is_some() {
        count_sql.push_str(" AND span_id = ?");
    }
    let mut count_query = state.ch.query(&count_sql).bind(ctx.tenant_id);
    if let Some(service) = &params.service {
        count_query = count_query.bind(service);
    }
    if let Some(severity) = params.severity {
        count_query = count_query.bind(severity);
    }
    if let Some(trace_id) = &params.trace_id {
        count_query = count_query.bind(trace_id);
    }
    if let Some(span_id) = &params.span_id {
        count_query = count_query.bind(span_id);
    }
    let total: u64 = count_query.fetch_one().await.map_err(|e| {
        tracing::error!("ClickHouse count error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Handle facets.
    let mut facet_results = HashMap::new();
    if let Some(facets_str) = params.facets {
        let requested_facets: Vec<&str> = facets_str.split(',').map(|s| s.trim()).collect();

        for field in requested_facets {
            // Validate field name to prevent SQL injection.
            let valid_fields = ["service_name", "severity_number", "environment", "host_id"];
            if !valid_fields.contains(&field) {
                continue;
            }

            let mut facet_sql =
                format!("SELECT {field} as value, count() as count FROM logs WHERE tenant_id = ?");
            if params.service.is_some() {
                facet_sql.push_str(" AND service_name = ?");
            }
            if params.severity.is_some() {
                facet_sql.push_str(" AND severity_number >= ?");
            }
            if params.trace_id.is_some() {
                facet_sql.push_str(" AND trace_id = ?");
            }
            if params.span_id.is_some() {
                facet_sql.push_str(" AND span_id = ?");
            }
            facet_sql.push_str(&format!(" GROUP BY {field} ORDER BY count DESC LIMIT 10"));

            let mut facet_query = state.ch.query(&facet_sql).bind(ctx.tenant_id);
            if let Some(service) = &params.service {
                facet_query = facet_query.bind(service);
            }
            if let Some(severity) = params.severity {
                facet_query = facet_query.bind(severity);
            }
            if let Some(trace_id) = &params.trace_id {
                facet_query = facet_query.bind(trace_id);
            }
            if let Some(span_id) = &params.span_id {
                facet_query = facet_query.bind(span_id);
            }

            let mut cursor = facet_query.fetch::<(String, u64)>().map_err(|e| {
                tracing::error!("ClickHouse facet query error: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

            let mut values = Vec::new();
            while let Some((value, count)) = cursor.next().await.map_err(|e| {
                tracing::error!("ClickHouse facet fetch error: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })? {
                values.push(FacetValue { value, count });
            }
            facet_results.insert(field.to_string(), values);
        }
    }

    // Fetch logs.
    let mut query = "SELECT ?fields FROM logs WHERE tenant_id = ?".to_string();
    if params.service.is_some() {
        query.push_str(" AND service_name = ?");
    }
    if params.severity.is_some() {
        query.push_str(" AND severity_number >= ?");
    }
    if params.trace_id.is_some() {
        query.push_str(" AND trace_id = ?");
    }
    if params.span_id.is_some() {
        query.push_str(" AND span_id = ?");
    }
    query.push_str(" ORDER BY timestamp_unix_nano DESC LIMIT ?");

    let mut ch_query = state.ch.query(&query).bind(ctx.tenant_id);

    if let Some(service) = &params.service {
        ch_query = ch_query.bind(service);
    }
    if let Some(severity) = params.severity {
        ch_query = ch_query.bind(severity);
    }
    if let Some(trace_id) = &params.trace_id {
        ch_query = ch_query.bind(trace_id);
    }
    if let Some(span_id) = &params.span_id {
        ch_query = ch_query.bind(span_id);
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

    let result_count = rows.len() as i64;
    let logs = rows.into_iter().map(LogRecord::from).collect();

    crate::audit::write(
        &state.db,
        &crate::audit::QueryAuditEntry {
            action: "log_search",
            tenant_id: ctx.tenant_id,
            result_count,
        },
    )
    .await;

    Ok(Json(LogListResponse {
        logs,
        total,
        facets: facet_results,
    }))
}

pub async fn tail_logs(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<LogTailParams>,
) -> Result<Json<LogListResponse>, StatusCode> {
    let limit = params.limit.unwrap_or(100).min(500);

    let mut query = "SELECT ?fields FROM logs WHERE tenant_id = ?".to_string();
    if params.service.is_some() {
        query.push_str(" AND service_name = ?");
    }
    if params.severity.is_some() {
        query.push_str(" AND severity_number >= ?");
    }
    if params.since_unix_nano.is_some() {
        query.push_str(" AND timestamp_unix_nano > ?");
    }
    query.push_str(" ORDER BY timestamp_unix_nano ASC LIMIT ?");

    let mut ch_query = state.ch.query(&query).bind(ctx.tenant_id);

    if let Some(service) = &params.service {
        ch_query = ch_query.bind(service);
    }
    if let Some(severity) = params.severity {
        ch_query = ch_query.bind(severity);
    }
    if let Some(since_unix_nano) = params.since_unix_nano {
        ch_query = ch_query.bind(since_unix_nano);
    }

    let mut cursor = ch_query.bind(limit).fetch::<LogRow>().map_err(|e| {
        tracing::error!("ClickHouse live tail query error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut rows = Vec::new();
    while let Some(row) = cursor.next().await.map_err(|e| {
        tracing::error!("ClickHouse live tail fetch error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        rows.push(row);
    }

    validate_log_rows_for_tenant(&rows, ctx.tenant_id)?;

    let result_count = rows.len() as i64;
    let logs = rows.into_iter().map(LogRecord::from).collect();

    crate::audit::write(
        &state.db,
        &crate::audit::QueryAuditEntry {
            action: "log_live_tail",
            tenant_id: ctx.tenant_id,
            result_count,
        },
    )
    .await;

    Ok(Json(LogListResponse {
        logs,
        total: result_count as u64,
        facets: HashMap::new(),
    }))
}

#[derive(Deserialize)]
pub struct LogContextParams {
    pub window: Option<u32>,
}

pub async fn get_log_context(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(log_id): Path<Uuid>,
    Query(params): Query<LogContextParams>,
) -> Result<Json<LogListResponse>, StatusCode> {
    let window = params.window.unwrap_or(10).min(100);

    // 1. Fetch the pivot log
    let pivot_row: LogRow = state
        .ch
        .query("SELECT ?fields FROM logs WHERE tenant_id = ? AND log_id = ?")
        .bind(ctx.tenant_id)
        .bind(log_id)
        .fetch_optional::<LogRow>()
        .await
        .map_err(|e| {
            tracing::error!("ClickHouse pivot fetch error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // 2. Fetch logs BEFORE
    let mut before_cursor = state
        .ch
        .query("SELECT ?fields FROM logs WHERE tenant_id = ? AND service_name = ? AND host_id = ? AND timestamp_unix_nano < ? ORDER BY timestamp_unix_nano DESC LIMIT ?")
        .bind(ctx.tenant_id)
        .bind(&pivot_row.service_name)
        .bind(&pivot_row.host_id)
        .bind(pivot_row.timestamp_unix_nano)
        .bind(window)
        .fetch::<LogRow>()
        .map_err(|e| {
            tracing::error!("ClickHouse before fetch error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut before_rows = Vec::new();
    while let Some(row) = before_cursor.next().await.map_err(|e| {
        tracing::error!("ClickHouse before next error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        before_rows.push(row);
    }
    before_rows.reverse();

    // 3. Fetch logs AFTER
    let mut after_cursor = state
        .ch
        .query("SELECT ?fields FROM logs WHERE tenant_id = ? AND service_name = ? AND host_id = ? AND timestamp_unix_nano > ? ORDER BY timestamp_unix_nano ASC LIMIT ?")
        .bind(ctx.tenant_id)
        .bind(&pivot_row.service_name)
        .bind(&pivot_row.host_id)
        .bind(pivot_row.timestamp_unix_nano)
        .bind(window)
        .fetch::<LogRow>()
        .map_err(|e| {
            tracing::error!("ClickHouse after fetch error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut after_rows = Vec::new();
    while let Some(row) = after_cursor.next().await.map_err(|e| {
        tracing::error!("ClickHouse after next error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        after_rows.push(row);
    }

    let mut all_rows = before_rows;
    all_rows.push(pivot_row);
    all_rows.extend(after_rows);

    validate_log_rows_for_tenant(&all_rows, ctx.tenant_id)?;

    let result_count = all_rows.len() as i64;
    let logs = all_rows.into_iter().map(LogRecord::from).collect();

    crate::audit::write(
        &state.db,
        &crate::audit::QueryAuditEntry {
            action: "log_context",
            tenant_id: ctx.tenant_id,
            result_count,
        },
    )
    .await;

    Ok(Json(LogListResponse {
        logs,
        total: result_count as u64,
        facets: HashMap::new(),
    }))
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

    #[test]
    fn make_log_row_with_trace_context() {
        let tenant_id = Uuid::new_v4();
        let mut row = make_log_row(tenant_id);
        row.trace_id = Some("trace-1".into());
        row.span_id = Some("span-1".into());

        assert_eq!(row.trace_id, Some("trace-1".into()));
        assert_eq!(row.span_id, Some("span-1".into()));
    }

    #[test]
    fn validate_combined_context_rows() {
        let tenant_id = Uuid::new_v4();
        let before = vec![make_log_row(tenant_id), make_log_row(tenant_id)];
        let pivot = make_log_row(tenant_id);
        let after = vec![make_log_row(tenant_id)];

        let mut all = before;
        all.push(pivot);
        all.extend(after);

        assert_eq!(validate_log_rows_for_tenant(&all, tenant_id), Ok(()));
    }

    #[test]
    fn reject_combined_context_rows_on_tenant_mismatch() {
        let tenant_id = Uuid::new_v4();
        let other = Uuid::new_v4();
        let before = vec![make_log_row(tenant_id)];
        let pivot = make_log_row(other); // Mismatch!
        let after = vec![make_log_row(tenant_id)];

        let mut all = before;
        all.push(pivot);
        all.extend(after);

        assert_eq!(
            validate_log_rows_for_tenant(&all, tenant_id),
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        );
    }

    #[test]
    fn log_tail_params_accept_cursor_and_filters() {
        let params = LogTailParams {
            service: Some("checkout".into()),
            severity: Some(9),
            since_unix_nano: Some(42),
            limit: Some(25),
        };

        assert_eq!(params.service.as_deref(), Some("checkout"));
        assert_eq!(params.severity, Some(9));
        assert_eq!(params.since_unix_nano, Some(42));
        assert_eq!(params.limit, Some(25));
    }
}
