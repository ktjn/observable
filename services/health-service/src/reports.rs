//! Scheduled status reports (W5b, OBS-13-07, docs/assurance-relocations.md 4.5).
//!
//! - `POST /v1/health/report-schedules` (signed-in tenant admin) requests a weekly or monthly
//!   report for a business scope of catalog services in one environment class. It starts as
//!   `pending_approval`.
//! - `POST /v1/health/report-schedules/{id}/approve`: a second tenant admin approves it. Only
//!   approved schedules run.
//! - The report worker writes one snapshot per completed period after approval. A snapshot
//!   holds exactly the scoped services, each with its own telemetry scope, state at the end of
//!   the period, evidence freshness and trend over the period.
//! - `GET /v1/health/report-schedules/{id}/reports` reads the snapshots with the caller's own
//!   catalog visibility: entries for services the caller may not read are omitted and never
//!   counted.

use std::collections::HashSet;
use std::time::Duration;

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Datelike, Months, NaiveDate, TimeZone, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::HealthState;
use crate::history::{Transition, interruptions};
use crate::ports::{Caller, CatalogInputs, EnvironmentClass, ServiceReadError};
use crate::scope::{Scope, scope};

fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(json!({ "error": code }))).into_response()
}

fn store_error(e: sqlx::Error) -> Response {
    tracing::warn!(error = %e, "health report store unavailable");
    error(StatusCode::SERVICE_UNAVAILABLE, "report_store_unavailable")
}

fn signed_in_admin(caller: &Caller) -> Option<Uuid> {
    caller.user_id.filter(|_| caller.role == "tenant_admin")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Weekly,
    Monthly,
}

impl Period {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "weekly" => Some(Self::Weekly),
            "monthly" => Some(Self::Monthly),
            _ => None,
        }
    }

    /// The period that contains `t`: ISO weeks from Monday 00:00 UTC, or calendar months.
    pub fn containing(self, t: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
        let day = t.date_naive();
        let start: NaiveDate = match self {
            Self::Weekly => {
                day - chrono::Days::new(u64::from(day.weekday().num_days_from_monday()))
            }
            Self::Monthly => day.with_day(1).expect("first of month"),
        };
        let end = match self {
            Self::Weekly => start + chrono::Days::new(7),
            Self::Monthly => start + Months::new(1),
        };
        let at = |d: NaiveDate| Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).expect("midnight"));
        (at(start), at(end))
    }
}

#[derive(Debug, Deserialize)]
pub struct ScheduleScope {
    #[serde(default)]
    pub service_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct ScheduleBody {
    pub name: String,
    pub scope: ScheduleScope,
    pub environment_class: String,
    pub period: String,
}

#[derive(sqlx::FromRow)]
struct ScheduleRow {
    schedule_id: Uuid,
    tenant_id: Uuid,
    name: String,
    service_ids: Vec<Uuid>,
    environment_class: String,
    period: String,
    status: String,
    requested_by: Uuid,
    approved_by: Option<Uuid>,
    approved_at: Option<DateTime<Utc>>,
}

const SCHEDULE_COLUMNS: &str = "schedule_id, tenant_id, name, service_ids, environment_class, \
     period, status, requested_by, approved_by, approved_at";

impl ScheduleRow {
    fn to_json(&self) -> Value {
        json!({
            "schedule_id": self.schedule_id,
            "name": self.name,
            "scope": {"service_ids": self.service_ids},
            "environment_class": self.environment_class,
            "period": self.period,
            "status": self.status,
            "requested_by": self.requested_by,
            "approved_by": self.approved_by,
            "approved_at": self.approved_at,
        })
    }
}

/// `POST /v1/health/report-schedules`.
pub async fn create_schedule(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Json(body): Json<ScheduleBody>,
) -> Response {
    let caller = match crate::guard::caller(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let Some(admin) = signed_in_admin(&caller) else {
        return error(StatusCode::FORBIDDEN, "tenant_admin_required");
    };
    let name = body.name.trim();
    let mut seen = HashSet::new();
    let services: Vec<Uuid> = body
        .scope
        .service_ids
        .iter()
        .copied()
        .filter(|s| seen.insert(*s))
        .collect();
    if name.is_empty()
        || name.chars().count() > 200
        || services.is_empty()
        || services.len() > 200
        || EnvironmentClass::parse(&body.environment_class).is_none()
        || Period::parse(&body.period).is_none()
    {
        return error(StatusCode::BAD_REQUEST, "invalid_schedule");
    }
    // The requester must be able to read every service in the scope.
    for id in &services {
        match state.services.read(&caller, &headers, *id).await {
            Ok(_) => {}
            Err(ServiceReadError::NotFound) => {
                return error(StatusCode::BAD_REQUEST, "unknown_service");
            }
            Err(ServiceReadError::Unauthorized) => {
                return error(StatusCode::UNAUTHORIZED, "unauthorized");
            }
            Err(ServiceReadError::Unavailable) => {
                return error(StatusCode::SERVICE_UNAVAILABLE, "catalog_unavailable");
            }
        }
    }
    let row: Result<ScheduleRow, _> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "INSERT INTO health.report_schedules \
           (tenant_id, name, service_ids, environment_class, period, requested_by) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {SCHEDULE_COLUMNS}"
    )))
    .bind(caller.tenant_id)
    .bind(name)
    .bind(&services)
    .bind(&body.environment_class)
    .bind(&body.period)
    .bind(admin)
    .fetch_one(&state.db)
    .await;
    match row {
        Ok(r) => (StatusCode::CREATED, Json(r.to_json())).into_response(),
        Err(e) => store_error(e),
    }
}

async fn schedule(db: &PgPool, tenant: Uuid, id: Uuid) -> sqlx::Result<Option<ScheduleRow>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {SCHEDULE_COLUMNS} FROM health.report_schedules \
         WHERE tenant_id = $1 AND schedule_id = $2"
    )))
    .bind(tenant)
    .bind(id)
    .fetch_optional(db)
    .await
}

/// `POST /v1/health/report-schedules/{id}/approve`: a tenant admin other than the requester.
pub async fn approve_schedule(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let caller = match crate::guard::caller(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let Some(admin) = signed_in_admin(&caller) else {
        return error(StatusCode::FORBIDDEN, "tenant_admin_required");
    };
    let row = match schedule(&state.db, caller.tenant_id, id).await {
        Ok(Some(r)) => r,
        Ok(None) => return error(StatusCode::NOT_FOUND, "not_found"),
        Err(e) => return store_error(e),
    };
    if row.requested_by == admin {
        return error(StatusCode::FORBIDDEN, "requester_cannot_approve");
    }
    let updated: Result<Option<ScheduleRow>, _> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "UPDATE health.report_schedules SET status = 'approved', approved_by = $3, \
         approved_at = now() WHERE tenant_id = $1 AND schedule_id = $2 \
         AND status = 'pending_approval' RETURNING {SCHEDULE_COLUMNS}"
    )))
    .bind(caller.tenant_id)
    .bind(id)
    .bind(admin)
    .fetch_optional(&state.db)
    .await;
    match updated {
        Ok(Some(r)) => Json(r.to_json()).into_response(),
        Ok(None) => error(StatusCode::CONFLICT, "schedule_already_approved"),
        Err(e) => store_error(e),
    }
}

fn rank(state: &str) -> Option<u8> {
    match state {
        "normal" => Some(0),
        "degraded" => Some(1),
        "unavailable" => Some(2),
        _ => None,
    }
}

/// The service's last transition strictly before `t`: its state at `t`.
async fn last_before(
    db: &PgPool,
    tenant: Uuid,
    id: Uuid,
    class: EnvironmentClass,
    t: DateTime<Utc>,
) -> sqlx::Result<Option<Transition>> {
    sqlx::query_as(
        "SELECT at, from_state, to_state, evidence_status FROM health.transitions \
         WHERE tenant_id = $1 AND service_id = $2 AND environment_class = $3 AND at < $4 \
         ORDER BY at DESC LIMIT 1",
    )
    .bind(tenant)
    .bind(id)
    .bind(class.as_str())
    .bind(t)
    .fetch_optional(db)
    .await
}

/// One scoped service's entry for the period `[start, end)`.
async fn entry(
    db: &PgPool,
    schedule: &ScheduleRow,
    class: EnvironmentClass,
    scoped: Scope,
    id: Uuid,
    (start, end): (DateTime<Utc>, DateTime<Utc>),
) -> sqlx::Result<Value> {
    let at_start = last_before(db, schedule.tenant_id, id, class, start).await?;
    let at_end = last_before(db, schedule.tenant_id, id, class, end).await?;
    let during: Vec<Transition> = sqlx::query_as(
        "SELECT at, from_state, to_state, evidence_status FROM health.transitions \
         WHERE tenant_id = $1 AND service_id = $2 AND environment_class = $3 \
           AND at >= $4 AND at < $5 ORDER BY at",
    )
    .bind(schedule.tenant_id)
    .bind(id)
    .bind(class.as_str())
    .bind(start)
    .bind(end)
    .fetch_all(db)
    .await?;
    let (state, evidence_status, as_of) = match &at_end {
        Some(t) => (t.to_state.as_str(), t.evidence_status.as_str(), Some(t.at)),
        None => ("unknown", "missing", None),
    };
    let trend = match (
        at_start.as_ref().and_then(|t| rank(&t.to_state)),
        rank(state),
    ) {
        (Some(a), Some(b)) if b < a => "improving",
        (Some(a), Some(b)) if b > a => "degrading",
        (Some(_), Some(_)) => "stable",
        _ => "insufficient_data",
    };
    let scope_json = match scoped {
        Scope::Bound(ids) => json!({"status": "bound", "identities": ids.iter().map(|i| json!({
            "service_name": i.service_name, "environment": i.environment})).collect::<Vec<_>>()}),
        Scope::NotEvaluated => json!({"status": "not_evaluated", "identities": []}),
        Scope::Unavailable => json!({"status": "unavailable", "identities": []}),
    };
    let incidents = interruptions(&during);
    Ok(json!({
        "service_id": id,
        "environment_class": class.as_str(),
        "scope": scope_json,
        "state": state,
        "evidence_status": evidence_status,
        "state_as_of": as_of,
        "trend": trend,
        "interruptions": incidents.len(),
        "transitions": during.len(),
    }))
}

/// Writes the snapshot of the last completed period of every approved schedule that does not
/// have one yet, for periods that ended after approval. A tenant whose catalog inputs are
/// unavailable is skipped and retried on the next pass. Returns the number written.
pub async fn generate_due(db: &PgPool, inputs: &CatalogInputs, now: DateTime<Utc>) -> u64 {
    let schedules: Vec<ScheduleRow> = match sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {SCHEDULE_COLUMNS} FROM health.report_schedules WHERE status = 'approved' \
         ORDER BY tenant_id, schedule_id"
    )))
    .fetch_all(db)
    .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "report schedules unavailable");
            return 0;
        }
    };
    let mut written = 0;
    for s in schedules {
        let (Some(period), Some(class), Some(approved_at)) = (
            Period::parse(&s.period),
            EnvironmentClass::parse(&s.environment_class),
            s.approved_at,
        ) else {
            continue;
        };
        let (current_start, _) = period.containing(now);
        let previous = period.containing(current_start - chrono::Duration::seconds(1));
        if previous.1 <= approved_at {
            continue;
        }
        let exists: Result<Option<(Uuid,)>, _> = sqlx::query_as(
            "SELECT report_id FROM health.status_reports WHERE schedule_id = $1 AND period_start = $2",
        )
        .bind(s.schedule_id)
        .bind(previous.0)
        .fetch_optional(db)
        .await;
        match exists {
            Ok(None) => {}
            Ok(Some(_)) => continue,
            Err(e) => {
                tracing::warn!(error = %e, "status report lookup failed");
                continue;
            }
        }
        let Ok(tenant_inputs) = inputs.inputs(s.tenant_id).await else {
            tracing::warn!(tenant = %s.tenant_id, "catalog inputs unavailable; report deferred");
            continue;
        };
        let mut entries = Vec::with_capacity(s.service_ids.len());
        let mut failed = false;
        for id in &s.service_ids {
            let scoped = scope(&tenant_inputs, *id, class);
            match entry(db, &s, class, scoped, *id, previous).await {
                Ok(e) => entries.push(e),
                Err(e) => {
                    tracing::warn!(error = %e, "status report entry failed");
                    failed = true;
                    break;
                }
            }
        }
        if failed {
            continue;
        }
        let res = sqlx::query(
            "INSERT INTO health.status_reports (schedule_id, tenant_id, period_start, period_end, \
             entries) VALUES ($1, $2, $3, $4, $5) ON CONFLICT (schedule_id, period_start) DO NOTHING",
        )
        .bind(s.schedule_id)
        .bind(s.tenant_id)
        .bind(previous.0)
        .bind(previous.1)
        .bind(Value::Array(entries))
        .execute(db)
        .await;
        match res {
            Ok(r) => written += r.rows_affected(),
            Err(e) => tracing::warn!(error = %e, "status report write failed"),
        }
    }
    written
}

pub async fn start_report_worker(db: PgPool, inputs: CatalogInputs, interval: Duration) {
    let mut tick = tokio::time::interval(interval);
    loop {
        tick.tick().await;
        let n = generate_due(&db, &inputs, Utc::now()).await;
        if n > 0 {
            tracing::info!(written = n, "status reports written");
        }
    }
}

/// `(report_id, period_start, period_end, entries, generated_at)`.
type ReportRow = (Uuid, DateTime<Utc>, DateTime<Utc>, Value, DateTime<Utc>);

/// `GET /v1/health/report-schedules/{id}/reports`.
pub async fn list_reports(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let caller = match crate::guard::caller(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let row = match schedule(&state.db, caller.tenant_id, id).await {
        Ok(Some(r)) => r,
        Ok(None) => return error(StatusCode::NOT_FOUND, "not_found"),
        Err(e) => return store_error(e),
    };
    let mut visible = HashSet::new();
    let mut withheld = false;
    for sid in &row.service_ids {
        match state.services.read(&caller, &headers, *sid).await {
            Ok(_) => {
                visible.insert(*sid);
            }
            Err(ServiceReadError::NotFound) => withheld = true,
            Err(ServiceReadError::Unauthorized) => {
                return error(StatusCode::UNAUTHORIZED, "unauthorized");
            }
            Err(ServiceReadError::Unavailable) => {
                return error(StatusCode::SERVICE_UNAVAILABLE, "catalog_unavailable");
            }
        }
    }
    let reports: Vec<ReportRow> = match sqlx::query_as(
        "SELECT report_id, period_start, period_end, entries, generated_at \
             FROM health.status_reports WHERE tenant_id = $1 AND schedule_id = $2 \
             ORDER BY period_start DESC",
    )
    .bind(caller.tenant_id)
    .bind(id)
    .fetch_all(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => return store_error(e),
    };
    let mut schedule_json = row.to_json();
    // The scope lists only services the caller may read.
    schedule_json["scope"]["service_ids"] = json!(
        row.service_ids
            .iter()
            .filter(|s| visible.contains(*s))
            .collect::<Vec<_>>()
    );
    let items: Vec<Value> = reports
        .into_iter()
        .map(|(report_id, start, end, entries, at)| {
            let entries: Vec<Value> = entries
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|e| {
                    e["service_id"]
                        .as_str()
                        .and_then(|s| s.parse::<Uuid>().ok())
                        .is_some_and(|s| visible.contains(&s))
                })
                .collect();
            json!({"report_id": report_id, "period_start": start, "period_end": end,
                   "generated_at": at, "entries": entries})
        })
        .collect();
    Json(json!({
        "schedule": schedule_json,
        "reports": items,
        "entries_withheld": withheld,
    }))
    .into_response()
}
