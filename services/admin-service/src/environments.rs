//! Environment classes (phase 1 S4, OBS-01-03): a tenant admin classifies each environment as
//! `production` or `non_production`. Environments seen on API keys but never classified are
//! `unclassified` and belong to neither context, so they can never leak into production views
//! or reports by default. The catalog reads classes through `/internal/environment-classes`.
//!
//! API keys are auth data (`auth.api_keys`, closed to the control role), so the environments
//! seen on keys come from auth-service's `/internal/key-environments`. When auth cannot
//! answer, listing fails closed with 503 instead of hiding unclassified environments.

use std::collections::BTreeMap;

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::AdminServiceAppState;
use crate::auth_directory::AuthDirectory;
use crate::middleware::auth::{TenantContext, require_admin};

pub const CLASSES: &[&str] = &["production", "non_production"];
pub const UNCLASSIFIED: &str = "unclassified";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentClass {
    pub environment: String,
    /// `production`, `non_production` or `unclassified`.
    pub class: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EnvironmentClassList {
    pub environments: Vec<EnvironmentClass>,
}

/// Classified environments plus every active API-key environment, sorted by name.
pub async fn list_classes(
    db: &PgPool,
    keys: &AuthDirectory,
    tenant_id: Uuid,
) -> Result<Vec<EnvironmentClass>, StatusCode> {
    let classified: Vec<(String, String)> =
        sqlx::query_as("SELECT environment, class FROM environment_classes WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_all(db)
            .await
            .map_err(db_error)?;
    let seen = keys.key_environments(tenant_id).await?;
    let mut all: BTreeMap<String, String> = seen
        .into_iter()
        .map(|e| (e, UNCLASSIFIED.to_owned()))
        .collect();
    all.extend(classified);
    Ok(all
        .into_iter()
        .map(|(environment, class)| EnvironmentClass { environment, class })
        .collect())
}

fn db_error(e: sqlx::Error) -> StatusCode {
    tracing::error!(error = ?e, "environment class store error");
    StatusCode::INTERNAL_SERVER_ERROR
}

fn valid_environment(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// GET /v1/tenants/{id}/environment-classes (any role of the tenant).
pub async fn handle_list_environment_classes(
    Extension(ctx): Extension<TenantContext>,
    Extension(keys): Extension<AuthDirectory>,
    State(state): State<AdminServiceAppState>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<EnvironmentClassList>, StatusCode> {
    if ctx.tenant_id != tenant_id {
        return Err(StatusCode::FORBIDDEN);
    }
    let environments = list_classes(&state.db, &keys, tenant_id).await?;
    Ok(Json(EnvironmentClassList { environments }))
}

#[derive(Deserialize)]
pub struct SetClass {
    pub class: String,
}

/// PUT /v1/tenants/{id}/environment-classes/{environment} (tenant_admin).
pub async fn handle_set_environment_class(
    Extension(ctx): Extension<TenantContext>,
    State(state): State<AdminServiceAppState>,
    Path((tenant_id, environment)): Path<(Uuid, String)>,
    Json(body): Json<SetClass>,
) -> Result<Json<EnvironmentClass>, StatusCode> {
    require_admin(&ctx)?;
    if ctx.tenant_id != tenant_id {
        return Err(StatusCode::FORBIDDEN);
    }
    if !valid_environment(&environment) || !CLASSES.contains(&body.class.as_str()) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let updated_by = match ctx.user_id {
        Some(u) => format!("user:{u}"),
        None => format!("api_key:{}", ctx.role),
    };
    sqlx::query(
        "INSERT INTO environment_classes (tenant_id, environment, class, updated_by) \
         VALUES ($1, $2, $3, $4) ON CONFLICT (tenant_id, environment) \
         DO UPDATE SET class = EXCLUDED.class, updated_by = EXCLUDED.updated_by, updated_at = now()",
    )
    .bind(tenant_id)
    .bind(&environment)
    .bind(&body.class)
    .bind(updated_by)
    .execute(&state.db)
    .await
    .map_err(db_error)?;
    Ok(Json(EnvironmentClass {
        environment,
        class: body.class,
    }))
}

/// DELETE /v1/tenants/{id}/environment-classes/{environment} (tenant_admin): back to
/// unclassified.
pub async fn handle_clear_environment_class(
    Extension(ctx): Extension<TenantContext>,
    State(state): State<AdminServiceAppState>,
    Path((tenant_id, environment)): Path<(Uuid, String)>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;
    if ctx.tenant_id != tenant_id {
        return Err(StatusCode::FORBIDDEN);
    }
    sqlx::query("DELETE FROM environment_classes WHERE tenant_id = $1 AND environment = $2")
        .bind(tenant_id)
        .bind(&environment)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct InternalParams {
    pub tenant_id: Uuid,
}

/// GET /internal/environment-classes?tenant_id= (internal token; catalog consumer).
pub async fn handle_internal_environment_classes(
    Extension(keys): Extension<AuthDirectory>,
    State(state): State<AdminServiceAppState>,
    Query(params): Query<InternalParams>,
) -> Result<Json<EnvironmentClassList>, StatusCode> {
    let environments = list_classes(&state.db, &keys, params.tenant_id).await?;
    Ok(Json(EnvironmentClassList { environments }))
}
