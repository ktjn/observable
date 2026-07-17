use axum::{
    Json,
    extract::{Extension, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AdminServiceAppState,
    middleware::auth::{TenantContext, require_admin},
};

#[derive(Debug, Deserialize)]
pub struct UsageReportQuery {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct UsageTelemetrySummary {
    pub spans: u64,
    pub logs: u64,
    pub metric_points: u64,
    pub metric_series_created: u64,
}

#[derive(Debug, Serialize)]
pub struct UsageControlPlaneSummary {
    pub query_reads: u64,
    pub query_rows: u64,
    pub credential_checks: u64,
    pub credential_allows: u64,
    pub credential_denies: u64,
}

#[derive(Debug, Serialize)]
pub struct UsageReportResponse {
    pub tenant_id: Uuid,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub telemetry_summary: UsageTelemetrySummary,
    pub control_plane_summary: UsageControlPlaneSummary,
    pub estimated_cost_index: u64,
}

fn to_unix_nano(dt: DateTime<Utc>) -> u64 {
    dt.timestamp_nanos_opt().unwrap_or(0) as u64
}

fn to_unix_seconds(dt: DateTime<Utc>) -> i64 {
    dt.timestamp()
}

async fn count_clickhouse_rows(
    state: &AdminServiceAppState,
    tenant_id: Uuid,
    table: &str,
    time_column: &str,
    from_ns: u64,
    to_ns: u64,
) -> Result<u64, clickhouse::error::Error> {
    let sql = format!(
        "SELECT count() FROM observable.{table} \
         WHERE tenant_id = ? \
           AND {time_column} >= ? \
           AND {time_column} < ?"
    );
    state
        .ch
        .query(&sql)
        .bind(tenant_id)
        .bind(from_ns)
        .bind(to_ns)
        .fetch_one()
        .await
}

async fn count_metric_series_created(
    state: &AdminServiceAppState,
    tenant_id: Uuid,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<u64, clickhouse::error::Error> {
    state
        .ch
        .query(
            "SELECT count() FROM observable.metric_series \
             WHERE tenant_id = ? \
               AND created_at >= toDateTime(?) \
               AND created_at < toDateTime(?)",
        )
        .bind(tenant_id)
        .bind(to_unix_seconds(from))
        .bind(to_unix_seconds(to))
        .fetch_one()
        .await
}

async fn query_pg_usage(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<UsageControlPlaneSummary, sqlx::Error> {
    let (query_reads, query_rows): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*)::BIGINT, COALESCE(SUM(result_count), 0)::BIGINT \
         FROM query_audit_log \
         WHERE tenant_id = $1 \
           AND occurred_at >= $2 \
           AND occurred_at < $3",
    )
    .bind(tenant_id)
    .bind(from)
    .bind(to)
    .fetch_one(db)
    .await?;

    let (credential_checks, credential_allows, credential_denies): (i64, i64, i64) =
        sqlx::query_as(
            "SELECT \
             COUNT(*)::BIGINT, \
             COUNT(*) FILTER (WHERE outcome = 'allow')::BIGINT, \
             COUNT(*) FILTER (WHERE outcome = 'deny')::BIGINT \
         FROM credential_audit_log \
         WHERE tenant_id = $1 \
           AND occurred_at >= $2 \
           AND occurred_at < $3",
        )
        .bind(tenant_id)
        .bind(from)
        .bind(to)
        .fetch_one(db)
        .await?;

    Ok(UsageControlPlaneSummary {
        query_reads: query_reads as u64,
        query_rows: query_rows.max(0) as u64,
        credential_checks: credential_checks as u64,
        credential_allows: credential_allows as u64,
        credential_denies: credential_denies as u64,
    })
}

fn cost_index(telemetry: &UsageTelemetrySummary, control: &UsageControlPlaneSummary) -> u64 {
    telemetry.spans * 10
        + telemetry.logs * 6
        + telemetry.metric_points * 2
        + telemetry.metric_series_created * 5
        + control.query_reads * 3
        + control.query_rows
        + control.credential_checks * 4
}

pub async fn get_tenant_usage_report(
    state: &AdminServiceAppState,
    tenant_id: Uuid,
    query: &UsageReportQuery,
) -> anyhow::Result<UsageReportResponse> {
    if query.to <= query.from {
        anyhow::bail!("invalid reporting interval");
    }

    let from_ns = to_unix_nano(query.from);
    let to_ns = to_unix_nano(query.to);

    let spans = count_clickhouse_rows(
        state,
        tenant_id,
        "spans",
        "start_time_unix_nano",
        from_ns,
        to_ns,
    )
    .await?;
    let logs = count_clickhouse_rows(
        state,
        tenant_id,
        "logs",
        "timestamp_unix_nano",
        from_ns,
        to_ns,
    )
    .await?;
    let metric_points = count_clickhouse_rows(
        state,
        tenant_id,
        "metric_points",
        "time_unix_nano",
        from_ns,
        to_ns,
    )
    .await?;
    let metric_series_created =
        count_metric_series_created(state, tenant_id, query.from, query.to).await?;
    let telemetry_summary = UsageTelemetrySummary {
        spans,
        logs,
        metric_points,
        metric_series_created,
    };

    let control_plane_summary = query_pg_usage(&state.db, tenant_id, query.from, query.to).await?;
    let estimated_cost_index = cost_index(&telemetry_summary, &control_plane_summary);

    Ok(UsageReportResponse {
        tenant_id,
        from: query.from,
        to: query.to,
        telemetry_summary,
        control_plane_summary,
        estimated_cost_index,
    })
}

pub async fn handle_get_tenant_usage_report(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(query): Query<UsageReportQuery>,
) -> Result<Json<UsageReportResponse>, StatusCode> {
    require_admin(&ctx)?;
    match get_tenant_usage_report(&state, ctx.tenant_id, &query).await {
        Ok(report) => Ok(Json(report)),
        Err(e) => {
            tracing::error!(error = %e, "failed to get tenant usage report");
            if e.to_string().contains("invalid reporting interval") {
                Err(StatusCode::UNPROCESSABLE_ENTITY)
            } else {
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}
