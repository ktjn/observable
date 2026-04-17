use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use clickhouse::Client;
use domain::Span;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub ch: Client,
    pub tenant_id: Uuid,
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
    Path(trace_id): Path<String>,
) -> Result<Json<TraceResponse>, StatusCode> {
    // Full ClickHouse query implemented in Task 12 when storage-writer is ready.
    // Stub returns NOT_FOUND for now so the API contract is wired.
    let _ = (state, &trace_id);
    Err(StatusCode::NOT_FOUND)
}

pub async fn search_traces(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<TraceListResponse>, StatusCode> {
    let limit = params.limit.unwrap_or(50).min(500);
    let _ = (state, limit, params.service);
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
