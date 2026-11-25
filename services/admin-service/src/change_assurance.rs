//! W5a (docs/assurance-relocations.md): partial release comparison (OBS-11-08), approved
//! change freezes with their exceptions (OBS-11-09), and the cost totals alerting's benefits
//! review reads (OBS-17-06).

use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AdminServiceAppState;
use crate::middleware::auth::TenantContext;

/// A population below this many requests is reported as insufficient, never as zero.
pub const MIN_SAMPLE: u64 = 100;

fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(json!({ "error": code }))).into_response()
}

fn store_error(e: sqlx::Error) -> Response {
    tracing::error!(error = %e, "change assurance store error");
    error(StatusCode::SERVICE_UNAVAILABLE, "store_unavailable")
}

fn signed_in_admin(ctx: &TenantContext) -> Option<Uuid> {
    ctx.user_id.filter(|_| ctx.role == "tenant_admin")
}

#[derive(sqlx::FromRow)]
struct Marker {
    deployment_id: Uuid,
    service_name: String,
    environment: String,
    service_version: String,
    status: String,
    started_at: DateTime<Utc>,
    deployed_by: Option<String>,
}

async fn marker(
    state: &AdminServiceAppState,
    tenant: Uuid,
    id: Uuid,
) -> sqlx::Result<Option<Marker>> {
    sqlx::query_as(
        "SELECT deployment_id, service_name, environment, service_version, status, started_at, \
         deployed_by FROM control.deployment_markers WHERE tenant_id = $1 AND deployment_id = $2",
    )
    .bind(tenant)
    .bind(id)
    .fetch_optional(&state.db)
    .await
}

// --- Partial release comparison (OBS-11-08) ---------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ComparisonQuery {
    pub window_minutes: Option<i64>,
}

#[derive(clickhouse::Row, Deserialize)]
struct PopulationRow {
    exposed: u8,
    requests: u64,
    errors: u64,
    p95_ns: f64,
}

/// `GET /v1/deployments/{id}/rollout-comparison?window_minutes=`: requests on the deployed
/// version (exposed) against every other version of the same service and environment
/// (unexposed) over the window after the deployment started.
pub async fn handle_rollout_comparison(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(id): Path<Uuid>,
    Query(q): Query<ComparisonQuery>,
) -> Response {
    let window = q.window_minutes.unwrap_or(60);
    if !(1..=7 * 24 * 60).contains(&window) {
        return error(StatusCode::BAD_REQUEST, "invalid_window");
    }
    let m = match marker(&state, ctx.tenant_id, id).await {
        Ok(Some(m)) => m,
        Ok(None) => return error(StatusCode::NOT_FOUND, "not_found"),
        Err(e) => return store_error(e),
    };
    let from = m.started_at;
    let to = (from + Duration::minutes(window)).min(Utc::now());
    let ns = |t: DateTime<Utc>| t.timestamp_nanos_opt().unwrap_or(0).max(0) as u64;
    let rows: Vec<PopulationRow> = match state
        .ch
        .query(
            "SELECT toUInt8(service_version = ?) AS exposed, count() AS requests, \
             countIf(status_code = 'ERROR') AS errors, \
             toFloat64(quantile(0.95)(duration_ns)) AS p95_ns \
             FROM observable.spans \
             WHERE tenant_id = ? AND service_name = ? AND environment = ? \
               AND start_time_unix_nano >= ? AND start_time_unix_nano < ? \
             GROUP BY exposed",
        )
        .bind(&m.service_version)
        .bind(ctx.tenant_id)
        .bind(&m.service_name)
        .bind(&m.environment)
        .bind(ns(from))
        .bind(ns(to))
        .fetch_all()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "rollout comparison read failed");
            return error(StatusCode::SERVICE_UNAVAILABLE, "telemetry_unavailable");
        }
    };
    let population = |exposed: u8| -> Value {
        let r = rows.iter().find(|r| r.exposed == exposed);
        let requests = r.map_or(0, |r| r.requests);
        if requests < MIN_SAMPLE {
            return json!({"requests": requests, "basis": "insufficient_data",
                          "min_sample": MIN_SAMPLE});
        }
        let r = r.expect("requests > 0");
        json!({
            "requests": r.requests,
            "error_rate": r.errors as f64 / r.requests as f64,
            "p95_latency_ms": r.p95_ns / 1e6,
            "basis": "measured",
        })
    };
    Json(json!({
        "deployment_id": m.deployment_id,
        "service_name": m.service_name,
        "environment": m.environment,
        "service_version": m.service_version,
        "window": {"from": from, "to": to},
        "exposed": population(1),
        "unexposed": population(0),
    }))
    .into_response()
}

// --- Change freezes (OBS-11-09) ---------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct FreezeBody {
    pub name: String,
    pub environment: String,
    #[serde(default)]
    pub services: Vec<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

/// `POST /v1/change-freezes`: a signed-in tenant admin approves a freeze period.
pub async fn handle_create_freeze(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(b): Json<FreezeBody>,
) -> Response {
    let Some(admin) = signed_in_admin(&ctx) else {
        return error(StatusCode::FORBIDDEN, "tenant_admin_required");
    };
    let name = b.name.trim();
    if name.is_empty()
        || name.chars().count() > 200
        || b.environment.trim().is_empty()
        || b.ends_at <= b.starts_at
        || b.services.len() > 200
    {
        return error(StatusCode::BAD_REQUEST, "invalid_freeze");
    }
    let row: Result<(Uuid,), _> = sqlx::query_as(
        "INSERT INTO control.change_freezes \
           (tenant_id, name, environment, services, starts_at, ends_at, approved_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING freeze_id",
    )
    .bind(ctx.tenant_id)
    .bind(name)
    .bind(b.environment.trim())
    .bind(&b.services)
    .bind(b.starts_at)
    .bind(b.ends_at)
    .bind(admin)
    .fetch_one(&state.db)
    .await;
    match row {
        Ok((id,)) => (
            StatusCode::CREATED,
            Json(
                json!({"freeze_id": id, "name": name, "environment": b.environment.trim(),
                        "services": b.services, "starts_at": b.starts_at, "ends_at": b.ends_at,
                        "approved_by": admin}),
            ),
        )
            .into_response(),
        Err(e) => store_error(e),
    }
}

#[derive(sqlx::FromRow)]
struct Freeze {
    freeze_id: Uuid,
    name: String,
    environment: String,
    services: Vec<String>,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    approved_by: Uuid,
}

async fn freeze(
    state: &AdminServiceAppState,
    tenant: Uuid,
    id: Uuid,
) -> sqlx::Result<Option<Freeze>> {
    sqlx::query_as(
        "SELECT freeze_id, name, environment, services, starts_at, ends_at, approved_by \
         FROM control.change_freezes WHERE tenant_id = $1 AND freeze_id = $2",
    )
    .bind(tenant)
    .bind(id)
    .fetch_optional(&state.db)
    .await
}

fn in_scope(f: &Freeze, m: &Marker) -> bool {
    m.environment == f.environment
        && (f.services.is_empty() || f.services.contains(&m.service_name))
        && m.started_at >= f.starts_at
        && m.started_at < f.ends_at
}

#[derive(Debug, Deserialize)]
pub struct ExceptionBody {
    pub freeze_id: Uuid,
    pub reason: String,
}

/// `POST /v1/deployments/{id}/freeze-exception`: a signed-in tenant admin other than the
/// deployer approves the change as an exception to the freeze it falls in.
pub async fn handle_freeze_exception(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(id): Path<Uuid>,
    Json(b): Json<ExceptionBody>,
) -> Response {
    let Some(admin) = signed_in_admin(&ctx) else {
        return error(StatusCode::FORBIDDEN, "tenant_admin_required");
    };
    let reason = b.reason.trim();
    if reason.is_empty() || reason.chars().count() > 1000 {
        return error(StatusCode::BAD_REQUEST, "reason_required");
    }
    let (m, f) = match (
        marker(&state, ctx.tenant_id, id).await,
        freeze(&state, ctx.tenant_id, b.freeze_id).await,
    ) {
        (Ok(Some(m)), Ok(Some(f))) => (m, f),
        (Ok(_), Ok(_)) => return error(StatusCode::NOT_FOUND, "not_found"),
        (Err(e), _) | (_, Err(e)) => return store_error(e),
    };
    if !in_scope(&f, &m) {
        return error(StatusCode::CONFLICT, "deployment_not_in_freeze");
    }
    let deployer = m.deployed_by.as_deref().unwrap_or_default();
    if deployer == admin.to_string() || deployer == format!("user:{admin}") {
        return error(
            StatusCode::FORBIDDEN,
            "deployer_cannot_approve_own_exception",
        );
    }
    let res = sqlx::query(
        "INSERT INTO control.freeze_exceptions (tenant_id, freeze_id, deployment_id, approved_by, reason) \
         VALUES ($1, $2, $3, $4, $5) ON CONFLICT (freeze_id, deployment_id) DO NOTHING",
    )
    .bind(ctx.tenant_id)
    .bind(f.freeze_id)
    .bind(m.deployment_id)
    .bind(admin)
    .bind(reason)
    .execute(&state.db)
    .await;
    match res {
        Ok(r) if r.rows_affected() == 0 => {
            error(StatusCode::CONFLICT, "exception_already_recorded")
        }
        Ok(_) => (
            StatusCode::CREATED,
            Json(
                json!({"freeze_id": f.freeze_id, "deployment_id": m.deployment_id,
                        "approved_by": admin, "reason": reason}),
            ),
        )
            .into_response(),
        Err(e) => store_error(e),
    }
}

/// `GET /v1/change-freezes/{id}/changes`: every in-scope change made during the freeze, with
/// its approved exception or marked `unapproved`.
pub async fn handle_freeze_changes(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(id): Path<Uuid>,
) -> Response {
    let f = match freeze(&state, ctx.tenant_id, id).await {
        Ok(Some(f)) => f,
        Ok(None) => return error(StatusCode::NOT_FOUND, "not_found"),
        Err(e) => return store_error(e),
    };
    let markers: Vec<Marker> = match sqlx::query_as(
        "SELECT deployment_id, service_name, environment, service_version, status, started_at, \
         deployed_by FROM control.deployment_markers \
         WHERE tenant_id = $1 AND environment = $2 AND started_at >= $3 AND started_at < $4 \
         ORDER BY started_at, deployment_id",
    )
    .bind(ctx.tenant_id)
    .bind(&f.environment)
    .bind(f.starts_at)
    .bind(f.ends_at)
    .fetch_all(&state.db)
    .await
    {
        Ok(m) => m,
        Err(e) => return store_error(e),
    };
    let exceptions: Vec<(Uuid, Uuid, String, DateTime<Utc>)> = match sqlx::query_as(
        "SELECT deployment_id, approved_by, reason, approved_at FROM control.freeze_exceptions \
         WHERE tenant_id = $1 AND freeze_id = $2",
    )
    .bind(ctx.tenant_id)
    .bind(id)
    .fetch_all(&state.db)
    .await
    {
        Ok(e) => e,
        Err(e) => return store_error(e),
    };
    let changes: Vec<Value> = markers
        .iter()
        .filter(|m| in_scope(&f, m))
        .map(|m| {
            let exception = exceptions.iter().find(|e| e.0 == m.deployment_id);
            json!({
                "deployment_id": m.deployment_id,
                "service_name": m.service_name,
                "service_version": m.service_version,
                "status": m.status,
                "started_at": m.started_at,
                "deployed_by": m.deployed_by,
                "approval": if exception.is_some() { "exception" } else { "unapproved" },
                "exception": exception.map(|e| json!({"approved_by": e.1, "reason": e.2,
                                                      "approved_at": e.3})),
            })
        })
        .collect();
    Json(json!({
        "freeze": {"freeze_id": f.freeze_id, "name": f.name, "environment": f.environment,
                   "services": f.services, "starts_at": f.starts_at, "ends_at": f.ends_at,
                   "approved_by": f.approved_by},
        "changes": changes,
    }))
    .into_response()
}

// --- Cost totals for the benefits review (OBS-17-06) ------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CostTotalsQuery {
    pub tenant_id: Uuid,
    pub period: String,
}

/// `GET /internal/cost/totals?tenant_id=&period=`: the tenant's recorded observability cost
/// for a period, or null when nothing was recorded for it (internal token).
pub async fn handle_internal_cost_totals(
    State(state): State<AdminServiceAppState>,
    Query(q): Query<CostTotalsQuery>,
) -> Response {
    let total: Result<Option<i64>, _> = sqlx::query_scalar(
        "SELECT sum(expenditure_cents)::bigint FROM control.service_cost_attribution \
         WHERE tenant_id = $1 AND period = $2",
    )
    .bind(q.tenant_id)
    .bind(&q.period)
    .fetch_one(&state.db)
    .await;
    match total {
        Ok(t) => Json(json!({"tenant_id": q.tenant_id, "period": q.period, "total_cents": t}))
            .into_response(),
        Err(e) => store_error(e),
    }
}
