// Bootstrap endpoints — no tenant-auth required (unlike every other admin-service
// route, these sit outside the `require_tenant` middleware layer in main.rs). Used
// by the frontend to populate the global tenant+environment selector before a scope
// is chosen.
//
// GET /v1/tenants                  — list tenants
// GET /v1/tenants/:id/environments — list environments for a tenant

use crate::AdminServiceAppState;
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::Serialize;
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

/// GET /v1/tenants
/// Without a session cookie: returns all tenants (backwards-compatible for API-key callers).
/// With a session cookie: filters to only the tenants the authenticated user belongs to.
pub async fn list_tenants(
    State(state): State<AdminServiceAppState>,
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

        let rows = sqlx::query_as::<_, (Uuid, String)>(
            r#"
            SELECT t.id, t.name
            FROM tenants t
            JOIN user_tenant_roles utr ON utr.tenant_id = t.id
            WHERE utr.user_id = $1
            ORDER BY t.name ASC
            "#,
        )
        .bind(session.user_id)
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
    let rows = sqlx::query!(r#"SELECT id, name FROM tenants ORDER BY name ASC"#)
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Failed to list tenants");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(TenantListResponse {
        tenants: rows
            .into_iter()
            .map(|r| TenantRecord {
                id: r.id,
                name: r.name,
            })
            .collect(),
    }))
}

/// GET /v1/tenants/:id/environments
pub async fn list_tenant_environments(
    State(state): State<AdminServiceAppState>,
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

        let has_access = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM user_tenant_roles WHERE user_id = $1 AND tenant_id = $2",
        )
        .bind(session.user_id)
        .bind(tenant_id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        if has_access == 0 {
            tracing::warn!(user_id = %session.user_id, %tenant_id, "User attempted to access unauthorized tenant environments");
            return Err(StatusCode::FORBIDDEN);
        }
    }

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

    Ok(Json(EnvironmentListResponse {
        environments: rows
            .into_iter()
            .map(|e| EnvironmentRecord { environment: e })
            .collect(),
    }))
}
