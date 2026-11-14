//! Operational context exchange: incident hand-off with retry & outbox
//! preservation (`OBS-16-01`) and authorized exit-exercise export (`OBS-16-02`).

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use chrono::{DateTime, Utc};
use observable_auth::authz::{
    AccessDecider, Classification, Decision, FilterRequest, Principal, ResourceRef,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::AdminServiceAppState;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentHandoffPayload {
    pub incident_id: String,
    pub tenant_id: Uuid,
    pub service_name: String,
    pub severity: String,
    pub status: String,
    pub summary: String,
    #[serde(default)]
    pub context: serde_json::Value,
}

#[derive(Deserialize)]
pub struct HandoffRequest {
    pub target_url: String,
    #[serde(default = "default_max_retries")]
    pub max_retries: u8,
    pub incident: IncidentHandoffPayload,
}

fn default_max_retries() -> u8 {
    2
}

#[derive(Serialize, Deserialize)]
pub struct HandoffResponse {
    pub status: String,
    pub attempts: u8,
    pub incident_id: String,
    pub preserved_context: IncidentHandoffPayload,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportableRecord {
    pub record_id: String,
    pub record_type: String,
    pub classification: Classification,
    pub payload: serde_json::Value,
}

#[derive(Deserialize)]
pub struct ExitExportRequest {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    #[serde(default)]
    pub purpose: Option<String>,
    #[serde(default = "default_format")]
    pub format: String,
    pub records: Vec<ExportableRecord>,
}

fn default_format() -> String {
    "json".to_owned()
}

#[derive(Serialize, Deserialize)]
pub struct ExitExportPackage {
    pub schema_version: String,
    pub format: String,
    pub tenant_id: Uuid,
    pub exported_at: DateTime<Utc>,
    pub record_count: usize,
    pub records: Vec<ExportableRecord>,
}

pub fn router(state: AdminServiceAppState) -> Router {
    Router::new()
        .route("/v1/context-exchange/handoff", post(handoff_handler))
        .route("/v1/context-exchange/export", post(export_handler))
        .with_state(state)
}

pub async fn handoff_handler(
    State(state): State<AdminServiceAppState>,
    Json(req): Json<HandoffRequest>,
) -> Response {
    if req.target_url.trim().is_empty()
        || req.incident.incident_id.trim().is_empty()
        || req.incident.service_name.trim().is_empty()
        || req.incident.severity.trim().is_empty()
        || req.incident.status.trim().is_empty()
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "target_url, incident_id, service_name, severity, and status are required" })),
        )
            .into_response();
    }

    let max_attempts = req.max_retries.saturating_add(1).max(1);
    let mut attempts = 0u8;
    let mut last_error: Option<String> = None;

    while attempts < max_attempts {
        attempts += 1;
        match state
            .http_client
            .post(&req.target_url)
            .json(&req.incident)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let _ = record_handoff_outbox(
                    &state.db,
                    req.incident.tenant_id,
                    &req.incident.incident_id,
                    &req.target_url,
                    "delivered",
                    attempts,
                    None,
                    &req.incident,
                )
                .await;
                return (
                    StatusCode::OK,
                    Json(HandoffResponse {
                        status: "delivered".into(),
                        attempts,
                        incident_id: req.incident.incident_id.clone(),
                        preserved_context: req.incident,
                        last_error: None,
                    }),
                )
                    .into_response();
            }
            Ok(resp) => {
                last_error = Some(format!("target returned HTTP {}", resp.status().as_u16()));
            }
            Err(err) => {
                last_error = Some(err.to_string());
            }
        }
    }

    let err_msg = last_error.unwrap_or_else(|| "handoff delivery failed".into());
    let _ = record_handoff_outbox(
        &state.db,
        req.incident.tenant_id,
        &req.incident.incident_id,
        &req.target_url,
        "failed_pending_retry",
        attempts,
        Some(&err_msg),
        &req.incident,
    )
    .await;

    (
        StatusCode::BAD_GATEWAY,
        Json(HandoffResponse {
            status: "failed_pending_retry".into(),
            attempts,
            incident_id: req.incident.incident_id.clone(),
            preserved_context: req.incident,
            last_error: Some(err_msg),
        }),
    )
        .into_response()
}

#[allow(clippy::too_many_arguments)]
async fn record_handoff_outbox(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    incident_id: &str,
    target_url: &str,
    status: &str,
    attempts: u8,
    last_error: Option<&str>,
    payload: &IncidentHandoffPayload,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO control.context_handoff_outbox \
         (tenant_id, incident_id, target_url, status, attempts, last_error, payload) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(tenant_id)
    .bind(incident_id)
    .bind(target_url)
    .bind(status)
    .bind(i32::from(attempts))
    .bind(last_error)
    .bind(serde_json::to_value(payload).unwrap_or_else(|_| json!({})))
    .execute(db)
    .await?;
    Ok(())
}

pub async fn export_handler(
    State(state): State<AdminServiceAppState>,
    Json(req): Json<ExitExportRequest>,
) -> Response {
    let internal_token = std::env::var("INTERNAL_AUTH_TOKEN")
        .unwrap_or_else(|_| "context-exchange-internal-token".into());
    let decider = observable_auth::authz::HttpAccessDecider::new(
        &state.auth_service_url,
        internal_token,
        state.http_client.clone(),
    );

    let resources: Vec<ResourceRef> = req
        .records
        .iter()
        .map(|r| ResourceRef::new(&r.record_type, &r.record_id, r.classification))
        .collect();

    let permitted_keys = match decider
        .filter(&FilterRequest {
            tenant_id: Some(req.tenant_id),
            principal: Some(Principal::user(req.user_id)),
            action: Some("read".into()),
            resources,
            purpose: req.purpose.clone(),
            elevation_id: None,
        })
        .await
    {
        Ok(keys) => keys,
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "authorization_unavailable" })),
            )
                .into_response();
        }
    };

    // Also double-check each permitted record individually for fail-closed export scope.
    let mut authorized_records = Vec::new();
    for record in req.records {
        let is_permitted = permitted_keys
            .iter()
            .any(|k| k.resource_type == record.record_type && k.id == record.record_id);
        if is_permitted {
            let check = decider
                .decide(&observable_auth::authz::DecideRequest {
                    tenant_id: Some(req.tenant_id),
                    principal: Some(Principal::user(req.user_id)),
                    action: Some("read".into()),
                    resource: Some(ResourceRef::new(
                        &record.record_type,
                        &record.record_id,
                        record.classification,
                    )),
                    purpose: req.purpose.clone(),
                    elevation_id: None,
                })
                .await;
            if matches!(check, Ok(Decision::Permit)) {
                authorized_records.push(record);
            }
        }
    }

    let package = ExitExportPackage {
        schema_version: "1.0".into(),
        format: req.format,
        tenant_id: req.tenant_id,
        exported_at: Utc::now(),
        record_count: authorized_records.len(),
        records: authorized_records,
    };

    (StatusCode::OK, Json(package)).into_response()
}
