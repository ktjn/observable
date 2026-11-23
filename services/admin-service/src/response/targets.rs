//! Exchange targets: the only destinations data may leave the platform to.
//!
//! Registering a target is the tenant admin's approval of that destination. Hand-off,
//! notification, status-page release and investigation hand-off all look the target up by
//! id inside the caller's tenant and refuse inactive or wrong-kind targets.

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::{ResponseState, acting_user, db_error, error, require_tenant_admin};
use crate::middleware::auth::TenantContext;

pub const KINDS: &[&str] = &[
    "incident_handoff",
    "notification",
    "status_page",
    "external_consumer",
];

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExchangeTarget {
    pub target_id: Uuid,
    pub kind: String,
    pub name: String,
    pub url: String,
    pub consumer_principal_id: Option<Uuid>,
    pub registered_by: Uuid,
    pub registered_at: DateTime<Utc>,
    pub active: bool,
}

#[derive(Deserialize)]
pub struct RegisterTargetRequest {
    pub kind: String,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub consumer_principal_id: Option<Uuid>,
}

macro_rules! target_columns {
    () => {
        "target_id, kind, name, url, consumer_principal_id, registered_by, registered_at, active"
    };
}

/// An active target of `kind` in `tenant`, or `None`.
pub(crate) async fn active_target(
    db: &PgPool,
    tenant_id: Uuid,
    target_id: Uuid,
    kind: &str,
) -> Result<Option<ExchangeTarget>, sqlx::Error> {
    sqlx::query_as::<_, ExchangeTarget>(concat!(
        "SELECT ",
        target_columns!(),
        " FROM response.exchange_targets \
         WHERE tenant_id = $1 AND target_id = $2 AND kind = $3 AND active"
    ))
    .bind(tenant_id)
    .bind(target_id)
    .bind(kind)
    .fetch_optional(db)
    .await
}

pub async fn list_targets(
    State(state): State<ResponseState>,
    Extension(ctx): Extension<TenantContext>,
) -> Response {
    match sqlx::query_as::<_, ExchangeTarget>(concat!(
        "SELECT ",
        target_columns!(),
        " FROM response.exchange_targets WHERE tenant_id = $1 ORDER BY name"
    ))
    .bind(ctx.tenant_id)
    .fetch_all(&state.db)
    .await
    {
        Ok(targets) => Json(serde_json::json!({ "targets": targets })).into_response(),
        Err(e) => db_error(e),
    }
}

pub async fn register_target(
    State(state): State<ResponseState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<RegisterTargetRequest>,
) -> Response {
    if let Err(r) = require_tenant_admin(&ctx) {
        return r;
    }
    let user = match acting_user(&ctx) {
        Ok(u) => u,
        Err(r) => return r,
    };
    if !KINDS.contains(&req.kind.as_str()) {
        return error(StatusCode::BAD_REQUEST, "unknown_target_kind");
    }
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 200 {
        return error(StatusCode::BAD_REQUEST, "invalid_target_name");
    }
    if req.kind == "external_consumer" && req.consumer_principal_id.is_none() {
        return error(StatusCode::BAD_REQUEST, "consumer_principal_required");
    }
    let url = match state.targets.validate(&req.url).await {
        Ok(u) => u,
        Err(r) => return error(StatusCode::BAD_REQUEST, r.code()),
    };
    let consumer = (req.kind == "external_consumer")
        .then_some(req.consumer_principal_id)
        .flatten();
    let inserted = sqlx::query_as::<_, ExchangeTarget>(concat!(
        "INSERT INTO response.exchange_targets \
         (tenant_id, kind, name, url, consumer_principal_id, registered_by) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING ",
        target_columns!()
    ))
    .bind(ctx.tenant_id)
    .bind(&req.kind)
    .bind(name)
    .bind(url.as_str())
    .bind(consumer)
    .bind(user)
    .fetch_one(&state.db)
    .await;
    match inserted {
        Ok(t) => (StatusCode::CREATED, Json(t)).into_response(),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            error(StatusCode::CONFLICT, "target_name_taken")
        }
        Err(e) => db_error(e),
    }
}

pub async fn deactivate_target(
    State(state): State<ResponseState>,
    Extension(ctx): Extension<TenantContext>,
    Path(target_id): Path<Uuid>,
) -> Response {
    if let Err(r) = require_tenant_admin(&ctx) {
        return r;
    }
    match sqlx::query(
        "UPDATE response.exchange_targets SET active = false \
         WHERE tenant_id = $1 AND target_id = $2",
    )
    .bind(ctx.tenant_id)
    .bind(target_id)
    .execute(&state.db)
    .await
    {
        Ok(r) if r.rows_affected() == 0 => error(StatusCode::NOT_FOUND, "target_not_found"),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => db_error(e),
    }
}
