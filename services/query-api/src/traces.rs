use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use clickhouse::Client;
use domain::{Span, SpanKind, SpanRow, StatusCode as SpanStatus};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

use crate::middleware::auth::TenantContext;

#[derive(Clone)]
pub struct AppState {
    pub ch: Client,
}

#[derive(Serialize)]
pub struct TraceResponse {
    pub trace_id: String,
    pub spans: Vec<Span>,
}

#[derive(Serialize)]
pub struct TraceListResponse {
    pub traces: Vec<TraceResponse>,
    pub total: u64,
}

#[derive(Deserialize)]
pub struct SearchParams {
    pub service: Option<String>,
    pub limit: Option<u32>,
}

const SELECT_COLS: &str = "tenant_id, trace_id, span_id, parent_span_id, service_name, \
    service_namespace, service_version, operation_name, span_kind, \
    start_time_unix_nano, end_time_unix_nano, duration_ns, \
    status_code, status_message, attributes, resource_attributes, \
    environment, host_id, workload, deployment_id";

pub async fn get_trace(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(trace_id): Path<String>,
) -> Result<Json<TraceResponse>, StatusCode> {
    let sql = format!(
        "SELECT {SELECT_COLS} FROM spans \
         WHERE tenant_id = ? AND trace_id = ? \
         ORDER BY start_time_unix_nano \
         LIMIT 1000"
    );
    let rows: Vec<SpanRow> = state
        .ch
        .query(&sql)
        .bind(ctx.tenant_id)
        .bind(trace_id.as_str())
        .fetch_all()
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "ClickHouse query error");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if rows.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }
    let spans: Vec<Span> = rows.into_iter().map(Span::from).collect();
    Ok(Json(TraceResponse { trace_id, spans }))
}

pub async fn search_traces(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<SearchParams>,
) -> Result<Json<TraceListResponse>, StatusCode> {
    let limit = params.limit.unwrap_or(50).min(500) as u64;
    // Fetch more rows than limit to ensure enough distinct traces after de-duplication.
    let fetch = limit * 10;

    let sql_base = format!("SELECT {SELECT_COLS} FROM spans WHERE tenant_id = ?");
    let rows: Vec<SpanRow> = if let Some(ref svc) = params.service {
        let sql =
            format!("{sql_base} AND service_name = ? ORDER BY start_time_unix_nano DESC LIMIT ?");
        state
            .ch
            .query(&sql)
            .bind(ctx.tenant_id)
            .bind(svc.as_str())
            .bind(fetch)
            .fetch_all()
            .await
    } else {
        let sql = format!("{sql_base} ORDER BY start_time_unix_nano DESC LIMIT ?");
        state
            .ch
            .query(&sql)
            .bind(ctx.tenant_id)
            .bind(fetch)
            .fetch_all()
            .await
    }
    .map_err(|e| {
        tracing::error!(error = ?e, "ClickHouse query error");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Group by trace_id; keep first (chronologically latest start) span per trace.
    let mut seen: HashSet<String> = HashSet::new();
    let mut traces: Vec<TraceResponse> = Vec::new();
    for row in rows {
        let trace_id = row.trace_id.clone();
        if seen.insert(trace_id.clone()) {
            traces.push(TraceResponse {
                trace_id,
                spans: vec![Span::from(row)],
            });
            if traces.len() >= limit as usize {
                break;
            }
        }
    }

    let total = traces.len() as u64;
    Ok(Json(TraceListResponse { traces, total }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_row_converts_status() {
        let mut row = make_row("INTERNAL", "ERROR");
        row.status_code = "ERROR".into();
        let span = Span::from(row);
        assert!(matches!(span.status_code, SpanStatus::Error));
    }

    #[test]
    fn span_row_converts_span_kind() {
        let row = make_row("SERVER", "OK");
        let span = Span::from(row);
        assert!(matches!(span.span_kind, SpanKind::Server));
    }

    #[test]
    fn trace_response_serializes() {
        let resp = TraceResponse {
            trace_id: "abc123".into(),
            spans: vec![],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("abc123"));
    }

    fn make_row(span_kind: &str, status_code: &str) -> SpanRow {
        SpanRow {
            tenant_id: Uuid::new_v4(),
            trace_id: "abc".into(),
            span_id: "def".into(),
            parent_span_id: None,
            service_name: "svc".into(),
            service_namespace: "".into(),
            service_version: "".into(),
            operation_name: "op".into(),
            span_kind: span_kind.into(),
            start_time_unix_nano: 0,
            end_time_unix_nano: 1_000_000,
            duration_ns: 1_000_000,
            status_code: status_code.into(),
            status_message: "".into(),
            attributes: "{}".into(),
            resource_attributes: "{}".into(),
            environment: "".into(),
            host_id: "".into(),
            workload: "".into(),
            deployment_id: "".into(),
        }
    }
}
