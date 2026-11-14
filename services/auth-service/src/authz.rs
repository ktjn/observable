//! `decide-access` (`POST /internal/authz/decide`, `POST /internal/authz/filter`) and
//! `lookup-principals` (`POST /internal/principals/resolve`) over the PostgreSQL policy store
//! (migration 047). Decision semantics live in `observable_auth::authz::evaluate`, shared with
//! the provider-owned mock.
//!
//! Wire responses:
//! - permit `200 {"decision":"permit"}`; every deny `403 {"decision":"deny"}` whatever the
//!   reason (existence hiding); purpose missing `422 {"decision":"deny","error":"purpose_missing"}`;
//! - store or decision-log failure `503 {"error":"decision_unavailable"}` (fail closed);
//! - a malformed or incomplete body is a deny, never a validation error.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::post,
};
use chrono::Utc;
use observable_auth::authz::{
    Classification, DecideRequest, DecisionRecord, DenyReason, Elevation, FilterRequest,
    FilterResponse, Grant, Outcome, PolicySnapshot, PrincipalKind, PurposeRule, ResourceKey,
    Subject, evaluate, filter_records, record_for, subject_of,
};
use observable_auth::principals::{
    MAX_RESOLVE, PrincipalState, ResolveRequest, ResolveResponse, ResolvedPrincipal, dedup,
};
use serde_json::json;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AuthzState {
    pub db: PgPool,
    pub internal_token: Arc<String>,
}

pub fn router(state: AuthzState) -> Router {
    Router::new()
        .route("/internal/authz/decide", post(decide_handler))
        .route("/internal/authz/filter", post(filter_handler))
        .route("/internal/principals/resolve", post(resolve_handler))
        .route(
            "/internal/health-worker/validate",
            post(crate::health_worker::validate),
        )
        .route(
            "/internal/key-environments",
            axum::routing::get(crate::directory::list),
        )
        .route(
            "/internal/users/{user_id}/tenants",
            axum::routing::get(crate::directory::user_tenants),
        )
        .route(
            "/internal/tenants/{tenant_id}/purge",
            post(crate::tenant_purge::purge_handler),
        )
        .route(
            "/internal/governance/retention-policies",
            axum::routing::put(crate::governance::upsert_retention_policy),
        )
        .route(
            "/internal/governance/retention-policies/{tenant_id}",
            axum::routing::get(crate::governance::review_retention_policies),
        )
        .route(
            "/internal/governance/retention/evaluate",
            post(crate::governance::evaluate_retention_handler),
        )
        .route(
            "/internal/governance/legal-holds",
            post(crate::governance::set_legal_hold),
        )
        .route(
            "/internal/governance/audit",
            post(crate::governance::record_governance_change_handler)
                .get(crate::governance::list_governance_changes_handler),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_internal_token,
        ))
        .with_state(state)
}

async fn require_internal_token(
    State(state): State<AuthzState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    observable_auth::verify_internal_token(req.headers(), &state.internal_token)
        .map_err(StatusCode::from)?;
    Ok(next.run(req).await)
}

fn deny() -> Response {
    (StatusCode::FORBIDDEN, Json(json!({"decision": "deny"}))).into_response()
}

fn unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({"error": "decision_unavailable"})),
    )
        .into_response()
}

fn respond(outcome: Outcome) -> Response {
    match outcome {
        Outcome::Permit => (StatusCode::OK, Json(json!({"decision": "permit"}))).into_response(),
        Outcome::PurposeMissing => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"decision": "deny", "error": "purpose_missing"})),
        )
            .into_response(),
        Outcome::Deny(_) => deny(),
    }
}

fn classification(value: &str) -> Result<Classification, sqlx::Error> {
    Classification::parse(value)
        .ok_or_else(|| sqlx::Error::Decode(format!("unknown classification '{value}'").into()))
}

/// Loads the policy for one principal in one tenant. Any store error is returned so the
/// caller fails closed.
pub async fn load_snapshot(
    db: &PgPool,
    subject: &Subject<'_>,
) -> Result<PolicySnapshot, sqlx::Error> {
    let role = match subject.principal.kind {
        PrincipalKind::User => {
            let id = subject.principal.id.expect("validated by subject_of");
            sqlx::query_scalar::<_, String>(
                "SELECT role FROM auth.user_tenant_roles WHERE user_id = $1 AND tenant_id = $2",
            )
            .bind(id)
            .bind(subject.tenant_id)
            .fetch_optional(db)
            .await?
        }
        PrincipalKind::Service => subject.principal.role.clone(),
    };

    let grants = sqlx::query(
        "SELECT role, resource_type, action, max_classification FROM auth.authz_role_grants \
         WHERE tenant_id IS NULL OR tenant_id = $1",
    )
    .bind(subject.tenant_id)
    .fetch_all(db)
    .await?
    .into_iter()
    .map(|r| {
        Ok(Grant {
            role: r.try_get("role")?,
            resource_type: r.try_get("resource_type")?,
            action: r.try_get("action")?,
            max_classification: classification(r.try_get("max_classification")?)?,
        })
    })
    .collect::<Result<Vec<_>, sqlx::Error>>()?;

    let purpose_rules = sqlx::query(
        "SELECT resource_type, action, min_classification FROM auth.authz_purpose_rules \
         WHERE tenant_id IS NULL OR tenant_id = $1",
    )
    .bind(subject.tenant_id)
    .fetch_all(db)
    .await?
    .into_iter()
    .map(|r| {
        Ok(PurposeRule {
            resource_type: r.try_get("resource_type")?,
            action: r.try_get("action")?,
            min_classification: classification(r.try_get("min_classification")?)?,
        })
    })
    .collect::<Result<Vec<_>, sqlx::Error>>()?;

    let elevation = match subject.elevation_id {
        None => None,
        Some(id) => sqlx::query(
            "SELECT id, tenant_id, user_id, resource_type, max_classification, expires_at, \
             revoked_at IS NOT NULL AS revoked FROM auth.authz_elevations WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(db)
        .await?
        .map(|r| {
            Ok::<_, sqlx::Error>(Elevation {
                id: r.try_get("id")?,
                tenant_id: r.try_get("tenant_id")?,
                user_id: r.try_get("user_id")?,
                resource_type: r.try_get("resource_type")?,
                max_classification: classification(r.try_get("max_classification")?)?,
                expires_at: r.try_get("expires_at")?,
                revoked: r.try_get("revoked")?,
            })
        })
        .transpose()?,
    };

    Ok(PolicySnapshot {
        role,
        grants,
        purpose_rules,
        elevation,
    })
}

pub async fn write_records(db: &PgPool, records: &[DecisionRecord]) -> Result<(), sqlx::Error> {
    if records.is_empty() {
        return Ok(());
    }
    let mut tx = db.begin().await?;
    for r in records {
        sqlx::query(
            "INSERT INTO auth.access_decision_log (tenant_id, principal_kind, principal_id, \
             resource_type, resource_id, action, purpose, elevation_id, decision, reason) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(r.tenant_id)
        .bind(r.principal_kind.map(|k| match k {
            PrincipalKind::User => "user",
            PrincipalKind::Service => "service",
        }))
        .bind(r.principal_id)
        .bind(&r.resource_type)
        .bind(&r.resource_id)
        .bind(&r.action)
        .bind(&r.purpose)
        .bind(r.elevation_id)
        .bind(&r.decision)
        .bind(&r.reason)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

pub async fn decide_handler(State(state): State<AuthzState>, body: Bytes) -> Response {
    let req: DecideRequest = serde_json::from_slice(&body).unwrap_or_default();
    let subject = subject_of(
        req.tenant_id,
        req.principal.as_ref(),
        req.action.as_deref(),
        req.purpose.as_deref(),
        req.elevation_id,
    );
    let outcome = match (subject, req.resource.as_ref()) {
        (Some(subject), Some(resource)) => match load_snapshot(&state.db, &subject).await {
            Ok(snapshot) => evaluate(&snapshot, &subject, resource, Utc::now()),
            Err(e) => {
                tracing::warn!(error = %e, "decide-access policy store unavailable");
                return unavailable();
            }
        },
        _ => Outcome::Deny(DenyReason::IncompleteRequest),
    };
    let record = record_for(
        req.tenant_id,
        req.principal.as_ref(),
        req.action.as_deref(),
        req.resource.as_ref(),
        req.purpose.as_deref(),
        req.elevation_id,
        outcome,
    );
    if let Err(e) = write_records(&state.db, std::slice::from_ref(&record)).await {
        tracing::warn!(error = %e, "decide-access decision log unavailable");
        return unavailable();
    }
    respond(outcome)
}

pub async fn filter_handler(State(state): State<AuthzState>, body: Bytes) -> Response {
    let req: FilterRequest = serde_json::from_slice(&body).unwrap_or_default();
    let Some(subject) = subject_of(
        req.tenant_id,
        req.principal.as_ref(),
        req.action.as_deref(),
        req.purpose.as_deref(),
        req.elevation_id,
    ) else {
        return Json(FilterResponse { permitted: vec![] }).into_response();
    };
    let snapshot = match load_snapshot(&state.db, &subject).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "decide-access policy store unavailable");
            return unavailable();
        }
    };
    let now = Utc::now();
    let mut permitted = Vec::new();
    let mut records = Vec::new();
    for resource in &req.resources {
        let outcome = evaluate(&snapshot, &subject, resource, now);
        if filter_records(req.purpose.as_deref(), req.elevation_id, outcome) {
            records.push(record_for(
                req.tenant_id,
                req.principal.as_ref(),
                req.action.as_deref(),
                Some(resource),
                req.purpose.as_deref(),
                req.elevation_id,
                outcome,
            ));
        }
        if outcome == Outcome::Permit {
            permitted.push(ResourceKey {
                resource_type: resource.resource_type.clone(),
                id: resource.id.clone(),
            });
        }
    }
    if let Err(e) = write_records(&state.db, &records).await {
        tracing::warn!(error = %e, "decide-access decision log unavailable");
        return unavailable();
    }
    Json(FilterResponse { permitted }).into_response()
}

pub async fn resolve_principals(
    db: &PgPool,
    tenant_id: Uuid,
    user_ids: &[Uuid],
) -> Result<Vec<ResolvedPrincipal>, sqlx::Error> {
    let ids = dedup(user_ids);
    let rows = sqlx::query(
        "SELECT u.id, EXISTS (SELECT 1 FROM auth.user_tenant_roles r \
         WHERE r.user_id = u.id AND r.tenant_id = $1) AS member \
         FROM auth.users u WHERE u.id = ANY($2)",
    )
    .bind(tenant_id)
    .bind(&ids)
    .fetch_all(db)
    .await?;
    let mut known = std::collections::HashMap::new();
    for r in rows {
        known.insert(r.try_get::<Uuid, _>("id")?, r.try_get::<bool, _>("member")?);
    }
    Ok(ids
        .into_iter()
        .map(|user_id| ResolvedPrincipal {
            user_id,
            state: match known.get(&user_id) {
                Some(true) => PrincipalState::Active,
                Some(false) => PrincipalState::Departed,
                None => PrincipalState::Unknown,
            },
        })
        .collect())
}

pub async fn resolve_handler(State(state): State<AuthzState>, body: Bytes) -> Response {
    let Ok(req) = serde_json::from_slice::<ResolveRequest>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if req.user_ids.len() > MAX_RESOLVE {
        return StatusCode::BAD_REQUEST.into_response();
    }
    match resolve_principals(&state.db, req.tenant_id, &req.user_ids).await {
        Ok(principals) => Json(ResolveResponse { principals }).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "lookup-principals directory unavailable");
            unavailable()
        }
    }
}

/// Resolves the shared internal-service token. Fails closed outside dev mode: an empty token
/// would accept any caller that sends an empty header.
pub fn resolve_internal_token(env_value: Option<String>, dev_mode: bool) -> anyhow::Result<String> {
    match env_value {
        Some(token) if !token.trim().is_empty() => Ok(token),
        _ if dev_mode => Ok("dev-internal-token-0000".to_owned()),
        _ => anyhow::bail!(
            "INTERNAL_SERVICE_TOKEN must be set (OBSERVABLE_ENV is not \"dev\"); refusing to \
             expose decide-access and lookup-principals without a caller credential"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_token_fails_closed_outside_dev() {
        assert!(resolve_internal_token(None, false).is_err());
        assert!(resolve_internal_token(Some("  ".into()), false).is_err());
        assert_eq!(
            resolve_internal_token(Some("t".into()), false).unwrap(),
            "t"
        );
        assert_eq!(
            resolve_internal_token(None, true).unwrap(),
            "dev-internal-token-0000"
        );
    }
}
