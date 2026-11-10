// Bootstrap & Tenant management endpoints.
//
// GET    /v1/tenants                  — list tenants (bootstrap, no tenant-auth)
// GET    /v1/tenants/:id/environments — list environments for a tenant (bootstrap)
// GET    /v1/tenants/:id/config       — get tenant retention and quota config (requires admin)
// PUT    /v1/tenants/:id/config       — update tenant retention and quota config (requires admin)
// DELETE /v1/tenants/:id              — hard-purge the tenant: telemetry, control, then auth (requires admin)

use crate::AdminServiceAppState;
use crate::auth_directory::AuthDirectory;
use crate::middleware::auth::{TenantContext, require_admin};
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TenantConfigResponse {
    pub tenant_id: Uuid,
    pub retention_days: u16,
    pub ingest_quota_per_sec: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTenantConfigRequest {
    pub retention_days: u16,
    pub ingest_quota_per_sec: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurgeTenantResponse {
    pub status: String,
    pub tenant_id: Uuid,
    pub purged_postgres_tables: Vec<String>,
    pub purged_clickhouse_tables: Vec<String>,
    /// Components that purge their own schema from the request feed and have not yet
    /// acknowledged (`GET /internal/tenant-purges`).
    pub pending_components: Vec<String>,
}

/// GET /v1/tenants
/// Without a session cookie: returns all tenants (backwards-compatible for API-key callers).
/// With a session cookie: filters to only the tenants the authenticated user belongs to.
pub async fn list_tenants(
    State(state): State<AdminServiceAppState>,
    Extension(directory): Extension<AuthDirectory>,
    headers: HeaderMap,
) -> Result<Json<TenantListResponse>, StatusCode> {
    let session_token = match observable_auth::extract_session_cookie(&headers) {
        Some(cookie) => Some(cookie),
        None => observable_auth::extract_bearer_token(&headers).map_err(StatusCode::from)?,
    };

    if let Some(session_token) = session_token {
        let session = observable_auth::verify_session(
            &state.http_client,
            &state.auth_service_url,
            &session_token,
        )
        .await
        .map_err(StatusCode::from)?;

        // Memberships are auth data; tenant names are control data.
        let ids: Vec<Uuid> = directory
            .user_tenants(session.user_id)
            .await?
            .into_iter()
            .map(|m| m.tenant_id)
            .collect();
        let rows = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT id, name FROM tenants WHERE id = ANY($1) ORDER BY name ASC",
        )
        .bind(&ids)
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Failed to list user tenants");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        return Ok(Json(TenantListResponse {
            tenants: rows
                .into_iter()
                .map(|(id, name)| TenantRecord { id, name })
                .collect(),
        }));
    }

    // No session cookie — legacy path: return all tenants (API key callers).
    let rows =
        sqlx::query_as::<_, (Uuid, String)>(r#"SELECT id, name FROM tenants ORDER BY name ASC"#)
            .fetch_all(&state.db)
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Failed to list tenants");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    Ok(Json(TenantListResponse {
        tenants: rows
            .into_iter()
            .map(|(id, name)| TenantRecord { id, name })
            .collect(),
    }))
}

/// GET /v1/tenants/:id/environments
pub async fn list_tenant_environments(
    State(state): State<AdminServiceAppState>,
    Extension(directory): Extension<AuthDirectory>,
    Path(tenant_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<EnvironmentListResponse>, StatusCode> {
    // If a session is present, verify the user has access to the requested tenant.
    let session_token = match observable_auth::extract_session_cookie(&headers) {
        Some(cookie) => Some(cookie),
        None => observable_auth::extract_bearer_token(&headers).map_err(StatusCode::from)?,
    };
    if let Some(session_token) = session_token {
        let session = observable_auth::verify_session(
            &state.http_client,
            &state.auth_service_url,
            &session_token,
        )
        .await
        .map_err(StatusCode::from)?;

        let member = directory
            .user_tenants(session.user_id)
            .await?
            .iter()
            .any(|m| m.tenant_id == tenant_id);
        if !member {
            tracing::warn!(user_id = %session.user_id, %tenant_id, "User attempted to access unauthorized tenant environments");
            return Err(StatusCode::FORBIDDEN);
        }
    }

    let rows = directory.key_environments(tenant_id).await?;

    Ok(Json(EnvironmentListResponse {
        environments: rows
            .into_iter()
            .map(|e| EnvironmentRecord { environment: e })
            .collect(),
    }))
}

/// GET /v1/tenants/:id/config
/// Requires tenant_admin role. Tenant ID must match TenantContext.
pub async fn handle_get_tenant_config(
    Extension(ctx): Extension<TenantContext>,
    State(state): State<AdminServiceAppState>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<TenantConfigResponse>, StatusCode> {
    require_admin(&ctx)?;
    if ctx.tenant_id != tenant_id {
        return Err(StatusCode::FORBIDDEN);
    }

    let row = sqlx::query_as::<_, (i32, i32)>(
        r#"
        SELECT retention_days, ingest_quota_per_sec
        FROM tenants
        WHERE id = $1
        "#,
    )
    .bind(tenant_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Failed to fetch tenant config");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(TenantConfigResponse {
        tenant_id,
        retention_days: row.0 as u16,
        ingest_quota_per_sec: row.1 as u32,
    }))
}

/// PUT /v1/tenants/:id/config
/// Updates retention_days and ingest_quota_per_sec. Requires tenant_admin.
pub async fn handle_update_tenant_config(
    Extension(ctx): Extension<TenantContext>,
    State(state): State<AdminServiceAppState>,
    Path(tenant_id): Path<Uuid>,
    Json(payload): Json<UpdateTenantConfigRequest>,
) -> Result<Json<TenantConfigResponse>, StatusCode> {
    require_admin(&ctx)?;
    if ctx.tenant_id != tenant_id {
        return Err(StatusCode::FORBIDDEN);
    }

    let row = sqlx::query_as::<_, (i32, i32)>(
        r#"
        UPDATE tenants
        SET retention_days = $1,
            ingest_quota_per_sec = $2,
            updated_at = NOW()
        WHERE id = $3
        RETURNING retention_days, ingest_quota_per_sec
        "#,
    )
    .bind(payload.retention_days as i32)
    .bind(payload.ingest_quota_per_sec as i32)
    .bind(tenant_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Failed to update tenant config");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(TenantConfigResponse {
        tenant_id,
        retention_days: row.0 as u16,
        ingest_quota_per_sec: row.1 as u32,
    }))
}

/// DELETE /v1/tenants/:id
/// Hard purges the tenant: ClickHouse telemetry (asynchronous mutations), the `control` schema,
/// then auth (sessions, memberships, keys, worker credentials, tenant policy, plus the
/// `tenant_purge` audit record). The request is recorded first; catalog, health and alerting
/// pull it and purge their own schemas (listed in `pending_components` until they ack). Auth goes last so the caller's session survives any earlier
/// failure; every step is idempotent, so a 503 is retried with the same request.
pub async fn handle_purge_tenant(
    Extension(ctx): Extension<TenantContext>,
    Extension(directory): Extension<AuthDirectory>,
    State(state): State<AdminServiceAppState>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<PurgeTenantResponse>, StatusCode> {
    require_admin(&ctx)?;
    if ctx.tenant_id != tenant_id {
        return Err(StatusCode::FORBIDDEN);
    }
    let unavailable = |step: &'static str| {
        move |e: &dyn std::fmt::Display| {
            tracing::error!(error = %e, step, %tenant_id, "tenant purge step failed");
            StatusCode::SERVICE_UNAVAILABLE
        }
    };

    let actor = ctx
        .user_id
        .map(|u| u.to_string())
        .unwrap_or_else(|| "admin_api".to_string());
    crate::tenant_purge::record_request(&state.db, tenant_id, &actor)
        .await
        .map_err(|e| unavailable("request")(&e))?;
    crate::tenant_purge::purge_telemetry(&state.ch, tenant_id)
        .await
        .map_err(|e| unavailable("clickhouse")(&e))?;
    let control = crate::tenant_purge::purge_tenant(&state.db, tenant_id)
        .await
        .map_err(|e| unavailable("control")(&e))?;
    let auth = directory.purge_tenant(tenant_id, &actor).await?;
    let pending = crate::tenant_purge::pending_components(&state.db, tenant_id)
        .await
        .map_err(|e| unavailable("pending")(&e))?;

    Ok(Json(PurgeTenantResponse {
        status: "purged".to_string(),
        tenant_id,
        purged_postgres_tables: control
            .into_iter()
            .map(|(t, _)| format!("control.{t}"))
            .chain(auth.into_iter().map(|t| format!("auth.{t}")))
            .collect(),
        purged_clickhouse_tables: crate::tenant_purge::CLICKHOUSE_TABLES
            .iter()
            .map(|t| t.to_string())
            .collect(),
        pending_components: pending,
    }))
}
