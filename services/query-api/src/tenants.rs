// Tenant and environment discovery endpoints for the UI context selector.
//
// These endpoints are intentionally placed outside the tenant-auth middleware
// because they are used to populate the tenant/environment pickers before
// the operator has selected a scope.  When authentication is introduced,
// these handlers will consult the authenticated principal to filter the
// returned list (admin → all, regular user → own tenants only).
//
// GET /v1/tenants                      — list all tenants
// GET /v1/tenants/:id/environments     — list environments for one tenant
//                                        (derived from active api_keys rows)

use crate::traces::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use uuid::Uuid;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct TenantRecord {
    pub id: Uuid,
    pub name: String,
}

#[derive(Serialize)]
pub struct TenantListResponse {
    pub tenants: Vec<TenantRecord>,
}

#[derive(Serialize)]
pub struct EnvironmentRecord {
    pub environment: String,
}

#[derive(Serialize)]
pub struct EnvironmentListResponse {
    pub environments: Vec<EnvironmentRecord>,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// GET /v1/tenants — list all tenants.
/// No tenant-auth filter is applied here; the endpoint is a bootstrap resource.
/// When authentication is in place this handler will filter by the caller's
/// identity: admins see all tenants, regular users see only their own.
pub async fn list_tenants(
    State(state): State<AppState>,
) -> Result<Json<TenantListResponse>, StatusCode> {
    let rows = sqlx::query!(r#"SELECT id, name FROM tenants ORDER BY name ASC"#)
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Failed to list tenants");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let tenants = rows
        .into_iter()
        .map(|r| TenantRecord {
            id: r.id,
            name: r.name,
        })
        .collect();

    Ok(Json(TenantListResponse { tenants }))
}

/// GET /v1/tenants/:id/environments — list distinct environments issued for
/// a given tenant, derived from active (non-revoked) api_keys rows.
/// Returns environments in alphabetical order.
pub async fn list_tenant_environments(
    State(state): State<AppState>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<EnvironmentListResponse>, StatusCode> {
    let rows = sqlx::query_scalar!(
        r#"
        SELECT DISTINCT environment
        FROM api_keys
        WHERE tenant_id = $1
          AND revoked_at IS NULL
          AND environment != ''
        ORDER BY environment ASC
        "#,
        tenant_id,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Failed to list tenant environments");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let environments = rows
        .into_iter()
        .map(|e| EnvironmentRecord { environment: e })
        .collect();

    Ok(Json(EnvironmentListResponse { environments }))
}
