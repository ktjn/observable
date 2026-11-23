//! `communicate-incident-status` (OBS-30-01..04).
//!
//! A communication is drafted by a signed-in user, approved by a tenant admin who is not
//! the drafter, and released by a tenant admin to one registered status-page audience.
//! Suspected scope and confirmed services are separate fields; a confirmed service must be
//! one the incident record names. Approval and release are refused while any evidence
//! reference is not `public` (a missing classification counts as restricted), and the
//! released payload never carries evidence references.

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use observable_auth::authz::Classification;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::{
    ResponseState, acting_user, db_error, error, exchange::EvidenceRef, fetch_incident,
    net::SendOutcome, require_tenant_admin, targets::active_target,
};
use crate::middleware::auth::TenantContext;

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct IncidentCommunication {
    pub communication_id: Uuid,
    pub incident_id: Uuid,
    pub audience_target_id: Uuid,
    pub suspected_scope: Vec<String>,
    pub confirmed_services: Vec<String>,
    pub summary: String,
    #[sqlx(json)]
    pub evidence_refs: Vec<EvidenceRef>,
    pub state: String,
    pub drafted_by: Uuid,
    pub drafted_at: DateTime<Utc>,
    pub approved_by: Option<Uuid>,
    pub approved_at: Option<DateTime<Utc>>,
    pub released_by: Option<Uuid>,
    pub released_at: Option<DateTime<Utc>>,
    pub delivery_status: Option<String>,
}

macro_rules! communication_columns {
    () => {
        "communication_id, incident_id, audience_target_id, suspected_scope, confirmed_services, \
         summary, evidence_refs, state, drafted_by, drafted_at, approved_by, approved_at, \
         released_by, released_at, delivery_status"
    };
}

#[derive(Deserialize)]
pub struct DraftRequest {
    pub audience_target_id: Uuid,
    #[serde(default)]
    pub suspected_scope: Vec<String>,
    #[serde(default)]
    pub confirmed_services: Vec<String>,
    pub summary: String,
    #[serde(default)]
    pub evidence_refs: Vec<EvidenceRef>,
}

/// The customer-facing update. Built from approved fields only; no evidence references.
#[derive(Serialize, Deserialize)]
pub struct PublishedUpdate {
    pub incident_id: Uuid,
    pub communication_id: Uuid,
    pub summary: String,
    pub suspected_scope: Vec<String>,
    pub confirmed_affected_services: Vec<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub released_at: DateTime<Utc>,
}

fn restricted_evidence(refs: &[EvidenceRef]) -> bool {
    refs.iter()
        .any(|r| r.classification != Some(Classification::Public))
}

fn clean_list(items: Vec<String>) -> Result<Vec<String>, Response> {
    if items.len() > 100 {
        return Err(error(StatusCode::BAD_REQUEST, "too_many_entries"));
    }
    let mut out: Vec<String> = Vec::with_capacity(items.len());
    for i in items {
        let i = i.trim().to_owned();
        if i.is_empty() || i.chars().count() > 200 {
            return Err(error(StatusCode::BAD_REQUEST, "invalid_entry"));
        }
        if !out.contains(&i) {
            out.push(i);
        }
    }
    Ok(out)
}

async fn load(
    state: &ResponseState,
    tenant_id: Uuid,
    incident_id: Uuid,
    communication_id: Uuid,
) -> Result<IncidentCommunication, Response> {
    sqlx::query_as::<_, IncidentCommunication>(concat!(
        "SELECT ",
        communication_columns!(),
        " FROM response.incident_communications \
         WHERE tenant_id = $1 AND incident_id = $2 AND communication_id = $3"
    ))
    .bind(tenant_id)
    .bind(incident_id)
    .bind(communication_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_error)?
    .ok_or_else(|| error(StatusCode::NOT_FOUND, "communication_not_found"))
}

/// `GET /v1/response/incidents/{incident_id}/communications`: the review trail (OBS-30-03).
pub async fn list_communications(
    State(state): State<ResponseState>,
    Extension(ctx): Extension<TenantContext>,
    headers: HeaderMap,
    Path(incident_id): Path<Uuid>,
) -> Response {
    if let Err(r) = fetch_incident(&state, &headers, incident_id).await {
        return r;
    }
    match sqlx::query_as::<_, IncidentCommunication>(concat!(
        "SELECT ",
        communication_columns!(),
        " FROM response.incident_communications \
         WHERE tenant_id = $1 AND incident_id = $2 ORDER BY drafted_at, communication_id"
    ))
    .bind(ctx.tenant_id)
    .bind(incident_id)
    .fetch_all(&state.db)
    .await
    {
        Ok(items) => Json(json!({ "communications": items })).into_response(),
        Err(e) => db_error(e),
    }
}

/// `POST /v1/response/incidents/{incident_id}/communications`.
pub async fn draft_communication(
    State(state): State<ResponseState>,
    Extension(ctx): Extension<TenantContext>,
    headers: HeaderMap,
    Path(incident_id): Path<Uuid>,
    Json(req): Json<DraftRequest>,
) -> Response {
    let user = match acting_user(&ctx) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let summary = req.summary.trim().to_owned();
    if summary.is_empty() || summary.chars().count() > 4000 {
        return error(StatusCode::BAD_REQUEST, "invalid_summary");
    }
    if req.evidence_refs.len() > 100 {
        return error(StatusCode::BAD_REQUEST, "too_many_entries");
    }
    let (suspected, confirmed) = match (
        clean_list(req.suspected_scope),
        clean_list(req.confirmed_services),
    ) {
        (Ok(s), Ok(c)) => (s, c),
        (Err(r), _) | (_, Err(r)) => return r,
    };
    let incident = match fetch_incident(&state, &headers, incident_id).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    let known: Vec<&String> = incident
        .affected_services
        .iter()
        .chain(incident.impacted_service.iter())
        .collect();
    if confirmed.iter().any(|c| !known.contains(&c)) {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "service_not_confirmed_by_incident",
        );
    }
    match active_target(
        &state.db,
        ctx.tenant_id,
        req.audience_target_id,
        "status_page",
    )
    .await
    {
        Ok(Some(_)) => {}
        Ok(None) => return error(StatusCode::NOT_FOUND, "audience_not_found"),
        Err(e) => return db_error(e),
    }
    let evidence = serde_json::to_value(&req.evidence_refs).unwrap_or_else(|_| json!([]));
    match sqlx::query_as::<_, IncidentCommunication>(concat!(
        "INSERT INTO response.incident_communications \
         (tenant_id, incident_id, audience_target_id, suspected_scope, confirmed_services, \
          summary, evidence_refs, drafted_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING ",
        communication_columns!()
    ))
    .bind(ctx.tenant_id)
    .bind(incident_id)
    .bind(req.audience_target_id)
    .bind(&suspected)
    .bind(&confirmed)
    .bind(&summary)
    .bind(evidence)
    .bind(user)
    .fetch_one(&state.db)
    .await
    {
        Ok(c) => (StatusCode::CREATED, Json(c)).into_response(),
        Err(e) => db_error(e),
    }
}

/// `POST .../communications/{communication_id}/approve`.
pub async fn approve_communication(
    State(state): State<ResponseState>,
    Extension(ctx): Extension<TenantContext>,
    Path((incident_id, communication_id)): Path<(Uuid, Uuid)>,
) -> Response {
    if let Err(r) = require_tenant_admin(&ctx) {
        return r;
    }
    let user = match acting_user(&ctx) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let current = match load(&state, ctx.tenant_id, incident_id, communication_id).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    if current.state != "draft" {
        return error(StatusCode::CONFLICT, "not_a_draft");
    }
    if current.drafted_by == user {
        return error(StatusCode::FORBIDDEN, "approver_must_differ_from_drafter");
    }
    if restricted_evidence(&current.evidence_refs) {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "non_public_evidence");
    }
    match sqlx::query_as::<_, IncidentCommunication>(concat!(
        "UPDATE response.incident_communications \
         SET state = 'approved', approved_by = $4, approved_at = now() \
         WHERE tenant_id = $1 AND incident_id = $2 AND communication_id = $3 AND state = 'draft' \
         RETURNING ",
        communication_columns!()
    ))
    .bind(ctx.tenant_id)
    .bind(incident_id)
    .bind(communication_id)
    .bind(user)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => Json(c).into_response(),
        Ok(None) => error(StatusCode::CONFLICT, "not_a_draft"),
        Err(e) => db_error(e),
    }
}

/// `POST .../communications/{communication_id}/release`. The row is locked while the update
/// is sent, so two concurrent releases cannot both deliver.
pub async fn release_communication(
    State(state): State<ResponseState>,
    Extension(ctx): Extension<TenantContext>,
    Path((incident_id, communication_id)): Path<(Uuid, Uuid)>,
) -> Response {
    if let Err(r) = require_tenant_admin(&ctx) {
        return r;
    }
    let user = match acting_user(&ctx) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let mut tx = match state.db.begin().await {
        Ok(tx) => tx,
        Err(e) => return db_error(e),
    };
    let current = match sqlx::query_as::<_, IncidentCommunication>(concat!(
        "SELECT ",
        communication_columns!(),
        " FROM response.incident_communications \
         WHERE tenant_id = $1 AND incident_id = $2 AND communication_id = $3 FOR UPDATE"
    ))
    .bind(ctx.tenant_id)
    .bind(incident_id)
    .bind(communication_id)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(Some(c)) => c,
        Ok(None) => return error(StatusCode::NOT_FOUND, "communication_not_found"),
        Err(e) => return db_error(e),
    };
    if current.state != "approved" {
        return error(StatusCode::CONFLICT, "not_approved");
    }
    if restricted_evidence(&current.evidence_refs) {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "non_public_evidence");
    }
    let target = match active_target(
        &state.db,
        ctx.tenant_id,
        current.audience_target_id,
        "status_page",
    )
    .await
    {
        Ok(Some(t)) => t,
        Ok(None) => return error(StatusCode::CONFLICT, "audience_inactive"),
        Err(e) => return db_error(e),
    };
    let released_at = Utc::now();
    let update = PublishedUpdate {
        incident_id,
        communication_id,
        summary: current.summary.clone(),
        suspected_scope: current.suspected_scope.clone(),
        confirmed_affected_services: current.confirmed_services.clone(),
        approved_at: current.approved_at,
        released_at,
    };
    let outcome = state.targets.post_json(&target.url, &update).await;
    let result = match &outcome {
        SendOutcome::Delivered { .. } => {
            sqlx::query_as::<_, IncidentCommunication>(concat!(
                "UPDATE response.incident_communications \
                 SET state = 'released', released_by = $4, released_at = $5, \
                     delivery_status = 'delivered' \
                 WHERE tenant_id = $1 AND incident_id = $2 AND communication_id = $3 \
                 RETURNING ",
                communication_columns!()
            ))
            .bind(ctx.tenant_id)
            .bind(incident_id)
            .bind(communication_id)
            .bind(user)
            .bind(released_at)
            .fetch_one(&mut *tx)
            .await
        }
        SendOutcome::Failed { .. } => {
            sqlx::query_as::<_, IncidentCommunication>(concat!(
                "UPDATE response.incident_communications SET delivery_status = 'failed' \
                 WHERE tenant_id = $1 AND incident_id = $2 AND communication_id = $3 \
                 RETURNING ",
                communication_columns!()
            ))
            .bind(ctx.tenant_id)
            .bind(incident_id)
            .bind(communication_id)
            .fetch_one(&mut *tx)
            .await
        }
    };
    let row = match result {
        Ok(r) => r,
        Err(e) => return db_error(e),
    };
    if let Err(e) = tx.commit().await {
        return db_error(e);
    }
    match outcome {
        SendOutcome::Delivered { .. } => {
            Json(json!({ "communication": row, "published": update })).into_response()
        }
        SendOutcome::Failed { error: e, .. } => (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": "release_delivery_failed", "detail": e, "communication": row })),
        )
            .into_response(),
    }
}
