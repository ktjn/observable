//! Catalog P1 governance: operating hours & maintenance windows (OBS-02-05),
//! unregistered observed components (OBS-02-07), and authoritative owner sync
//! with conflict detection (OBS-02-08).

use std::collections::HashSet;

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use observable_auth::authz::Classification;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::CatalogState;
use crate::api::{ApiError, ApiResult, SERVICE, WRITE, decide, non_empty, writable_service};
use crate::ports::Caller;

#[derive(Clone, Debug, FromRow, Serialize)]
pub struct MaintenanceWindowView {
    pub id: Uuid,
    pub title: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct CreateMaintenanceWindow {
    pub title: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

/// `POST /v1/catalog/services/{id}/maintenance` — records a planned maintenance window (OBS-02-05).
pub async fn create_maintenance_window(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(service_id): Path<Uuid>,
    Json(body): Json<CreateMaintenanceWindow>,
) -> ApiResult<impl IntoResponse> {
    writable_service(&state, &caller, service_id).await?;
    let title = non_empty("title", &body.title)?;
    if body.ends_at <= body.starts_at {
        return Err(ApiError::BadRequest(
            "ends_at must be after starts_at".into(),
        ));
    }
    let row = sqlx::query_as::<_, MaintenanceWindowView>(
        "INSERT INTO catalog.service_maintenance_windows \
         (tenant_id, service_id, title, starts_at, ends_at, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING id, title, starts_at, ends_at, created_by, created_at",
    )
    .bind(caller.tenant_id)
    .bind(service_id)
    .bind(title)
    .bind(body.starts_at)
    .bind(body.ends_at)
    .bind(caller.user_id)
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

pub(crate) async fn load_maintenance_windows(
    db: &PgPool,
    tenant_id: Uuid,
    service_id: Uuid,
) -> Result<Vec<MaintenanceWindowView>, sqlx::Error> {
    sqlx::query_as::<_, MaintenanceWindowView>(
        "SELECT id, title, starts_at, ends_at, created_by, created_at \
         FROM catalog.service_maintenance_windows \
         WHERE tenant_id = $1 AND service_id = $2 \
         ORDER BY starts_at, id",
    )
    .bind(tenant_id)
    .bind(service_id)
    .fetch_all(db)
    .await
}

#[derive(Deserialize)]
pub struct RecordObservedRequest {
    pub operational_name: String,
    #[serde(default = "default_component_kind")]
    pub component_kind: String,
    pub proposed_owner_kind: Option<String>,
    pub proposed_owner_id: Option<Uuid>,
}

fn default_component_kind() -> String {
    "service".into()
}

#[derive(Clone, Debug, FromRow, Serialize)]
pub struct UnregisteredObservedItem {
    pub id: Uuid,
    pub operational_name: String,
    pub component_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposed_owner_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposed_owner_id: Option<Uuid>,
    pub last_observed_at: DateTime<Utc>,
}

/// `POST /v1/catalog/observed` — records an observed service/component/provider (OBS-02-07).
pub async fn record_observed_service(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Json(body): Json<RecordObservedRequest>,
) -> ApiResult<impl IntoResponse> {
    if !decide(
        &state,
        &caller,
        WRITE,
        SERVICE,
        "observed",
        Some(Classification::Internal),
    )
    .await?
    {
        return Err(ApiError::Forbidden);
    }
    let name = non_empty("operational_name", &body.operational_name)?;
    if !matches!(
        body.component_kind.as_str(),
        "service" | "component" | "provider"
    ) {
        return Err(ApiError::BadRequest(format!(
            "unsupported component_kind '{}'",
            body.component_kind
        )));
    }
    if let Some(ref kind) = body.proposed_owner_kind
        && !matches!(kind.as_str(), "user" | "team")
    {
        return Err(ApiError::BadRequest(format!(
            "unsupported proposed_owner_kind '{kind}'"
        )));
    }
    if body.proposed_owner_kind.is_some() != body.proposed_owner_id.is_some() {
        return Err(ApiError::BadRequest(
            "proposed_owner_kind and proposed_owner_id must both be provided or both omitted"
                .into(),
        ));
    }

    let row = sqlx::query_as::<_, UnregisteredObservedItem>(
        "INSERT INTO catalog.observed_unregistered \
         (tenant_id, operational_name, component_kind, proposed_owner_kind, proposed_owner_id, last_observed_at) \
         VALUES ($1, $2, $3, $4, $5, now()) \
         ON CONFLICT (tenant_id, lower(operational_name)) DO UPDATE SET \
             component_kind = EXCLUDED.component_kind, \
             proposed_owner_kind = COALESCE(EXCLUDED.proposed_owner_kind, catalog.observed_unregistered.proposed_owner_kind), \
             proposed_owner_id = COALESCE(EXCLUDED.proposed_owner_id, catalog.observed_unregistered.proposed_owner_id), \
             last_observed_at = now() \
         RETURNING id, operational_name, component_kind, proposed_owner_kind, proposed_owner_id, last_observed_at",
    )
    .bind(caller.tenant_id)
    .bind(name)
    .bind(&body.component_kind)
    .bind(&body.proposed_owner_kind)
    .bind(body.proposed_owner_id)
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

/// Loads observed services/components that do not match any registered service or active alias.
pub(crate) async fn load_unregistered_observed(
    db: &PgPool,
    tenant_id: Uuid,
) -> Result<Vec<UnregisteredObservedItem>, sqlx::Error> {
    sqlx::query_as::<_, UnregisteredObservedItem>(
        "SELECT o.id, o.operational_name, o.component_kind, o.proposed_owner_kind, o.proposed_owner_id, o.last_observed_at \
         FROM catalog.observed_unregistered o \
         WHERE o.tenant_id = $1 \
           AND NOT EXISTS ( \
               SELECT 1 FROM catalog.services s \
               WHERE s.tenant_id = o.tenant_id AND lower(s.operational_name) = lower(o.operational_name) \
           ) \
           AND NOT EXISTS ( \
               SELECT 1 FROM catalog.service_aliases a \
               WHERE a.tenant_id = o.tenant_id AND a.state = 'active' AND lower(a.alias) = lower(o.operational_name) \
           ) \
         ORDER BY o.last_observed_at DESC, lower(o.operational_name)",
    )
    .bind(tenant_id)
    .fetch_all(db)
    .await
}

#[derive(Deserialize)]
pub struct AuthoritativeRecordInput {
    pub operational_name: String,
    #[serde(default = "default_owner_role")]
    pub role: String,
    pub authoritative_principal_kind: String,
    pub authoritative_principal_id: Uuid,
    #[serde(default)]
    pub auto_apply: bool,
}

fn default_owner_role() -> String {
    "operational_owner".into()
}

#[derive(Deserialize)]
pub struct SyncAuthoritativeRequest {
    pub records: Vec<AuthoritativeRecordInput>,
}

#[derive(Serialize)]
pub struct SyncAuthoritativeResponse {
    pub updated: usize,
    pub conflicts: usize,
    pub unchanged: usize,
}

/// `POST /v1/catalog/services/sync-authoritative` — reconciles service owners against an
/// authoritative organizational record, either updating owners or flagging conflicts (OBS-02-08).
pub async fn sync_authoritative_records(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Json(body): Json<SyncAuthoritativeRequest>,
) -> ApiResult<Json<SyncAuthoritativeResponse>> {
    if !decide(
        &state,
        &caller,
        WRITE,
        SERVICE,
        "sync-authoritative",
        Some(Classification::Internal),
    )
    .await?
    {
        return Err(ApiError::Forbidden);
    }

    let mut updated = 0usize;
    let mut conflicts = 0usize;
    let mut unchanged = 0usize;

    for rec in &body.records {
        if !matches!(rec.role.as_str(), "business_owner" | "operational_owner") {
            return Err(ApiError::BadRequest(format!(
                "unsupported owner role '{}'",
                rec.role
            )));
        }
        if !matches!(rec.authoritative_principal_kind.as_str(), "user" | "team") {
            return Err(ApiError::BadRequest(format!(
                "unsupported principal_kind '{}'",
                rec.authoritative_principal_kind
            )));
        }
        let op_name = non_empty("operational_name", &rec.operational_name)?;

        let Some(service_id): Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM catalog.services WHERE tenant_id = $1 AND lower(operational_name) = lower($2)",
        )
        .bind(caller.tenant_id)
        .bind(op_name)
        .fetch_optional(&state.db)
        .await?
        else {
            continue;
        };

        let existing: Option<(String, Uuid)> = sqlx::query_as(
            "SELECT principal_kind, principal_id FROM catalog.service_owners \
             WHERE tenant_id = $1 AND service_id = $2 AND role = $3",
        )
        .bind(caller.tenant_id)
        .bind(service_id)
        .bind(&rec.role)
        .fetch_optional(&state.db)
        .await?;

        let matches_existing = existing.as_ref().is_some_and(|(k, id)| {
            k == &rec.authoritative_principal_kind && *id == rec.authoritative_principal_id
        });

        if matches_existing {
            sqlx::query(
                "UPDATE catalog.authoritative_sync_conflicts \
                 SET resolved_at = now() \
                 WHERE tenant_id = $1 AND service_id = $2 AND role = $3 AND resolved_at IS NULL",
            )
            .bind(caller.tenant_id)
            .bind(service_id)
            .bind(&rec.role)
            .execute(&state.db)
            .await?;
            unchanged += 1;
            continue;
        }

        if rec.auto_apply || existing.is_none() {
            sqlx::query(
                "INSERT INTO catalog.service_owners (service_id, tenant_id, role, principal_kind, principal_id) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (service_id, role) DO UPDATE SET \
                     principal_kind = EXCLUDED.principal_kind, \
                     principal_id = EXCLUDED.principal_id, \
                     assigned_at = now()",
            )
            .bind(service_id)
            .bind(caller.tenant_id)
            .bind(&rec.role)
            .bind(&rec.authoritative_principal_kind)
            .bind(rec.authoritative_principal_id)
            .execute(&state.db)
            .await?;
            sqlx::query(
                "UPDATE catalog.authoritative_sync_conflicts \
                 SET resolved_at = now() \
                 WHERE tenant_id = $1 AND service_id = $2 AND role = $3 AND resolved_at IS NULL",
            )
            .bind(caller.tenant_id)
            .bind(service_id)
            .bind(&rec.role)
            .execute(&state.db)
            .await?;
            updated += 1;
        } else {
            let catalog_id = existing.map(|(_, id)| id);
            sqlx::query(
                "INSERT INTO catalog.authoritative_sync_conflicts \
                 (tenant_id, service_id, role, catalog_principal_id, authoritative_principal_id) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (tenant_id, service_id, role) WHERE resolved_at IS NULL DO UPDATE SET \
                     catalog_principal_id = EXCLUDED.catalog_principal_id, \
                     authoritative_principal_id = EXCLUDED.authoritative_principal_id, \
                     detected_at = now()",
            )
            .bind(caller.tenant_id)
            .bind(service_id)
            .bind(&rec.role)
            .bind(catalog_id)
            .bind(rec.authoritative_principal_id)
            .execute(&state.db)
            .await?;
            conflicts += 1;
        }
    }

    Ok(Json(SyncAuthoritativeResponse {
        updated,
        conflicts,
        unchanged,
    }))
}

pub(crate) async fn open_conflict_services(
    db: &PgPool,
    tenant_id: Uuid,
) -> Result<HashSet<Uuid>, sqlx::Error> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT service_id FROM catalog.authoritative_sync_conflicts \
         WHERE tenant_id = $1 AND resolved_at IS NULL",
    )
    .bind(tenant_id)
    .fetch_all(db)
    .await?;
    Ok(ids.into_iter().collect())
}
