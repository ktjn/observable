use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct DiscoveryResponse {
    pub items: Vec<String>,
}

#[derive(Deserialize)]
pub struct SummaryParams {
    pub environment: Option<String>,
    pub lookback_minutes: Option<u32>,
}

#[derive(Serialize, Deserialize, clickhouse::Row)]
pub struct ServiceSummaryRow {
    pub service_name: String,
    pub request_count: u64,
    pub error_count: u64,
    pub p95_latency_ns: f64,
}

#[derive(Serialize)]
pub struct ServiceSummary {
    pub service_name: String,
    pub request_rate: f64,
    pub error_rate: f64,
    pub p95_latency_ms: f64,
}

#[derive(Serialize)]
pub struct ServiceSummaryResponse {
    pub items: Vec<ServiceSummary>,
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

pub async fn list_service_summaries(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<SummaryParams>,
) -> Result<Json<ServiceSummaryResponse>, StatusCode> {
    let lookback_mins = params.lookback_minutes.unwrap_or(60);
    let lookback_ns = (lookback_mins as u64) * 60 * 1_000_000_000;
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let start_ns = now_ns.saturating_sub(lookback_ns);

    let mut sql = "SELECT \
            service_name, \
            count() as request_count, \
            countIf(status_code = 'ERROR') as error_count, \
            quantile(0.95)(duration_ns) as p95_latency_ns \
        FROM spans \
        WHERE tenant_id = ? AND start_time_unix_nano >= ?"
        .to_string();

    if params.environment.is_some() {
        sql.push_str(" AND environment = ?");
    }

    sql.push_str(" GROUP BY service_name ORDER BY service_name");

    let mut query = state.ch.query(&sql).bind(ctx.tenant_id).bind(start_ns);

    if let Some(ref env) = params.environment {
        query = query.bind(env);
    }

    let rows: Vec<ServiceSummaryRow> = query.fetch_all().await.map_err(|e| {
        tracing::error!(error = ?e, "ClickHouse service summary error");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let duration_secs = (lookback_mins as f64) * 60.0;
    let items = rows
        .into_iter()
        .map(|r| ServiceSummary {
            service_name: r.service_name,
            request_rate: (r.request_count as f64) / duration_secs,
            error_rate: if r.request_count > 0 {
                (r.error_count as f64) / (r.request_count as f64)
            } else {
                0.0
            },
            p95_latency_ms: r.p95_latency_ns / 1_000_000.0,
        })
        .collect();

    Ok(Json(ServiceSummaryResponse { items }))
}
