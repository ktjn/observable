//! Recorded execution of an incident's approved procedure (OBS-33-05,
//! `manage-operational-guidance`).
//!
//! The approved procedure of an incident is the runbook its rule carries, which only a tenant
//! admin sets. A run executes that procedure and nothing else. A manual run starts at once and
//! only signed-in users record its steps. An automated run waits for a tenant admin other than
//! the requester to approve it; once approved, automation (an API key) may record steps too.
//! Every step keeps its actor, inputs, outputs and result; the actor always comes from the
//! caller's credential, never from the body.

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::middleware::auth::TenantContext;

/// Largest accepted `inputs` or `outputs` document, serialized.
const MAX_DOC_BYTES: usize = 16 * 1024;
const RESULTS: &[&str] = &["succeeded", "failed", "skipped"];

fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(json!({ "error": code }))).into_response()
}

fn store_error(e: sqlx::Error) -> Response {
    tracing::error!(error = %e, "procedure store error");
    error(StatusCode::INTERNAL_SERVER_ERROR, "internal")
}

fn can_write(ctx: &TenantContext) -> bool {
    matches!(ctx.role.as_str(), "member" | "tenant_admin" | "admin")
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ProcedureRun {
    pub run_id: Uuid,
    pub incident_id: Uuid,
    pub procedure_ref: String,
    pub automated: bool,
    pub status: String,
    pub requested_by: Uuid,
    pub approved_by: Option<Uuid>,
    pub approved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ProcedureStep {
    pub run_id: Uuid,
    pub seq: i32,
    pub name: String,
    pub actor_kind: String,
    pub actor_id: Option<Uuid>,
    pub inputs: Value,
    pub outputs: Value,
    pub result: String,
    pub recorded_at: DateTime<Utc>,
}

const RUN_COLUMNS: &str = "run_id, incident_id, procedure_ref, automated, status, requested_by, \
     approved_by, approved_at, created_at, finished_at";

async fn load_run(
    state: &AppState,
    tenant: Uuid,
    incident: Uuid,
    run: Uuid,
) -> Result<Option<ProcedureRun>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {RUN_COLUMNS} FROM alerting.procedure_runs \
         WHERE tenant_id = $1 AND incident_id = $2 AND run_id = $3"
    )))
    .bind(tenant)
    .bind(incident)
    .bind(run)
    .fetch_optional(&state.db)
    .await
}

/// The incident's approved procedure: `Ok(None)` when the incident is not in the tenant,
/// `Ok(Some(None))` when it has no approved procedure.
async fn approved_procedure(
    state: &AppState,
    tenant: Uuid,
    incident: Uuid,
) -> Result<Option<Option<String>>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT runbook_url FROM incidents WHERE tenant_id = $1 AND incident_id = $2",
    )
    .bind(tenant)
    .bind(incident)
    .fetch_optional(&state.db)
    .await
}

#[derive(Debug, Deserialize)]
pub struct StartRun {
    pub procedure_ref: String,
    #[serde(default)]
    pub automated: bool,
}

/// POST /v1/incidents/{incident_id}/procedures: a signed-in member starts a run of the
/// incident's approved procedure.
pub async fn handle_start(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(incident_id): Path<Uuid>,
    Json(req): Json<StartRun>,
) -> Response {
    let Some(user) = ctx.user_id.filter(|_| can_write(&ctx)) else {
        return error(StatusCode::FORBIDDEN, "forbidden");
    };
    let approved = match approved_procedure(&state, ctx.tenant_id, incident_id).await {
        Ok(Some(p)) => p,
        Ok(None) => return error(StatusCode::NOT_FOUND, "not_found"),
        Err(e) => return store_error(e),
    };
    match approved {
        Some(p) if p == req.procedure_ref.trim() => {}
        Some(_) => return error(StatusCode::CONFLICT, "procedure_not_approved_for_incident"),
        None => return error(StatusCode::CONFLICT, "incident_has_no_approved_procedure"),
    }
    let status = if req.automated {
        "pending_approval"
    } else {
        "in_progress"
    };
    let run: Result<ProcedureRun, _> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "INSERT INTO alerting.procedure_runs \
         (tenant_id, incident_id, procedure_ref, automated, status, requested_by) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {RUN_COLUMNS}"
    )))
    .bind(ctx.tenant_id)
    .bind(incident_id)
    .bind(req.procedure_ref.trim())
    .bind(req.automated)
    .bind(status)
    .bind(user)
    .fetch_one(&state.db)
    .await;
    match run {
        Ok(r) => (StatusCode::CREATED, Json(r)).into_response(),
        Err(e) => store_error(e),
    }
}

/// POST /v1/incidents/{incident_id}/procedures/{run_id}/approve: a tenant admin other than
/// the requester permits an automated run.
pub async fn handle_approve(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path((incident_id, run_id)): Path<(Uuid, Uuid)>,
) -> Response {
    let Some(admin) = ctx.user_id.filter(|_| ctx.role == "tenant_admin") else {
        return error(StatusCode::FORBIDDEN, "forbidden");
    };
    let run = match load_run(&state, ctx.tenant_id, incident_id, run_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return error(StatusCode::NOT_FOUND, "not_found"),
        Err(e) => return store_error(e),
    };
    if run.status != "pending_approval" {
        return error(StatusCode::CONFLICT, "run_not_awaiting_approval");
    }
    if run.requested_by == admin {
        return error(StatusCode::FORBIDDEN, "requester_cannot_approve");
    }
    let updated: Result<Option<ProcedureRun>, _> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "UPDATE alerting.procedure_runs SET status = 'in_progress', approved_by = $4, \
         approved_at = now() \
         WHERE tenant_id = $1 AND incident_id = $2 AND run_id = $3 \
           AND status = 'pending_approval' RETURNING {RUN_COLUMNS}"
    )))
    .bind(ctx.tenant_id)
    .bind(incident_id)
    .bind(run_id)
    .bind(admin)
    .fetch_optional(&state.db)
    .await;
    match updated {
        Ok(Some(r)) => Json(r).into_response(),
        Ok(None) => error(StatusCode::CONFLICT, "run_not_awaiting_approval"),
        Err(e) => store_error(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct RecordStep {
    pub name: String,
    #[serde(default)]
    pub inputs: Value,
    #[serde(default)]
    pub outputs: Value,
    pub result: String,
}

fn doc(v: &Value) -> Option<Value> {
    let v = if v.is_null() { json!({}) } else { v.clone() };
    (v.is_object() && v.to_string().len() <= MAX_DOC_BYTES).then_some(v)
}

/// POST /v1/incidents/{incident_id}/procedures/{run_id}/steps: records one executed step.
pub async fn handle_record_step(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path((incident_id, run_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<RecordStep>,
) -> Response {
    if !can_write(&ctx) {
        return error(StatusCode::FORBIDDEN, "forbidden");
    }
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 200 || !RESULTS.contains(&req.result.as_str()) {
        return error(StatusCode::BAD_REQUEST, "invalid_step");
    }
    let (Some(inputs), Some(outputs)) = (doc(&req.inputs), doc(&req.outputs)) else {
        return error(StatusCode::BAD_REQUEST, "invalid_step");
    };
    let run = match load_run(&state, ctx.tenant_id, incident_id, run_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return error(StatusCode::NOT_FOUND, "not_found"),
        Err(e) => return store_error(e),
    };
    if run.status != "in_progress" {
        return error(StatusCode::CONFLICT, "run_not_in_progress");
    }
    // Automation acts only on an approved automated run.
    let (actor_kind, actor_id) = match ctx.user_id {
        Some(u) => ("user", Some(u)),
        None if run.automated => ("automation", None),
        None => return error(StatusCode::FORBIDDEN, "automation_not_permitted"),
    };
    let step: Result<Option<ProcedureStep>, _> = sqlx::query_as(
        "INSERT INTO alerting.procedure_steps \
         (run_id, tenant_id, seq, name, actor_kind, actor_id, inputs, outputs, result) \
         SELECT r.run_id, r.tenant_id, \
                COALESCE((SELECT max(seq) FROM alerting.procedure_steps s WHERE s.run_id = r.run_id), 0) + 1, \
                $3, $4, $5, $6, $7, $8 \
         FROM alerting.procedure_runs r \
         WHERE r.tenant_id = $1 AND r.run_id = $2 AND r.status = 'in_progress' \
         RETURNING run_id, seq, name, actor_kind, actor_id, inputs, outputs, result, recorded_at",
    )
    .bind(ctx.tenant_id)
    .bind(run_id)
    .bind(name)
    .bind(actor_kind)
    .bind(actor_id)
    .bind(&inputs)
    .bind(&outputs)
    .bind(&req.result)
    .fetch_optional(&state.db)
    .await;
    match step {
        Ok(Some(s)) => (StatusCode::CREATED, Json(s)).into_response(),
        Ok(None) => error(StatusCode::CONFLICT, "run_not_in_progress"),
        Err(sqlx::Error::Database(d)) if d.is_unique_violation() => {
            error(StatusCode::CONFLICT, "concurrent_step")
        }
        Err(e) => store_error(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct FinishRun {
    pub outcome: String,
}

/// POST /v1/incidents/{incident_id}/procedures/{run_id}/finish: `completed` or `failed`.
pub async fn handle_finish(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path((incident_id, run_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<FinishRun>,
) -> Response {
    if ctx.user_id.is_none() || !can_write(&ctx) {
        return error(StatusCode::FORBIDDEN, "forbidden");
    }
    if !matches!(req.outcome.as_str(), "completed" | "failed") {
        return error(StatusCode::BAD_REQUEST, "invalid_outcome");
    }
    let updated: Result<Option<ProcedureRun>, _> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "UPDATE alerting.procedure_runs SET status = $4, finished_at = now() \
         WHERE tenant_id = $1 AND incident_id = $2 AND run_id = $3 AND status = 'in_progress' \
         RETURNING {RUN_COLUMNS}"
    )))
    .bind(ctx.tenant_id)
    .bind(incident_id)
    .bind(run_id)
    .bind(&req.outcome)
    .fetch_optional(&state.db)
    .await;
    match updated {
        Ok(Some(r)) => Json(r).into_response(),
        Ok(None) => match load_run(&state, ctx.tenant_id, incident_id, run_id).await {
            Ok(Some(_)) => error(StatusCode::CONFLICT, "run_not_in_progress"),
            Ok(None) => error(StatusCode::NOT_FOUND, "not_found"),
            Err(e) => store_error(e),
        },
        Err(e) => store_error(e),
    }
}

/// GET /v1/incidents/{incident_id}/procedures: every run of the incident with its steps.
pub async fn handle_list(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(incident_id): Path<Uuid>,
) -> Response {
    match approved_procedure(&state, ctx.tenant_id, incident_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return error(StatusCode::NOT_FOUND, "not_found"),
        Err(e) => return store_error(e),
    }
    let runs: Vec<ProcedureRun> = match sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {RUN_COLUMNS} FROM alerting.procedure_runs \
         WHERE tenant_id = $1 AND incident_id = $2 ORDER BY created_at, run_id"
    )))
    .bind(ctx.tenant_id)
    .bind(incident_id)
    .fetch_all(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => return store_error(e),
    };
    let ids: Vec<Uuid> = runs.iter().map(|r| r.run_id).collect();
    let steps: Vec<ProcedureStep> = match sqlx::query_as(
        "SELECT run_id, seq, name, actor_kind, actor_id, inputs, outputs, result, recorded_at \
         FROM alerting.procedure_steps WHERE tenant_id = $1 AND run_id = ANY($2) \
         ORDER BY run_id, seq",
    )
    .bind(ctx.tenant_id)
    .bind(&ids)
    .fetch_all(&state.db)
    .await
    {
        Ok(s) => s,
        Err(e) => return store_error(e),
    };
    let items: Vec<Value> = runs
        .into_iter()
        .map(|r| {
            let own: Vec<&ProcedureStep> = steps.iter().filter(|s| s.run_id == r.run_id).collect();
            let mut v = serde_json::to_value(&r).unwrap_or(Value::Null);
            v["steps"] = serde_json::to_value(own).unwrap_or(Value::Null);
            v
        })
        .collect();
    Json(json!({ "items": items })).into_response()
}
