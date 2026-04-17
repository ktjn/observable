use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use clickhouse::Client;
use domain::Span;
use serde::{Deserialize, Serialize};

use crate::middleware::auth::TenantContext;

#[derive(Clone)]
pub struct AppState {
    #[allow(dead_code)]
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
    // ClickHouse row query wired once Row derive is added to domain::Span (Phase 2+).
    let _ = (state, ctx, &trace_id);
    Err(StatusCode::NOT_FOUND)
}

pub async fn search_traces(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<SearchParams>,
) -> Result<Json<TraceListResponse>, StatusCode> {
    let limit = params.limit.unwrap_or(50).min(500);
    let _ = (state, ctx, limit, params.service);
    Ok(Json(TraceListResponse {
        traces: vec![],
        total: 0,
    }))
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
