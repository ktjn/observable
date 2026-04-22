use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    extract::{Extension, Path, Query, State},
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

#[derive(Deserialize)]
pub struct TopologyParams {
    pub environment: Option<String>,
    pub lookback_minutes: Option<u32>,
    pub service: Option<String>,
}

#[derive(Serialize, Deserialize, clickhouse::Row)]
pub struct TopologyRow {
    pub caller: String,
    pub callee: String,
    pub request_count: u64,
    pub error_count: u64,
    pub p95_latency_ns: f64,
}

#[derive(Serialize)]
pub struct TopologyEdge {
    pub caller: String,
    pub callee: String,
    pub request_count: u64,
    pub error_rate: f64,
    pub p95_latency_ms: f64,
}

#[derive(Serialize)]
pub struct TopologyResponse {
    pub edges: Vec<TopologyEdge>,
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
    pub health_state: String,
    pub active_alert_count: u64,
    pub latest_deployment: Option<String>,
}

#[derive(Serialize)]
pub struct ServiceSummaryResponse {
    pub items: Vec<ServiceSummary>,
}

#[derive(Serialize)]
pub struct ServiceDetailResponse {
    pub service: ServiceSummary,
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
        .map(|row| service_summary_from_row(row, duration_secs))
        .collect();

    Ok(Json(ServiceSummaryResponse { items }))
}

pub async fn get_service_summary(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(service_name): Path<String>,
    Query(params): Query<SummaryParams>,
) -> Result<Json<ServiceDetailResponse>, StatusCode> {
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
        WHERE tenant_id = ? AND service_name = ? AND start_time_unix_nano >= ?"
        .to_string();

    if params.environment.is_some() {
        sql.push_str(" AND environment = ?");
    }

    sql.push_str(" GROUP BY service_name LIMIT 1");

    let mut query = state
        .ch
        .query(&sql)
        .bind(ctx.tenant_id)
        .bind(&service_name)
        .bind(start_ns);

    if let Some(ref env) = params.environment {
        query = query.bind(env);
    }

    let row = query
        .fetch_optional::<ServiceSummaryRow>()
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "ClickHouse single service summary error");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let duration_secs = (lookback_mins as f64) * 60.0;
    Ok(Json(ServiceDetailResponse {
        service: service_summary_from_row(row, duration_secs),
    }))
}

pub async fn get_topology(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<TopologyParams>,
) -> Result<Json<TopologyResponse>, StatusCode> {
    let lookback_mins = params.lookback_minutes.unwrap_or(60);
    let lookback_ns = (lookback_mins as u64) * 60 * 1_000_000_000;
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let start_ns = now_ns.saturating_sub(lookback_ns);

    let plan = state.planner.plan_topology(&params);

    let mut query = state
        .ch
        .query(&plan.sql)
        .bind(ctx.tenant_id)
        .bind(ctx.tenant_id)
        .bind(start_ns);

    if let Some(ref env) = params.environment {
        query = query.bind(env).bind(env);
    }

    if let Some(ref service) = params.service {
        query = query.bind(service).bind(service);
    }

    let rows: Vec<TopologyRow> = query.fetch_all().await.map_err(|e| {
        tracing::error!(error = ?e, "ClickHouse topology error");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let edges = rows
        .into_iter()
        .map(|row| {
            let error_rate = if row.request_count > 0 {
                (row.error_count as f64) / (row.request_count as f64)
            } else {
                0.0
            };
            TopologyEdge {
                caller: row.caller,
                callee: row.callee,
                request_count: row.request_count,
                error_rate,
                p95_latency_ms: row.p95_latency_ns / 1_000_000.0,
            }
        })
        .collect();

    Ok(Json(TopologyResponse { edges }))
}

fn service_summary_from_row(row: ServiceSummaryRow, duration_secs: f64) -> ServiceSummary {
    let error_rate = if row.request_count > 0 {
        (row.error_count as f64) / (row.request_count as f64)
    } else {
        0.0
    };

    ServiceSummary {
        service_name: row.service_name,
        request_rate: (row.request_count as f64) / duration_secs,
        error_rate,
        p95_latency_ms: row.p95_latency_ns / 1_000_000.0,
        health_state: health_state(error_rate).to_string(),
        active_alert_count: 0,
        latest_deployment: None,
    }
}

fn health_state(error_rate: f64) -> &'static str {
    if error_rate > 0.05 {
        "breach"
    } else if error_rate > 0.01 {
        "watch"
    } else {
        "healthy"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_row_derives_red_metrics_and_health() {
        let summary = service_summary_from_row(
            ServiceSummaryRow {
                service_name: "checkout".into(),
                request_count: 120,
                error_count: 3,
                p95_latency_ns: 245_000_000.0,
            },
            60.0,
        );

        assert_eq!(summary.service_name, "checkout");
        assert_eq!(summary.request_rate, 2.0);
        assert_eq!(summary.error_rate, 0.025);
        assert_eq!(summary.p95_latency_ms, 245.0);
        assert_eq!(summary.health_state, "watch");
        assert_eq!(summary.active_alert_count, 0);
        assert_eq!(summary.latest_deployment, None);
    }

    #[test]
    fn health_state_thresholds_are_stable() {
        assert_eq!(health_state(0.0), "healthy");
        assert_eq!(health_state(0.02), "watch");
        assert_eq!(health_state(0.06), "breach");
    }
}
