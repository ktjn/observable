//! Shared operational view (W5b, OBS-16-06, docs/assurance-relocations.md 4.7): teams that
//! coordinate on a cross-domain incident read the same timeline of coordination notes.
//!
//! - The creator must read the incident from `manage-incidents` with their own credential.
//! - Participants are validated with `lookup-principals`: each must be active in the tenant.
//! - Every read and write checks that the caller is a participant and that the caller can
//!   still read the incident with their own credential. Otherwise the view is 404, the same
//!   as a missing one.

use std::collections::HashSet;

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use observable_auth::principals::{PrincipalDirectory, PrincipalState};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::{IncidentRecord, ResponseState, acting_user, db_error, error, fetch_incident};
use crate::middleware::auth::TenantContext;

/// Most participants per view, the creator included.
pub const MAX_PARTICIPANTS: usize = 50;

#[derive(Debug, Deserialize)]
pub struct CreateView {
    pub incident_id: Uuid,
    #[serde(default)]
    pub participants: Vec<Uuid>,
}

/// `POST /v1/response/shared-views`.
pub async fn create_view(
    State(state): State<ResponseState>,
    Extension(ctx): Extension<TenantContext>,
    headers: HeaderMap,
    Json(req): Json<CreateView>,
) -> Response {
    let creator = match acting_user(&ctx) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let incident = match fetch_incident(&state, &headers, req.incident_id).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    let mut seen = HashSet::from([creator]);
    let mut participants = vec![creator];
    participants.extend(req.participants.iter().copied().filter(|p| seen.insert(*p)));
    if participants.len() > MAX_PARTICIPANTS {
        return error(StatusCode::BAD_REQUEST, "too_many_participants");
    }
    let resolved = match state.directory.resolve(ctx.tenant_id, &participants).await {
        Ok(r) => r,
        Err(_) => {
            return error(
                StatusCode::SERVICE_UNAVAILABLE,
                "principal_directory_unavailable",
            );
        }
    };
    let active: HashSet<Uuid> = resolved
        .iter()
        .filter(|p| p.state == PrincipalState::Active)
        .map(|p| p.user_id)
        .collect();
    let inactive: Vec<Uuid> = participants
        .iter()
        .copied()
        .filter(|p| !active.contains(p))
        .collect();
    if !inactive.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "participant_not_active", "principals": inactive})),
        )
            .into_response();
    }
    let mut tx = match state.db.begin().await {
        Ok(t) => t,
        Err(e) => return db_error(e),
    };
    let view: Result<(Uuid, DateTime<Utc>), _> = sqlx::query_as(
        "INSERT INTO response.shared_views (tenant_id, incident_id, created_by) \
         VALUES ($1, $2, $3) RETURNING view_id, created_at",
    )
    .bind(ctx.tenant_id)
    .bind(incident.incident_id)
    .bind(creator)
    .fetch_one(&mut *tx)
    .await;
    let (view_id, created_at) = match view {
        Ok(v) => v,
        Err(e) => return db_error(e),
    };
    if let Err(e) = sqlx::query(
        "INSERT INTO response.shared_view_participants (view_id, tenant_id, principal_id, added_by) \
         SELECT $1, $2, p, $3 FROM unnest($4::uuid[]) AS p",
    )
    .bind(view_id)
    .bind(ctx.tenant_id)
    .bind(creator)
    .bind(&participants)
    .execute(&mut *tx)
    .await
    {
        return db_error(e);
    }
    if let Err(e) = tx.commit().await {
        return db_error(e);
    }
    (
        StatusCode::CREATED,
        Json(json!({
            "view_id": view_id,
            "incident_id": incident.incident_id,
            "created_by": creator,
            "created_at": created_at,
            "participants": participants,
        })),
    )
        .into_response()
}

/// The view's incident, when the caller is a participant and can still read the incident.
async fn admitted(
    state: &ResponseState,
    ctx: &TenantContext,
    headers: &HeaderMap,
    view_id: Uuid,
) -> Result<(Uuid, IncidentRecord), Response> {
    let caller = acting_user(ctx)?;
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT v.incident_id FROM response.shared_views v \
         JOIN response.shared_view_participants p ON p.view_id = v.view_id \
         WHERE v.tenant_id = $1 AND v.view_id = $2 AND p.principal_id = $3",
    )
    .bind(ctx.tenant_id)
    .bind(view_id)
    .bind(caller)
    .fetch_optional(&state.db)
    .await
    .map_err(db_error)?;
    let Some((incident_id,)) = row else {
        return Err(error(StatusCode::NOT_FOUND, "not_found"));
    };
    match fetch_incident(state, headers, incident_id).await {
        Ok(i) => Ok((caller, i)),
        // Lost access to the incident: the view is not disclosed either.
        Err(r) if r.status() == StatusCode::NOT_FOUND => {
            Err(error(StatusCode::NOT_FOUND, "not_found"))
        }
        Err(r) => Err(r),
    }
}

#[derive(Debug, Deserialize)]
pub struct NoteBody {
    pub body: String,
}

/// `POST /v1/response/shared-views/{view_id}/notes`.
pub async fn post_note(
    State(state): State<ResponseState>,
    Extension(ctx): Extension<TenantContext>,
    headers: HeaderMap,
    Path(view_id): Path<Uuid>,
    Json(req): Json<NoteBody>,
) -> Response {
    let (author, _) = match admitted(&state, &ctx, &headers, view_id).await {
        Ok(a) => a,
        Err(r) => return r,
    };
    let body = req.body.trim();
    if body.is_empty() || body.chars().count() > 4000 {
        return error(StatusCode::BAD_REQUEST, "invalid_note");
    }
    let row: Result<(Uuid, DateTime<Utc>), _> = sqlx::query_as(
        "INSERT INTO response.shared_view_notes (view_id, tenant_id, author_id, body) \
         VALUES ($1, $2, $3, $4) RETURNING note_id, created_at",
    )
    .bind(view_id)
    .bind(ctx.tenant_id)
    .bind(author)
    .bind(body)
    .fetch_one(&state.db)
    .await;
    match row {
        Ok((note_id, at)) => (
            StatusCode::CREATED,
            Json(json!({"note_id": note_id, "author_id": author, "body": body, "created_at": at})),
        )
            .into_response(),
        Err(e) => db_error(e),
    }
}

/// `GET /v1/response/shared-views/{view_id}`: the incident as the caller reads it, the
/// participants and the one shared timeline of notes, oldest first.
pub async fn get_view(
    State(state): State<ResponseState>,
    Extension(ctx): Extension<TenantContext>,
    headers: HeaderMap,
    Path(view_id): Path<Uuid>,
) -> Response {
    let (_, incident) = match admitted(&state, &ctx, &headers, view_id).await {
        Ok(a) => a,
        Err(r) => return r,
    };
    let participants: Vec<(Uuid,)> = match sqlx::query_as(
        "SELECT principal_id FROM response.shared_view_participants \
         WHERE tenant_id = $1 AND view_id = $2 ORDER BY added_at, principal_id",
    )
    .bind(ctx.tenant_id)
    .bind(view_id)
    .fetch_all(&state.db)
    .await
    {
        Ok(p) => p,
        Err(e) => return db_error(e),
    };
    let notes: Vec<(Uuid, Uuid, String, DateTime<Utc>)> = match sqlx::query_as(
        "SELECT note_id, author_id, body, created_at FROM response.shared_view_notes \
         WHERE tenant_id = $1 AND view_id = $2 ORDER BY created_at, note_id",
    )
    .bind(ctx.tenant_id)
    .bind(view_id)
    .fetch_all(&state.db)
    .await
    {
        Ok(n) => n,
        Err(e) => return db_error(e),
    };
    Json(json!({
        "view_id": view_id,
        "incident": {
            "incident_id": incident.incident_id,
            "title": incident.title,
            "severity": incident.severity,
            "status": incident.status,
            "affected_services": incident.affected_services,
        },
        "participants": participants.into_iter().map(|(p,)| p).collect::<Vec<_>>(),
        "timeline": notes.into_iter().map(|(id, author, body, at)| json!({
            "note_id": id, "author_id": author, "body": body, "created_at": at,
        })).collect::<Vec<_>>(),
    }))
    .into_response()
}
