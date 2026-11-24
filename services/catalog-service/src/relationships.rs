//! Current relationships of a tenant for the operational guidance ledger (W4a,
//! docs/operational-guidance-graph.md section 6). Internal token only.
//!
//! The catalog keeps current state; alerting records each change in its append-only ledger
//! and authorizes every read against this catalog with the caller's credential, so this feed
//! carries ids and names, not access decisions.

use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::CatalogState;
use crate::api::{ApiError, ApiResult};
use crate::coverage::internal_auth;

#[derive(Deserialize)]
pub struct RelationshipsQuery {
    tenant_id: Uuid,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ServiceItem {
    id: Uuid,
    name: String,
    lifecycle: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct BindingItem {
    service_id: Uuid,
    service_name: String,
    environment: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct NamedItem {
    id: Uuid,
    name: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct JourneyMember {
    journey_id: Uuid,
    service_id: Uuid,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct OwnerItem {
    service_id: Uuid,
    role: String,
    principal_kind: String,
    principal_id: Uuid,
    assigned_at: DateTime<Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ProviderDependencyItem {
    id: Uuid,
    provider_id: Option<Uuid>,
    provider_name: String,
    service_id: Option<Uuid>,
    journey_id: Option<Uuid>,
    function: String,
    created_at: DateTime<Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct OwnerConflictItem {
    id: Uuid,
    service_id: Uuid,
    role: String,
    catalog_principal_id: Option<Uuid>,
    authoritative_principal_id: Uuid,
    detected_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct Relationships {
    tenant_id: Uuid,
    read_at: DateTime<Utc>,
    services: Vec<ServiceItem>,
    bindings: Vec<BindingItem>,
    journeys: Vec<NamedItem>,
    journey_services: Vec<JourneyMember>,
    owners: Vec<OwnerItem>,
    providers: Vec<NamedItem>,
    provider_dependencies: Vec<ProviderDependencyItem>,
    /// Open conflicts between the catalog owner and an authoritative source (OBS-39-11).
    owner_conflicts: Vec<OwnerConflictItem>,
}

/// GET /internal/catalog/relationships?tenant_id= (internal token): one consistent snapshot.
pub async fn internal_relationships(
    State(state): State<CatalogState>,
    headers: HeaderMap,
    Query(q): Query<RelationshipsQuery>,
) -> ApiResult<Json<Relationships>> {
    internal_auth(&state, &headers)?;
    let mut tx = state.db.begin().await.map_err(|_| ApiError::Internal)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(|_| ApiError::Internal)?;
    let t = q.tenant_id;
    macro_rules! rows {
        ($ty:ty, $sql:expr) => {
            sqlx::query_as::<_, $ty>($sql)
                .bind(t)
                .fetch_all(&mut *tx)
                .await
                .map_err(|_| ApiError::Internal)?
        };
    }
    let services = rows!(
        ServiceItem,
        "SELECT id, business_name AS name, lifecycle FROM catalog.services \
         WHERE tenant_id = $1 ORDER BY id"
    );
    let bindings = rows!(
        BindingItem,
        "SELECT service_id, service_name, environment FROM catalog.service_bindings \
         WHERE tenant_id = $1 ORDER BY service_id, service_name, environment"
    );
    let journeys = rows!(
        NamedItem,
        "SELECT id, name FROM catalog.journeys WHERE tenant_id = $1 ORDER BY id"
    );
    let journey_services = rows!(
        JourneyMember,
        "SELECT journey_id, service_id FROM catalog.journey_services \
         WHERE tenant_id = $1 ORDER BY journey_id, step"
    );
    let owners = rows!(
        OwnerItem,
        "SELECT service_id, role, principal_kind, principal_id, assigned_at \
         FROM catalog.service_owners WHERE tenant_id = $1 ORDER BY service_id, role"
    );
    let providers = rows!(
        NamedItem,
        "SELECT id, name FROM catalog.providers WHERE tenant_id = $1 ORDER BY id"
    );
    let provider_dependencies = rows!(
        ProviderDependencyItem,
        "SELECT id, provider_id, provider_name, service_id, journey_id, function, created_at \
         FROM catalog.provider_dependencies WHERE tenant_id = $1 ORDER BY id"
    );
    let owner_conflicts = rows!(
        OwnerConflictItem,
        "SELECT id, service_id, role, catalog_principal_id, authoritative_principal_id, detected_at \
         FROM catalog.authoritative_sync_conflicts \
         WHERE tenant_id = $1 AND resolved_at IS NULL ORDER BY id"
    );
    tx.commit().await.map_err(|_| ApiError::Internal)?;
    Ok(Json(Relationships {
        tenant_id: t,
        read_at: Utc::now(),
        services,
        bindings,
        journeys,
        journey_services,
        owners,
        providers,
        provider_dependencies,
        owner_conflicts,
    }))
}
