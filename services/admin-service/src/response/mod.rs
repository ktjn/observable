//! observable-response (`exchange-operational-context`, `communicate-incident-status`).
//!
//! Runs inside the admin-service binary with its own Postgres login
//! (`observable_response`, schema `response`, migration 085). Every route sits
//! behind `require_tenant`: the tenant and the acting principal always come from
//! the authenticated caller, never from the request body. Data leaves the
//! platform only to registered, active exchange targets of the caller's tenant
//! (`targets`), through the guarded sender in `net`.

#![allow(clippy::result_large_err)] // handler helpers return the early `Response` directly.

pub mod communications;
pub mod exchange;
pub mod net;
pub mod shared_views;
pub mod targets;

use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use observable_auth::authz::HttpAccessDecider;
use observable_auth::principals::HttpPrincipalDirectory;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::middleware::auth::TenantContext;

/// State for the response routes.
#[derive(Clone)]
pub struct ResponseState {
    /// Pool for the `observable_response` login.
    pub db: PgPool,
    /// Client for observable-alerting reads (`manage-incidents`), with the caller's credentials.
    pub http: reqwest::Client,
    pub alerting_url: String,
    pub decider: HttpAccessDecider,
    pub targets: net::TargetPolicy,
    /// `lookup-principals` (auth), to validate shared-view participants (OBS-16-06).
    pub directory: HttpPrincipalDirectory,
}

pub fn router<S: Clone + Send + Sync + 'static>(state: ResponseState) -> Router<S> {
    Router::new()
        .route(
            "/v1/response/targets",
            get(targets::list_targets).post(targets::register_target),
        )
        .route(
            "/v1/response/targets/{target_id}/deactivate",
            post(targets::deactivate_target),
        )
        .route("/v1/response/handoffs", post(exchange::handoff_incident))
        .route("/v1/response/exports", post(exchange::export_records))
        .route(
            "/v1/response/investigation-handoffs",
            post(exchange::handoff_investigation),
        )
        .route(
            "/v1/response/notifications",
            post(exchange::dispatch_notification),
        )
        .route(
            "/v1/response/incidents/{incident_id}/communications",
            get(communications::list_communications).post(communications::draft_communication),
        )
        .route(
            "/v1/response/incidents/{incident_id}/communications/{communication_id}/approve",
            post(communications::approve_communication),
        )
        .route(
            "/v1/response/incidents/{incident_id}/communications/{communication_id}/release",
            post(communications::release_communication),
        )
        .route("/v1/response/shared-views", post(shared_views::create_view))
        .route(
            "/v1/response/shared-views/{view_id}",
            get(shared_views::get_view),
        )
        .route(
            "/v1/response/shared-views/{view_id}/notes",
            post(shared_views::post_note),
        )
        .with_state(state)
}

/// Error body shared by the response routes.
pub(crate) fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(json!({ "error": code }))).into_response()
}

pub(crate) fn db_error(e: sqlx::Error) -> Response {
    tracing::error!(error = %e, "response store unavailable");
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "response_store_unavailable",
    )
}

/// The acting user. API keys carry no person, so person-attributed actions need a session.
pub(crate) fn acting_user(ctx: &TenantContext) -> Result<Uuid, Response> {
    ctx.user_id
        .ok_or_else(|| error(StatusCode::FORBIDDEN, "user_session_required"))
}

pub(crate) fn require_tenant_admin(ctx: &TenantContext) -> Result<(), Response> {
    if ctx.role == "tenant_admin" {
        Ok(())
    } else {
        Err(error(StatusCode::FORBIDDEN, "tenant_admin_required"))
    }
}

/// The caller's credential headers, forwarded to providers that decide on the caller.
pub(crate) fn caller_credentials(
    headers: &HeaderMap,
) -> Vec<(reqwest::header::HeaderName, String)> {
    [
        axum::http::header::AUTHORIZATION,
        axum::http::header::COOKIE,
    ]
    .into_iter()
    .filter_map(|name| {
        headers
            .get(&name)
            .and_then(|v| v.to_str().ok())
            .map(|v| (name, v.to_owned()))
    })
    .collect()
}

/// An incident as observable-alerting returns it to the caller (`GET /v1/incidents/{id}`).
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct IncidentRecord {
    pub incident_id: Uuid,
    pub title: String,
    pub severity: String,
    pub status: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub impact_summary: Option<String>,
    #[serde(default)]
    pub affected_services: Vec<String>,
    #[serde(default)]
    pub impacted_service: Option<String>,
    #[serde(default)]
    pub runbook_url: Option<String>,
}

/// Reads the incident as the caller. Not found and not visible to the caller look the same.
pub(crate) async fn fetch_incident(
    state: &ResponseState,
    headers: &HeaderMap,
    incident_id: Uuid,
) -> Result<IncidentRecord, Response> {
    let mut req = state
        .http
        .get(format!("{}/v1/incidents/{incident_id}", state.alerting_url));
    for (name, value) in caller_credentials(headers) {
        req = req.header(name, value);
    }
    match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            resp.json::<IncidentRecord>().await.map_err(|e| {
                tracing::error!(error = %e, "incident response unreadable");
                error(StatusCode::BAD_GATEWAY, "incident_unreadable")
            })
        }
        Ok(resp)
            if matches!(
                resp.status(),
                reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::FORBIDDEN
            ) =>
        {
            Err(error(StatusCode::NOT_FOUND, "incident_not_found"))
        }
        Ok(resp) => {
            tracing::warn!(status = %resp.status(), "incident read failed");
            Err(error(
                StatusCode::SERVICE_UNAVAILABLE,
                "incidents_unavailable",
            ))
        }
        Err(e) => {
            tracing::warn!(error = %e, "incident read failed");
            Err(error(
                StatusCode::SERVICE_UNAVAILABLE,
                "incidents_unavailable",
            ))
        }
    }
}

/// The `observable_response` pool, handed to the tenant purge handler as an extension.
#[derive(Clone)]
pub struct ResponsePurge(pub PgPool);

/// Tenant purge of the `response` schema, as the `observable_response` login.
/// `(table, statement)` in delete order; each binds the tenant as `$1`.
pub const PURGE_STEPS: &[(&str, &str)] = &[
    (
        "shared_view_notes",
        "DELETE FROM response.shared_view_notes WHERE tenant_id = $1",
    ),
    (
        "shared_view_participants",
        "DELETE FROM response.shared_view_participants WHERE tenant_id = $1",
    ),
    (
        "shared_views",
        "DELETE FROM response.shared_views WHERE tenant_id = $1",
    ),
    (
        "incident_communications",
        "DELETE FROM response.incident_communications WHERE tenant_id = $1",
    ),
    (
        "notification_deliveries",
        "DELETE FROM response.notification_deliveries WHERE tenant_id = $1",
    ),
    (
        "handoff_outbox",
        "DELETE FROM response.handoff_outbox WHERE tenant_id = $1",
    ),
    (
        "exchange_targets",
        "DELETE FROM response.exchange_targets WHERE tenant_id = $1",
    ),
];

pub async fn purge_tenant(
    db: &PgPool,
    tenant_id: Uuid,
) -> Result<Vec<(&'static str, u64)>, sqlx::Error> {
    let mut tx = db.begin().await?;
    let mut deleted = Vec::with_capacity(PURGE_STEPS.len());
    for (table, sql) in PURGE_STEPS {
        let n = sqlx::query(*sql)
            .bind(tenant_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        deleted.push((*table, n));
    }
    tx.commit().await?;
    Ok(deleted)
}
