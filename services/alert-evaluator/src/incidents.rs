use crate::AppState;
use crate::middleware::auth::TenantContext;
use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Canonical incident summary entity. Mirrors `incidents.Incident@1` in
/// `models/incidents.mdl` field-for-field.
/// `IncidentRow` (the Postgres `sqlx::FromRow` projection) and
/// `IncidentDetailResponse` (join + timeline aggregation) are NOT modeled —
/// timestamp fields stay `chrono::DateTime<Utc>` (Phase 1 backlog item 5:
/// modelable's `timestamp` emits as Rust `String`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentItem {
    pub incident_id: Uuid,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub triggered_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub triggered_by_rule_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentListResponse {
    pub items: Vec<IncidentItem>,
}

/// Canonical incident timeline event. Mirrors `incidents.IncidentEvent@1` in
/// `models/incidents.mdl` field-for-field.
/// `event_time` stays `chrono::DateTime<Utc>` (Phase 1 backlog item 5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentEventItem {
    pub event_time: DateTime<Utc>,
    pub event_type: String,
    pub actor: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentDetailResponse {
    pub incident_id: Uuid,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub dedup_key: String,
    pub triggered_at: DateTime<Utc>,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub acknowledged_by: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolved_by: Option<String>,
    pub triggered_by_rule_id: Option<Uuid>,
    pub related_deployment_id: Option<Uuid>,
    pub runbook_url: Option<String>,
    pub rule_name: Option<String>,
    pub timeline: Vec<IncidentEventItem>,
    pub impacted_service: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListIncidentsQuery {
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AcknowledgeIncidentRequest {
    pub actor: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateIncidentStatusRequest {
    pub status: String,
    pub actor: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddIncidentCommentRequest {
    pub message: String,
    pub actor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LinkDeploymentRequest {
    pub deployment_id: Uuid,
    pub actor: Option<String>,
    pub message: Option<String>,
}

#[derive(sqlx::FromRow)]
struct IncidentRow {
    incident_id: Uuid,
    title: String,
    severity: String,
    status: String,
    triggered_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    triggered_by_rule_id: Option<Uuid>,
}

#[derive(sqlx::FromRow)]
struct IncidentDetailRow {
    incident_id: Uuid,
    title: String,
    severity: String,
    status: String,
    dedup_key: String,
    triggered_at: DateTime<Utc>,
    acknowledged_at: Option<DateTime<Utc>>,
    acknowledged_by: Option<String>,
    resolved_at: Option<DateTime<Utc>>,
    resolved_by: Option<String>,
    triggered_by_rule_id: Option<Uuid>,
    related_deployment_id: Option<Uuid>,
    runbook_url: Option<String>,
    rule_name: Option<String>,
    impacted_service: Option<String>,
}

#[derive(sqlx::FromRow)]
struct IncidentEventRow {
    event_time: DateTime<Utc>,
    event_type: String,
    actor: String,
    message: Option<String>,
}

pub async fn list_incidents(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    status_filter: Option<String>,
) -> Result<Vec<IncidentItem>, sqlx::Error> {
    let rows = if let Some(status) = status_filter {
        sqlx::query_as::<_, IncidentRow>(
            "SELECT incident_id, title, severity, status, triggered_at, resolved_at, triggered_by_rule_id \
             FROM incidents \
             WHERE tenant_id = $1 AND status = $2 \
             ORDER BY triggered_at DESC",
        )
        .bind(tenant_id)
        .bind(status)
        .fetch_all(db)
        .await?
    } else {
        sqlx::query_as::<_, IncidentRow>(
            "SELECT incident_id, title, severity, status, triggered_at, resolved_at, triggered_by_rule_id \
             FROM incidents \
             WHERE tenant_id = $1 \
             ORDER BY triggered_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(db)
        .await?
    };

    Ok(rows
        .into_iter()
        .map(|row| IncidentItem {
            incident_id: row.incident_id,
            title: row.title,
            severity: row.severity,
            status: row.status,
            triggered_at: row.triggered_at,
            resolved_at: row.resolved_at,
            triggered_by_rule_id: row.triggered_by_rule_id,
        })
        .collect())
}

pub async fn get_incident(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    incident_id: Uuid,
) -> Result<Option<IncidentDetailResponse>, sqlx::Error> {
    let row: Option<IncidentDetailRow> = sqlx::query_as(
        "SELECT i.incident_id, i.title, i.severity, i.status, i.dedup_key, \
                i.triggered_at, i.acknowledged_at, i.acknowledged_by, \
                i.resolved_at, i.resolved_by, \
                i.triggered_by_rule_id, i.related_deployment_id, i.runbook_url, \
                r.name AS rule_name, \
                CASE WHEN r.alert_type = 'slo_burn_rate' \
                     THEN s.service_name \
                END AS impacted_service \
         FROM incidents i \
         LEFT JOIN alert_rules r ON i.triggered_by_rule_id = r.rule_id \
         LEFT JOIN slo_definitions s \
                ON r.alert_type = 'slo_burn_rate' \
               AND (r.condition->>'slo_id')::uuid = s.slo_id \
               AND s.tenant_id = i.tenant_id \
         WHERE i.incident_id = $1 AND i.tenant_id = $2",
    )
    .bind(incident_id)
    .bind(tenant_id)
    .fetch_optional(db)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let events: Vec<IncidentEventRow> = sqlx::query_as(
        "SELECT event_time, event_type, actor, message \
         FROM incident_events \
         WHERE incident_id = $1 \
         ORDER BY event_time ASC",
    )
    .bind(incident_id)
    .fetch_all(db)
    .await?;

    let timeline = events
        .into_iter()
        .map(|e| IncidentEventItem {
            event_time: e.event_time,
            event_type: e.event_type,
            actor: e.actor,
            message: e.message,
        })
        .collect();

    Ok(Some(IncidentDetailResponse {
        incident_id: row.incident_id,
        title: row.title,
        severity: row.severity,
        status: row.status,
        dedup_key: row.dedup_key,
        triggered_at: row.triggered_at,
        acknowledged_at: row.acknowledged_at,
        acknowledged_by: row.acknowledged_by,
        resolved_at: row.resolved_at,
        resolved_by: row.resolved_by,
        triggered_by_rule_id: row.triggered_by_rule_id,
        related_deployment_id: row.related_deployment_id,
        runbook_url: row.runbook_url,
        rule_name: row.rule_name,
        timeline,
        impacted_service: row.impacted_service,
    }))
}

pub async fn acknowledge_incident(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    incident_id: Uuid,
    actor: Option<String>,
    message: Option<String>,
) -> Result<Option<IncidentDetailResponse>, sqlx::Error> {
    let current: Option<(String, Option<DateTime<Utc>>, Option<String>)> = sqlx::query_as(
        "SELECT status, acknowledged_at, acknowledged_by FROM incidents WHERE incident_id = $1 AND tenant_id = $2"
    )
    .bind(incident_id)
    .bind(tenant_id)
    .fetch_optional(db)
    .await?;

    let Some((status, ack_at, ack_by)) = current else {
        return Ok(None);
    };

    let actor_name = actor
        .filter(|a| !a.trim().is_empty())
        .unwrap_or_else(|| "responder".to_string());

    if status != "resolved" && status != "post_mortem" {
        sqlx::query(
            "UPDATE incidents \
             SET status = 'acknowledged', \
                 acknowledged_at = COALESCE($1, NOW()), \
                 acknowledged_by = COALESCE($2, $3), \
                 updated_at = NOW() \
             WHERE incident_id = $4 AND tenant_id = $5",
        )
        .bind(ack_at)
        .bind(ack_by)
        .bind(&actor_name)
        .bind(incident_id)
        .bind(tenant_id)
        .execute(db)
        .await?;

        let event_msg = message.unwrap_or_else(|| "Incident acknowledged by responder".to_string());
        sqlx::query(
            "INSERT INTO incident_events (incident_id, event_type, actor, message) \
             VALUES ($1, 'acknowledged', $2, $3)",
        )
        .bind(incident_id)
        .bind(&actor_name)
        .bind(&event_msg)
        .execute(db)
        .await?;
    }

    get_incident(db, tenant_id, incident_id).await
}

pub async fn update_incident_status(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    incident_id: Uuid,
    new_status: &str,
    actor: Option<String>,
    message: Option<String>,
) -> Result<Option<IncidentDetailResponse>, anyhow::Error> {
    const VALID_STATUSES: &[&str] = &[
        "triggered",
        "acknowledged",
        "investigating",
        "resolved",
        "post_mortem",
    ];
    if !VALID_STATUSES.contains(&new_status) {
        anyhow::bail!("invalid incident status: {new_status}");
    }

    let exists: Option<(String,)> =
        sqlx::query_as("SELECT status FROM incidents WHERE incident_id = $1 AND tenant_id = $2")
            .bind(incident_id)
            .bind(tenant_id)
            .fetch_optional(db)
            .await?;

    let Some((old_status,)) = exists else {
        return Ok(None);
    };

    let actor_name = actor
        .filter(|a| !a.trim().is_empty())
        .unwrap_or_else(|| "responder".to_string());

    match new_status {
        "resolved" => {
            sqlx::query(
                "UPDATE incidents \
                 SET status = 'resolved', \
                     resolved_at = NOW(), \
                     resolved_by = $1, \
                     updated_at = NOW() \
                 WHERE incident_id = $2 AND tenant_id = $3",
            )
            .bind(&actor_name)
            .bind(incident_id)
            .bind(tenant_id)
            .execute(db)
            .await?;
        }
        "acknowledged" => {
            sqlx::query(
                "UPDATE incidents \
                 SET status = 'acknowledged', \
                     acknowledged_at = COALESCE(acknowledged_at, NOW()), \
                     acknowledged_by = COALESCE(acknowledged_by, $1), \
                     updated_at = NOW() \
                 WHERE incident_id = $2 AND tenant_id = $3",
            )
            .bind(&actor_name)
            .bind(incident_id)
            .bind(tenant_id)
            .execute(db)
            .await?;
        }
        _ => {
            sqlx::query(
                "UPDATE incidents \
                 SET status = $1, \
                     updated_at = NOW() \
                 WHERE incident_id = $2 AND tenant_id = $3",
            )
            .bind(new_status)
            .bind(incident_id)
            .bind(tenant_id)
            .execute(db)
            .await?;
        }
    }

    let event_type = if new_status == "acknowledged" {
        "acknowledged"
    } else if new_status == "resolved" {
        "alert_resolved"
    } else {
        "status_change"
    };

    let event_msg =
        message.unwrap_or_else(|| format!("Status transitioned from {old_status} to {new_status}"));
    sqlx::query(
        "INSERT INTO incident_events (incident_id, event_type, actor, message) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(incident_id)
    .bind(event_type)
    .bind(&actor_name)
    .bind(&event_msg)
    .execute(db)
    .await?;

    Ok(get_incident(db, tenant_id, incident_id).await?)
}

pub async fn add_incident_comment(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    incident_id: Uuid,
    message: &str,
    actor: Option<String>,
) -> Result<Option<IncidentDetailResponse>, sqlx::Error> {
    let exists: Option<(String,)> =
        sqlx::query_as("SELECT status FROM incidents WHERE incident_id = $1 AND tenant_id = $2")
            .bind(incident_id)
            .bind(tenant_id)
            .fetch_optional(db)
            .await?;

    if exists.is_none() {
        return Ok(None);
    }

    let actor_name = actor
        .filter(|a| !a.trim().is_empty())
        .unwrap_or_else(|| "responder".to_string());

    sqlx::query(
        "INSERT INTO incident_events (incident_id, event_type, actor, message) \
         VALUES ($1, 'comment', $2, $3)",
    )
    .bind(incident_id)
    .bind(&actor_name)
    .bind(message)
    .execute(db)
    .await?;

    sqlx::query(
        "UPDATE incidents SET updated_at = NOW() WHERE incident_id = $1 AND tenant_id = $2",
    )
    .bind(incident_id)
    .bind(tenant_id)
    .execute(db)
    .await?;

    get_incident(db, tenant_id, incident_id).await
}

pub async fn link_incident_deployment(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    incident_id: Uuid,
    deployment_id: Uuid,
    actor: Option<String>,
    message: Option<String>,
) -> Result<Option<IncidentDetailResponse>, sqlx::Error> {
    let exists: Option<(String,)> =
        sqlx::query_as("SELECT status FROM incidents WHERE incident_id = $1 AND tenant_id = $2")
            .bind(incident_id)
            .bind(tenant_id)
            .fetch_optional(db)
            .await?;

    if exists.is_none() {
        return Ok(None);
    }

    let actor_name = actor
        .filter(|a| !a.trim().is_empty())
        .unwrap_or_else(|| "responder".to_string());

    sqlx::query(
        "UPDATE incidents \
         SET related_deployment_id = $1, updated_at = NOW() \
         WHERE incident_id = $2 AND tenant_id = $3",
    )
    .bind(deployment_id)
    .bind(incident_id)
    .bind(tenant_id)
    .execute(db)
    .await?;

    let event_msg = message.unwrap_or_else(|| format!("Correlated deployment {deployment_id}"));
    sqlx::query(
        "INSERT INTO incident_events (incident_id, event_type, actor, message) \
         VALUES ($1, 'deployment_linked', $2, $3)",
    )
    .bind(incident_id)
    .bind(&actor_name)
    .bind(&event_msg)
    .execute(db)
    .await?;

    get_incident(db, tenant_id, incident_id).await
}

pub async fn handle_list_incidents(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(query): Query<ListIncidentsQuery>,
) -> Result<Json<IncidentListResponse>, StatusCode> {
    let items = list_incidents(&state.db, ctx.tenant_id, query.status)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to list incidents");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(IncidentListResponse { items }))
}

pub async fn handle_get_incident(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(incident_id): Path<Uuid>,
) -> Result<Json<IncidentDetailResponse>, StatusCode> {
    match get_incident(&state.db, ctx.tenant_id, incident_id).await {
        Ok(Some(detail)) => Ok(Json(detail)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to get incident");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_acknowledge_incident(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(incident_id): Path<Uuid>,
    payload: Option<Json<AcknowledgeIncidentRequest>>,
) -> Result<Json<IncidentDetailResponse>, StatusCode> {
    let (actor, message) = match payload {
        Some(Json(p)) => (p.actor, p.message),
        None => (None, None),
    };
    let resolved_actor = actor.or_else(|| ctx.user_id.map(|u| format!("user:{u}")));

    match acknowledge_incident(
        &state.db,
        ctx.tenant_id,
        incident_id,
        resolved_actor,
        message,
    )
    .await
    {
        Ok(Some(detail)) => Ok(Json(detail)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to acknowledge incident");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_update_incident_status(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(incident_id): Path<Uuid>,
    Json(payload): Json<UpdateIncidentStatusRequest>,
) -> Result<Json<IncidentDetailResponse>, StatusCode> {
    let resolved_actor = payload
        .actor
        .or_else(|| ctx.user_id.map(|u| format!("user:{u}")));
    match update_incident_status(
        &state.db,
        ctx.tenant_id,
        incident_id,
        &payload.status,
        resolved_actor,
        payload.message,
    )
    .await
    {
        Ok(Some(detail)) => Ok(Json(detail)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to update incident status");
            if e.to_string().contains("invalid incident status") {
                Err(StatusCode::UNPROCESSABLE_ENTITY)
            } else {
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn handle_resolve_incident(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(incident_id): Path<Uuid>,
    payload: Option<Json<AcknowledgeIncidentRequest>>,
) -> Result<Json<IncidentDetailResponse>, StatusCode> {
    let (actor, message) = match payload {
        Some(Json(p)) => (p.actor, p.message),
        None => (None, None),
    };
    let resolved_actor = actor.or_else(|| ctx.user_id.map(|u| format!("user:{u}")));
    match update_incident_status(
        &state.db,
        ctx.tenant_id,
        incident_id,
        "resolved",
        resolved_actor,
        message,
    )
    .await
    {
        Ok(Some(detail)) => Ok(Json(detail)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to resolve incident");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_add_incident_comment(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(incident_id): Path<Uuid>,
    Json(payload): Json<AddIncidentCommentRequest>,
) -> Result<Json<IncidentDetailResponse>, StatusCode> {
    if payload.message.trim().is_empty() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let resolved_actor = payload
        .actor
        .or_else(|| ctx.user_id.map(|u| format!("user:{u}")));
    match add_incident_comment(
        &state.db,
        ctx.tenant_id,
        incident_id,
        payload.message.trim(),
        resolved_actor,
    )
    .await
    {
        Ok(Some(detail)) => Ok(Json(detail)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to add incident comment");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_link_incident_deployment(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(incident_id): Path<Uuid>,
    Json(payload): Json<LinkDeploymentRequest>,
) -> Result<Json<IncidentDetailResponse>, StatusCode> {
    let resolved_actor = payload
        .actor
        .or_else(|| ctx.user_id.map(|u| format!("user:{u}")));
    match link_incident_deployment(
        &state.db,
        ctx.tenant_id,
        incident_id,
        payload.deployment_id,
        resolved_actor,
        payload.message,
    )
    .await
    {
        Ok(Some(detail)) => Ok(Json(detail)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to link incident deployment");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
