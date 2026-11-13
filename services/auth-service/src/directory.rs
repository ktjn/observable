//! Read-only directory views of auth data for observable-control (internal token), so control
//! never queries `auth.*` tables directly:
//! - `GET /internal/key-environments?tenant_id=`: distinct environments of active API keys;
//! - `GET /internal/users/{user_id}/tenants`: a user's tenant memberships.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::authz::AuthzState;

#[derive(Deserialize)]
pub(crate) struct KeyEnvironmentsQuery {
    tenant_id: Uuid,
}

#[derive(Serialize)]
pub(crate) struct KeyEnvironments {
    environments: Vec<String>,
}

pub(crate) async fn list(
    State(state): State<AuthzState>,
    Query(q): Query<KeyEnvironmentsQuery>,
) -> Result<Json<KeyEnvironments>, StatusCode> {
    sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT environment FROM api_keys \
         WHERE tenant_id = $1 AND revoked_at IS NULL AND environment <> '' \
         ORDER BY environment",
    )
    .bind(q.tenant_id)
    .fetch_all(&state.db)
    .await
    .map(|environments| Json(KeyEnvironments { environments }))
    .map_err(|e| {
        tracing::warn!(error = %e, "api key environment store unavailable");
        StatusCode::SERVICE_UNAVAILABLE
    })
}

#[derive(Deserialize)]
pub(crate) struct UserPath {
    user_id: Uuid,
}

#[derive(Serialize)]
pub(crate) struct Membership {
    tenant_id: Uuid,
    role: String,
}

#[derive(Serialize)]
pub(crate) struct Memberships {
    tenants: Vec<Membership>,
}

/// `GET /internal/users/{user_id}/tenants` (internal token): the user's tenant memberships, so
/// observable-control can name a session's tenants without reading `auth.user_tenant_roles`.
pub(crate) async fn user_tenants(
    State(state): State<AuthzState>,
    axum::extract::Path(p): axum::extract::Path<UserPath>,
) -> Result<Json<Memberships>, StatusCode> {
    sqlx::query_as::<_, (Uuid, String)>(
        "SELECT tenant_id, role FROM user_tenant_roles WHERE user_id = $1 ORDER BY tenant_id",
    )
    .bind(p.user_id)
    .fetch_all(&state.db)
    .await
    .map(|rows| {
        Json(Memberships {
            tenants: rows
                .into_iter()
                .map(|(tenant_id, role)| Membership { tenant_id, role })
                .collect(),
        })
    })
    .map_err(|e| {
        tracing::warn!(error = %e, "membership store unavailable");
        StatusCode::SERVICE_UNAVAILABLE
    })
}
