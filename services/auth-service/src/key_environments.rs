//! `GET /internal/key-environments?tenant_id=` (internal token): the distinct environments of a
//! tenant's active API keys. API keys are auth data; observable-control reads them here to list
//! environments that still need a class instead of querying `auth.api_keys` directly.

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
