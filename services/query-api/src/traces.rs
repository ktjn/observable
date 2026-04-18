use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use clickhouse::Client;
use domain::{Span, SpanRow};
use serde::{Deserialize, Serialize};

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

pub async fn get_trace(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(trace_id): Path<String>,
) -> Result<Json<TraceResponse>, StatusCode> {
    let mut cursor = state
        .ch
        .query("SELECT ?fields FROM spans WHERE tenant_id = ? AND trace_id = ?")
        .bind(ctx.tenant_id)
        .bind(&trace_id)
        .fetch::<SpanRow>()
        .map_err(|e| {
            tracing::error!("ClickHouse query error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut spans = Vec::new();
    while let Some(row) = cursor.next().await.map_err(|e| {
        tracing::error!("ClickHouse fetch error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        spans.push(Span::from(row));
    }

    if spans.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(Json(TraceResponse { trace_id, spans }))
}

pub async fn search_traces(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<SearchParams>,
) -> Result<Json<TraceListResponse>, StatusCode> {
    let limit = params.limit.unwrap_or(50).min(500);

    let mut query = "SELECT DISTINCT trace_id FROM spans WHERE tenant_id = ?".to_string();
    if params.service.is_some() {
        query.push_str(" AND service_name = ?");
    }
    query.push_str(" ORDER BY start_time_unix_nano DESC LIMIT ?");

    let mut ch_query = state.ch.query(&query).bind(ctx.tenant_id);

    if let Some(service) = params.service {
        ch_query = ch_query.bind(service);
    }

    let mut cursor = ch_query.bind(limit).fetch::<String>().map_err(|e| {
        tracing::error!("ClickHouse query error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut traces = Vec::new();
    while let Some(trace_id) = cursor.next().await.map_err(|e| {
        tracing::error!("ClickHouse fetch error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        // For MVP, we just return the trace_id with empty spans for the list view,
        // or we could fetch the first span.
        // The TraceResponse expects Vec<Span>.
        traces.push(TraceResponse {
            trace_id,
            spans: vec![],
        });
    }

    let total = traces.len() as u64;

    Ok(Json(TraceListResponse { traces, total }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_response_serializes() {
        let resp = TraceResponse {
            trace_id: "abc123".into(),
            spans: vec![],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("abc123"));
    }
}
