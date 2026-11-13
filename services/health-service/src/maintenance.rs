//! Planned maintenance windows (phase 2 E2).
//!
//! - `POST /v1/health/services/{id}/maintenance` requests a window for one environment class.
//! - `POST /v1/health/maintenance/{window_id}/approve` approves it; the requester cannot.
//! - `POST /v1/health/maintenance/{window_id}/cancel` cancels a pending or approved window.
//! - `GET /v1/health/services/{id}/maintenance` lists the service's windows.
//!
//! Order of checks, each failing closed: credential, `decide-access` read on
//! `health.maintenance` for the requested id before any lookup, the caller's own catalog read
//! of the service, then for writes a user session and either `decide-access` write on
//! `health.maintenance` (classified as the service) or being the service's active
//! operational owner in person.
//!
//! A window affects health only while approved, not cancelled and current: failing evidence
//! (`degraded`, `unavailable`) is disclosed as `planned_maintenance` with the underlying
//! state. `normal` and `unknown` are never changed by a window.

use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use observable_auth::authz::{AccessDecider, Classification, DecideRequest, Decision, ResourceRef};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::HealthState;
use crate::ports::{Caller, EnvironmentClass, ServiceReadError, ServiceView};

pub const MAINTENANCE: &str = "health.maintenance";
/// Longest window, and how far ahead a window may start.
pub const MAX_WINDOW_DAYS: i64 = 7;
pub const MAX_LEAD_DAYS: i64 = 90;

#[derive(Clone, Debug, PartialEq, Eq, sqlx::FromRow)]
pub struct Window {
    pub id: Uuid,
    pub service_id: Uuid,
    pub environment_class: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub reason: String,
    pub requested_by: Uuid,
    pub requested_at: DateTime<Utc>,
    pub approved_by: Option<Uuid>,
    pub approved_at: Option<DateTime<Utc>>,
    pub cancelled_by: Option<Uuid>,
    pub cancelled_at: Option<DateTime<Utc>>,
}

impl Window {
    /// `cancelled`, `expired`, `pending`, `approved` (not started) or `active`.
    pub fn status(&self, now: DateTime<Utc>) -> &'static str {
        if self.cancelled_at.is_some() {
            "cancelled"
        } else if self.ends_at <= now {
            "expired"
        } else if self.approved_at.is_none() {
            "pending"
        } else if self.starts_at > now {
            "approved"
        } else {
            "active"
        }
    }

    pub fn to_json(&self, now: DateTime<Utc>) -> serde_json::Value {
        json!({
            "id": self.id,
            "service_id": self.service_id,
            "environment_class": self.environment_class,
            "starts_at": self.starts_at,
            "ends_at": self.ends_at,
            "reason": self.reason,
            "status": self.status(now),
            "requested_by": self.requested_by,
            "requested_at": self.requested_at,
            "approved_by": self.approved_by,
            "approved_at": self.approved_at,
            "cancelled_by": self.cancelled_by,
            "cancelled_at": self.cancelled_at,
        })
    }
}

const COLUMNS: &str = "id, service_id, environment_class, starts_at, ends_at, reason, \
     requested_by, requested_at, approved_by, approved_at, cancelled_by, cancelled_at";

/// The approved, uncancelled window covering `now`, if any.
pub async fn active(
    db: &PgPool,
    tenant_id: Uuid,
    service_id: Uuid,
    class: EnvironmentClass,
    now: DateTime<Utc>,
) -> Result<Option<Window>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM health.maintenance_windows WHERE tenant_id = $1 \
         AND service_id = $2 AND environment_class = $3 AND approved_at IS NOT NULL \
         AND cancelled_at IS NULL AND starts_at <= $4 AND ends_at > $4 \
         ORDER BY starts_at LIMIT 1"
    )))
    .bind(tenant_id)
    .bind(service_id)
    .bind(class.as_str())
    .bind(now)
    .fetch_optional(db)
    .await
}

/// The disclosed state: failing evidence during an active window is `planned_maintenance`.
pub fn apply(state: &'static str, window: Option<&Window>) -> &'static str {
    match (window, state) {
        (Some(_), "degraded" | "unavailable") => "planned_maintenance",
        _ => state,
    }
}

fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(json!({ "error": code }))).into_response()
}

fn invalid(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "invalid_request", "message": message })),
    )
        .into_response()
}

fn not_found() -> Response {
    error(StatusCode::NOT_FOUND, "not_found")
}

fn unavailable(code: &str) -> Response {
    error(StatusCode::SERVICE_UNAVAILABLE, code)
}

async fn caller(state: &HealthState, headers: &HeaderMap) -> Result<Caller, Response> {
    state.credentials.verify(headers).await.map_err(|e| {
        let status = StatusCode::from(e);
        let code = if status == StatusCode::BAD_REQUEST {
            "invalid_request"
        } else {
            "unauthorized"
        };
        error(status, code)
    })
}

async fn decide(
    state: &HealthState,
    caller: &Caller,
    action: &str,
    id: &str,
    classification: Classification,
) -> Result<bool, Response> {
    let req = DecideRequest {
        tenant_id: Some(caller.tenant_id),
        principal: Some(caller.principal()),
        action: Some(action.into()),
        resource: Some(ResourceRef::new(MAINTENANCE, id, classification)),
        purpose: None,
        elevation_id: None,
    };
    match state.decider.decide(&req).await {
        Ok(Decision::Permit) => Ok(true),
        Ok(Decision::Deny | Decision::PurposeMissing) => Ok(false),
        Err(_) => Err(unavailable("access_decision_unavailable")),
    }
}

/// Credential, request-level read decision on `raw_id`, then the parsed id.
async fn admit(
    state: &HealthState,
    headers: &HeaderMap,
    raw_id: &str,
) -> Result<(Caller, Uuid), Response> {
    let caller = caller(state, headers).await?;
    if !decide(state, &caller, "read", raw_id, Classification::Public).await? {
        return Err(not_found());
    }
    let id = raw_id.parse::<Uuid>().map_err(|_| not_found())?;
    Ok((caller, id))
}

async fn service(
    state: &HealthState,
    caller: &Caller,
    headers: &HeaderMap,
    id: Uuid,
) -> Result<ServiceView, Response> {
    match state.services.read(caller, headers, id).await {
        Ok(s) => Ok(s),
        Err(ServiceReadError::NotFound) => Err(not_found()),
        Err(ServiceReadError::Unauthorized) => Err(error(StatusCode::UNAUTHORIZED, "unauthorized")),
        Err(ServiceReadError::Unavailable) => Err(unavailable("access_decision_unavailable")),
    }
}

/// The acting user, when the caller may manage maintenance for the service.
async fn manager(
    state: &HealthState,
    caller: &Caller,
    service: &ServiceView,
) -> Result<Uuid, Response> {
    // Requests and approvals name accountable people; API keys have none.
    let Some(user) = caller.user_id else {
        return Err(error(StatusCode::FORBIDDEN, "user_session_required"));
    };
    let class =
        Classification::parse(&service.classification).unwrap_or(Classification::Restricted);
    if service.operated_by(user)
        || decide(state, caller, "write", &service.id.to_string(), class).await?
    {
        Ok(user)
    } else {
        Err(error(StatusCode::FORBIDDEN, "forbidden"))
    }
}

fn store_error(e: sqlx::Error) -> Response {
    tracing::warn!(error = %e, "health maintenance store unavailable");
    unavailable("maintenance_store_unavailable")
}

pub async fn list(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (caller, id) = match admit(&state, &headers, &raw_id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    if let Err(r) = service(&state, &caller, &headers, id).await {
        return r;
    }
    let rows: Result<Vec<Window>, _> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM health.maintenance_windows WHERE tenant_id = $1 \
         AND service_id = $2 ORDER BY starts_at DESC, id LIMIT 100"
    )))
    .bind(caller.tenant_id)
    .bind(id)
    .fetch_all(&state.db)
    .await;
    match rows {
        Ok(rows) => {
            let now = Utc::now();
            Json(json!({
                "service_id": id,
                "windows": rows.iter().map(|w| w.to_json(now)).collect::<Vec<_>>(),
            }))
            .into_response()
        }
        Err(e) => store_error(e),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestBody {
    environment_class: String,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    reason: String,
}

pub async fn request(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    body: Bytes,
) -> Response {
    let (caller, id) = match admit(&state, &headers, &raw_id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let service = match service(&state, &caller, &headers, id).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    if !matches!(service.lifecycle.as_str(), "live" | "deprecated") {
        return not_found();
    }
    let user = match manager(&state, &caller, &service).await {
        Ok(u) => u,
        Err(r) => return r,
    };
    let body: RequestBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => return invalid(&e.to_string()),
    };
    let Some(class) = EnvironmentClass::parse(&body.environment_class) else {
        return invalid("environment_class must be production or non_production");
    };
    let now = Utc::now();
    let reason = body.reason.trim();
    if reason.is_empty() || reason.chars().count() > 500 {
        return invalid("reason must be 1 to 500 characters");
    }
    if body.ends_at <= body.starts_at {
        return invalid("ends_at must be after starts_at");
    }
    if body.ends_at - body.starts_at > Duration::days(MAX_WINDOW_DAYS) {
        return invalid("a window lasts at most 7 days");
    }
    if body.ends_at <= now {
        return invalid("ends_at must be in the future");
    }
    if body.starts_at > now + Duration::days(MAX_LEAD_DAYS) {
        return invalid("a window starts within 90 days");
    }
    let row: Result<Window, _> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "INSERT INTO health.maintenance_windows (id, tenant_id, service_id, environment_class, \
         starts_at, ends_at, reason, requested_by) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         RETURNING {COLUMNS}"
    )))
    .bind(Uuid::new_v4())
    .bind(caller.tenant_id)
    .bind(id)
    .bind(class.as_str())
    .bind(body.starts_at)
    .bind(body.ends_at)
    .bind(reason)
    .bind(user)
    .fetch_one(&state.db)
    .await;
    match row {
        Ok(w) => {
            tracing::info!(tenant_id = %caller.tenant_id, window = %w.id, %user, "maintenance requested");
            (StatusCode::CREATED, Json(w.to_json(now))).into_response()
        }
        Err(e) => store_error(e),
    }
}

/// Loads a window of the caller's tenant and authorizes the caller to manage it.
async fn managed_window(
    state: &HealthState,
    headers: &HeaderMap,
    raw_id: &str,
) -> Result<(Caller, Uuid, Window), Response> {
    let (caller, id) = admit(state, headers, raw_id).await?;
    let window: Option<Window> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM health.maintenance_windows WHERE tenant_id = $1 AND id = $2"
    )))
    .bind(caller.tenant_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(store_error)?;
    let window = window.ok_or_else(not_found)?;
    // A window of a service the caller cannot see is not found.
    let service = service(state, &caller, headers, window.service_id).await?;
    let user = manager(state, &caller, &service).await?;
    Ok((caller, user, window))
}

pub async fn approve(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (caller, user, window) = match managed_window(&state, &headers, &raw_id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    if window.requested_by == user {
        return error(StatusCode::FORBIDDEN, "self_approval_refused");
    }
    let row: Result<Option<Window>, _> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "UPDATE health.maintenance_windows SET approved_by = $3, approved_at = now() \
         WHERE tenant_id = $1 AND id = $2 AND approved_at IS NULL AND cancelled_at IS NULL \
         AND ends_at > now() AND requested_by <> $3 RETURNING {COLUMNS}"
    )))
    .bind(caller.tenant_id)
    .bind(window.id)
    .bind(user)
    .fetch_optional(&state.db)
    .await;
    match row {
        Ok(Some(w)) => {
            tracing::info!(tenant_id = %caller.tenant_id, window = %w.id, %user, "maintenance approved");
            Json(w.to_json(Utc::now())).into_response()
        }
        Ok(None) => error(StatusCode::CONFLICT, "not_approvable"),
        Err(e) => store_error(e),
    }
}

pub async fn cancel(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
) -> Response {
    let (caller, user, window) = match managed_window(&state, &headers, &raw_id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let row: Result<Option<Window>, _> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "UPDATE health.maintenance_windows SET cancelled_by = $3, cancelled_at = now() \
         WHERE tenant_id = $1 AND id = $2 AND cancelled_at IS NULL AND ends_at > now() \
         RETURNING {COLUMNS}"
    )))
    .bind(caller.tenant_id)
    .bind(window.id)
    .bind(user)
    .fetch_optional(&state.db)
    .await;
    match row {
        Ok(Some(w)) => {
            tracing::info!(tenant_id = %caller.tenant_id, window = %w.id, %user, "maintenance cancelled");
            Json(w.to_json(Utc::now())).into_response()
        }
        Ok(None) => error(StatusCode::CONFLICT, "not_cancellable"),
        Err(e) => store_error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(approved: bool, cancelled: bool, start: i64, end: i64) -> Window {
        let now = Utc::now();
        Window {
            id: Uuid::nil(),
            service_id: Uuid::nil(),
            environment_class: "production".into(),
            starts_at: now + Duration::minutes(start),
            ends_at: now + Duration::minutes(end),
            reason: "r".into(),
            requested_by: Uuid::nil(),
            requested_at: now,
            approved_by: approved.then(Uuid::new_v4),
            approved_at: approved.then_some(now),
            cancelled_by: cancelled.then(Uuid::new_v4),
            cancelled_at: cancelled.then_some(now),
        }
    }

    #[test]
    fn only_failing_states_become_planned_maintenance() {
        let w = window(true, false, -5, 5);
        assert_eq!(apply("degraded", Some(&w)), "planned_maintenance");
        assert_eq!(apply("unavailable", Some(&w)), "planned_maintenance");
        assert_eq!(apply("normal", Some(&w)), "normal");
        assert_eq!(apply("unknown", Some(&w)), "unknown");
        assert_eq!(apply("unavailable", None), "unavailable");
    }

    #[test]
    fn status_follows_approval_cancellation_and_time() {
        let now = Utc::now();
        assert_eq!(window(false, false, -5, 5).status(now), "pending");
        assert_eq!(window(true, false, -5, 5).status(now), "active");
        assert_eq!(window(true, false, 5, 10).status(now), "approved");
        assert_eq!(window(true, false, -10, -5).status(now), "expired");
        assert_eq!(window(true, true, -5, 5).status(now), "cancelled");
    }
}
