//! `exchange-operational-context` routes: incident hand-off (OBS-16-01), exit-exercise
//! export (OBS-16-02), notification to approved enterprise channels (OBS-16-04) and
//! authorized investigation hand-off (OBS-16-10).
//!
//! The tenant and principal always come from `TenantContext`. Incident content comes from
//! observable-alerting read with the caller's own credential, never from the request body,
//! so a caller can only hand off what it can already see.

use axum::{
    Extension, Json,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use observable_auth::authz::{
    AccessDecider, Classification, DecideRequest, Decision, FilterRequest, Principal, ResourceKey,
    ResourceRef,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{
    IncidentRecord, ResponseState, acting_user, db_error, error, fetch_incident, net::SendOutcome,
    targets::active_target,
};
use crate::middleware::auth::TenantContext;

const MAX_RETRIES: u8 = 5;
const MAX_CONTEXT_BYTES: usize = 64 * 1024;
const MAX_RECORDS: usize = 1000;
const MAX_EVIDENCE: usize = 500;

/// What a hand-off target receives (OBS-16-01).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IncidentHandoffPayload {
    pub incident_id: Uuid,
    pub tenant_id: Uuid,
    pub title: String,
    pub service_name: Option<String>,
    pub affected_services: Vec<String>,
    pub severity: String,
    pub status: String,
    pub owner: Option<String>,
    pub impact_summary: Option<String>,
    pub runbook_url: Option<String>,
    #[serde(default)]
    pub context: Value,
}

impl IncidentHandoffPayload {
    fn from_incident(tenant_id: Uuid, incident: IncidentRecord, context: Value) -> Self {
        let service_name = incident
            .impacted_service
            .clone()
            .or_else(|| incident.affected_services.first().cloned());
        Self {
            incident_id: incident.incident_id,
            tenant_id,
            title: incident.title,
            service_name,
            affected_services: incident.affected_services,
            severity: incident.severity,
            status: incident.status,
            owner: incident.owner,
            impact_summary: incident.impact_summary,
            runbook_url: incident.runbook_url,
            context,
        }
    }
}

#[derive(Deserialize)]
pub struct HandoffRequest {
    pub incident_id: Uuid,
    pub target_id: Uuid,
    #[serde(default = "default_max_retries")]
    pub max_retries: u8,
    /// Responder notes carried with the incident.
    #[serde(default)]
    pub context: Value,
}

fn default_max_retries() -> u8 {
    2
}

#[derive(Serialize, Deserialize)]
pub struct HandoffResponse {
    pub handoff_id: Uuid,
    pub status: String,
    pub attempts: u8,
    pub target_id: Uuid,
    pub preserved_context: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// Sends `payload` with retries and records the outcome. A failure that cannot be recorded
/// is a 503, so a lost hand-off is never reported as handled.
#[allow(clippy::too_many_arguments)]
async fn deliver_and_record(
    state: &ResponseState,
    ctx: &TenantContext,
    kind: &str,
    incident_id: Uuid,
    target_id: Uuid,
    url: &str,
    max_retries: u8,
    payload: Value,
) -> Response {
    let max_attempts = max_retries.min(MAX_RETRIES).saturating_add(1);
    let mut attempts = 0u8;
    let mut last_error = None;
    while attempts < max_attempts {
        attempts += 1;
        match state.targets.post_json(url, &payload).await {
            SendOutcome::Delivered { .. } => {
                last_error = None;
                break;
            }
            SendOutcome::Failed { error, .. } => last_error = Some(error),
        }
    }
    let status = if last_error.is_none() {
        "delivered"
    } else {
        "failed_pending_retry"
    };
    let recorded: Result<Uuid, _> = sqlx::query_scalar(
        "INSERT INTO response.handoff_outbox \
         (tenant_id, kind, incident_id, target_id, status, attempts, last_error, payload, requested_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING handoff_id",
    )
    .bind(ctx.tenant_id)
    .bind(kind)
    .bind(incident_id)
    .bind(target_id)
    .bind(status)
    .bind(i32::from(attempts))
    .bind(last_error.as_deref())
    .bind(&payload)
    .bind(ctx.user_id)
    .fetch_one(&state.db)
    .await;
    let handoff_id = match recorded {
        Ok(id) => id,
        Err(e) => return db_error(e),
    };
    let code = if last_error.is_none() {
        StatusCode::OK
    } else {
        StatusCode::BAD_GATEWAY
    };
    (
        code,
        Json(HandoffResponse {
            handoff_id,
            status: status.to_owned(),
            attempts,
            target_id,
            preserved_context: payload,
            last_error,
        }),
    )
        .into_response()
}

fn context_too_large(v: &Value) -> bool {
    serde_json::to_vec(v).map_or(true, |b| b.len() > MAX_CONTEXT_BYTES)
}

/// `POST /v1/response/handoffs` (OBS-16-01).
pub async fn handoff_incident(
    axum::extract::State(state): axum::extract::State<ResponseState>,
    Extension(ctx): Extension<TenantContext>,
    headers: HeaderMap,
    Json(req): Json<HandoffRequest>,
) -> Response {
    if context_too_large(&req.context) {
        return error(StatusCode::PAYLOAD_TOO_LARGE, "context_too_large");
    }
    let target =
        match active_target(&state.db, ctx.tenant_id, req.target_id, "incident_handoff").await {
            Ok(Some(t)) => t,
            Ok(None) => return error(StatusCode::NOT_FOUND, "target_not_found"),
            Err(e) => return db_error(e),
        };
    let incident = match fetch_incident(&state, &headers, req.incident_id).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    let payload = IncidentHandoffPayload::from_incident(ctx.tenant_id, incident, req.context);
    let payload = serde_json::to_value(&payload).unwrap_or_else(|_| json!({}));
    deliver_and_record(
        &state,
        &ctx,
        "incident",
        req.incident_id,
        target.target_id,
        &target.url,
        req.max_retries,
        payload,
    )
    .await
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportableRecord {
    pub record_id: String,
    pub record_type: String,
    pub classification: Classification,
    pub payload: Value,
}

#[derive(Deserialize)]
pub struct ExportRequest {
    pub purpose: String,
    #[serde(default)]
    pub records: Vec<ExportableRecord>,
}

#[derive(Serialize, Deserialize)]
pub struct ExportPackage {
    pub schema_version: String,
    pub format: String,
    pub tenant_id: Uuid,
    pub exported_by: Uuid,
    pub purpose: String,
    pub exported_at: DateTime<Utc>,
    pub record_count: usize,
    pub withheld_count: usize,
    pub records: Vec<ExportableRecord>,
}

fn caller_principal(ctx: &TenantContext) -> Principal {
    match ctx.user_id {
        Some(u) => Principal::user(u),
        None => Principal::service(&ctx.role),
    }
}

/// The subset of `refs` that `principal` may read, decided per resource. Both the batch
/// filter and the per-resource decision must permit, so a stale or partial filter answer
/// cannot widen the result.
async fn permitted(
    state: &ResponseState,
    tenant_id: Uuid,
    principal: Principal,
    purpose: Option<&str>,
    refs: &[ResourceRef],
) -> Result<Vec<ResourceKey>, Response> {
    let unavailable = || error(StatusCode::SERVICE_UNAVAILABLE, "authorization_unavailable");
    let keys = state
        .decider
        .filter(&FilterRequest {
            tenant_id: Some(tenant_id),
            principal: Some(principal.clone()),
            action: Some("read".into()),
            resources: refs.to_vec(),
            purpose: purpose.map(str::to_owned),
            elevation_id: None,
        })
        .await
        .map_err(|_| unavailable())?;
    let mut out = Vec::new();
    for r in refs {
        if !keys
            .iter()
            .any(|k| k.resource_type == r.resource_type && k.id == r.id)
        {
            continue;
        }
        let decision = state
            .decider
            .decide(&DecideRequest {
                tenant_id: Some(tenant_id),
                principal: Some(principal.clone()),
                action: Some("read".into()),
                resource: Some(r.clone()),
                purpose: purpose.map(str::to_owned),
                elevation_id: None,
            })
            .await
            .map_err(|_| unavailable())?;
        if decision == Decision::Permit {
            out.push(ResourceKey {
                resource_type: r.resource_type.clone(),
                id: r.id.clone(),
            });
        }
    }
    Ok(out)
}

/// `POST /v1/response/exports` (OBS-16-02). The package holds only the records the
/// signed-in user may read; the count of withheld records is reported, their content is not.
pub async fn export_records(
    axum::extract::State(state): axum::extract::State<ResponseState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<ExportRequest>,
) -> Response {
    let user = match acting_user(&ctx) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let purpose = req.purpose.trim().to_owned();
    if purpose.is_empty() {
        return error(StatusCode::BAD_REQUEST, "purpose_required");
    }
    if req.records.len() > MAX_RECORDS {
        return error(StatusCode::PAYLOAD_TOO_LARGE, "too_many_records");
    }
    let refs: Vec<ResourceRef> = req
        .records
        .iter()
        .map(|r| ResourceRef::new(&r.record_type, &r.record_id, r.classification))
        .collect();
    let keys = match permitted(
        &state,
        ctx.tenant_id,
        Principal::user(user),
        Some(&purpose),
        &refs,
    )
    .await
    {
        Ok(k) => k,
        Err(r) => return r,
    };
    let total = req.records.len();
    let records: Vec<ExportableRecord> = req
        .records
        .into_iter()
        .filter(|r| {
            keys.iter()
                .any(|k| k.resource_type == r.record_type && k.id == r.record_id)
        })
        .collect();
    Json(ExportPackage {
        schema_version: "1.0".into(),
        format: "json".into(),
        tenant_id: ctx.tenant_id,
        exported_by: user,
        purpose,
        exported_at: Utc::now(),
        record_count: records.len(),
        withheld_count: total - records.len(),
        records,
    })
    .into_response()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub resource_type: String,
    pub id: String,
    /// Owner classification. A missing classification is treated as restricted.
    #[serde(default)]
    pub classification: Option<Classification>,
}

impl EvidenceRef {
    pub(crate) fn as_resource(&self) -> ResourceRef {
        ResourceRef {
            resource_type: self.resource_type.clone(),
            id: self.id.clone(),
            classification: self.classification,
        }
    }
}

#[derive(Deserialize)]
pub struct InvestigationHandoffRequest {
    pub incident_id: Uuid,
    pub target_id: Uuid,
    pub scope: String,
    pub purpose: String,
    #[serde(default)]
    pub evidence: Vec<EvidenceRef>,
    #[serde(default = "default_max_retries")]
    pub max_retries: u8,
}

/// `POST /v1/response/investigation-handoffs` (OBS-16-10). Evidence is kept only when both
/// the caller and the receiving consumer's principal may read it.
pub async fn handoff_investigation(
    axum::extract::State(state): axum::extract::State<ResponseState>,
    Extension(ctx): Extension<TenantContext>,
    headers: HeaderMap,
    Json(req): Json<InvestigationHandoffRequest>,
) -> Response {
    let scope = req.scope.trim().to_owned();
    let purpose = req.purpose.trim().to_owned();
    if scope.is_empty() || scope.chars().count() > 2000 {
        return error(StatusCode::BAD_REQUEST, "invalid_scope");
    }
    if purpose.is_empty() {
        return error(StatusCode::BAD_REQUEST, "purpose_required");
    }
    if req.evidence.len() > MAX_EVIDENCE {
        return error(StatusCode::PAYLOAD_TOO_LARGE, "too_much_evidence");
    }
    let target =
        match active_target(&state.db, ctx.tenant_id, req.target_id, "external_consumer").await {
            Ok(Some(t)) => t,
            Ok(None) => return error(StatusCode::NOT_FOUND, "target_not_found"),
            Err(e) => return db_error(e),
        };
    let Some(consumer) = target.consumer_principal_id else {
        return error(StatusCode::CONFLICT, "consumer_principal_missing");
    };
    let incident = match fetch_incident(&state, &headers, req.incident_id).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    let refs: Vec<ResourceRef> = req.evidence.iter().map(EvidenceRef::as_resource).collect();
    let by_caller = match permitted(
        &state,
        ctx.tenant_id,
        caller_principal(&ctx),
        Some(&purpose),
        &refs,
    )
    .await
    {
        Ok(k) => k,
        Err(r) => return r,
    };
    let by_consumer = match permitted(
        &state,
        ctx.tenant_id,
        Principal::user(consumer),
        Some(&purpose),
        &refs,
    )
    .await
    {
        Ok(k) => k,
        Err(r) => return r,
    };
    let evidence: Vec<&EvidenceRef> = req
        .evidence
        .iter()
        .filter(|e| {
            let key = |k: &ResourceKey| k.resource_type == e.resource_type && k.id == e.id;
            by_caller.iter().any(key) && by_consumer.iter().any(key)
        })
        .collect();
    let withheld = req.evidence.len() - evidence.len();
    let payload = json!({
        "incident": IncidentHandoffPayload::from_incident(ctx.tenant_id, incident, Value::Null),
        "scope": scope,
        "purpose": purpose,
        "evidence_refs": evidence,
        "withheld_evidence_count": withheld,
    });
    deliver_and_record(
        &state,
        &ctx,
        "investigation",
        req.incident_id,
        target.target_id,
        &target.url,
        req.max_retries,
        payload,
    )
    .await
}

#[derive(Deserialize)]
pub struct NotificationRequest {
    #[serde(default)]
    pub incident_id: Option<Uuid>,
    pub target_ids: Vec<Uuid>,
    pub message: String,
}

#[derive(Serialize, Deserialize)]
pub struct NotificationDelivery {
    pub delivery_id: Uuid,
    pub target_id: Uuid,
    pub status: String,
    pub http_status: Option<u16>,
}

/// `POST /v1/response/notifications` (OBS-16-04). Each requested target must be an active
/// notification target of the caller's tenant; any other id is listed as rejected and
/// nothing is sent to it.
pub async fn dispatch_notification(
    axum::extract::State(state): axum::extract::State<ResponseState>,
    Extension(ctx): Extension<TenantContext>,
    headers: HeaderMap,
    Json(req): Json<NotificationRequest>,
) -> Response {
    let message = req.message.trim().to_owned();
    if message.is_empty() || message.chars().count() > 2000 {
        return error(StatusCode::BAD_REQUEST, "invalid_message");
    }
    if req.target_ids.is_empty() || req.target_ids.len() > 50 {
        return error(StatusCode::BAD_REQUEST, "invalid_target_ids");
    }
    let incident = match req.incident_id {
        Some(id) => match fetch_incident(&state, &headers, id).await {
            Ok(i) => Some(i),
            Err(r) => return r,
        },
        None => None,
    };
    let payload = json!({
        "message": message,
        "incident": incident.as_ref().map(|i| json!({
            "incident_id": i.incident_id,
            "title": i.title,
            "severity": i.severity,
            "status": i.status,
        })),
    });
    let mut ids = req.target_ids;
    ids.sort();
    ids.dedup();
    let mut deliveries = Vec::new();
    let mut rejected = Vec::new();
    for id in ids {
        let target = match active_target(&state.db, ctx.tenant_id, id, "notification").await {
            Ok(Some(t)) => t,
            Ok(None) => {
                rejected.push(id);
                continue;
            }
            Err(e) => return db_error(e),
        };
        let (status, http_status, last_error) =
            match state.targets.post_json(&target.url, &payload).await {
                SendOutcome::Delivered { http_status } => ("delivered", Some(http_status), None),
                SendOutcome::Failed { http_status, error } => ("failed", http_status, Some(error)),
            };
        let delivery_id: Uuid = match sqlx::query_scalar(
            "INSERT INTO response.notification_deliveries \
             (tenant_id, target_id, incident_id, status, http_status, last_error, requested_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING delivery_id",
        )
        .bind(ctx.tenant_id)
        .bind(target.target_id)
        .bind(req.incident_id)
        .bind(status)
        .bind(http_status.map(i32::from))
        .bind(last_error)
        .bind(ctx.user_id)
        .fetch_one(&state.db)
        .await
        {
            Ok(id) => id,
            Err(e) => return db_error(e),
        };
        deliveries.push(NotificationDelivery {
            delivery_id,
            target_id: target.target_id,
            status: status.to_owned(),
            http_status,
        });
    }
    Json(json!({ "deliveries": deliveries, "rejected_target_ids": rejected })).into_response()
}
