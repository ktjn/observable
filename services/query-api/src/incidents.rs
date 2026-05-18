use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize)]
pub struct IncidentItem {
    pub incident_id: Uuid,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub triggered_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub triggered_by_rule_id: Option<Uuid>,
}

#[derive(Serialize)]
pub struct IncidentListResponse {
    pub items: Vec<IncidentItem>,
}

#[derive(Serialize)]
pub struct IncidentEventItem {
    pub event_time: DateTime<Utc>,
    pub event_type: String,
    pub actor: String,
    pub message: Option<String>,
}

#[derive(Serialize)]
pub struct IncidentDetailResponse {
    pub incident_id: Uuid,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub dedup_key: String,
    pub triggered_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub triggered_by_rule_id: Option<Uuid>,
    pub runbook_url: Option<String>,
    pub rule_name: Option<String>,
    pub timeline: Vec<IncidentEventItem>,
}

#[derive(Deserialize)]
pub struct ListIncidentsQuery {
    pub status: Option<String>,
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
    resolved_at: Option<DateTime<Utc>>,
    triggered_by_rule_id: Option<Uuid>,
    runbook_url: Option<String>,
    rule_name: Option<String>,
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
                i.triggered_at, i.resolved_at, i.triggered_by_rule_id, i.runbook_url, \
                r.name AS rule_name \
         FROM incidents i \
         LEFT JOIN alert_rules r ON i.triggered_by_rule_id = r.rule_id \
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
        resolved_at: row.resolved_at,
        triggered_by_rule_id: row.triggered_by_rule_id,
        runbook_url: row.runbook_url,
        rule_name: row.rule_name,
        timeline,
    }))
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
