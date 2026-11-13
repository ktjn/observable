//! Tenant health rules (phase 2 E1): `GET /v1/health/rules` and `PUT /v1/health/rules`.
//!
//! - Reads return the caller tenant's effective rule: its newest revision, else the platform
//!   default. `decide-access` read on `health.rule` is checked first.
//! - Publishing requires `decide-access` write on `health.rule` (tenant_admin by default) and a
//!   user session, validates the rule, and inserts a new immutable revision that records the
//!   publisher. Evaluation picks it up on the next pass; snapshots of the previous revision
//!   are withheld as `missing` until then.

use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use observable_auth::authz::{AccessDecider, Classification, DecideRequest, Decision, ResourceRef};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::HealthState;
use crate::assessment::{Condition, validate_rule};
use crate::ports::Caller;

pub const RULE: &str = "health.rule";

fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(json!({ "error": code }))).into_response()
}

fn invalid(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "invalid_request", "message": message })),
    )
        .into_response()
}

#[allow(clippy::result_large_err)]
async fn caller(state: &HealthState, headers: &HeaderMap) -> Result<Caller, Response> {
    state.credentials.verify(headers).await.map_err(|e| {
        let status = StatusCode::from(e);
        let code = if status == StatusCode::BAD_REQUEST {
            "invalid_request"
        } else {
            "unauthorized"
        };
        error(status, code)
    })
}

#[allow(clippy::result_large_err)]
async fn authorize(state: &HealthState, caller: &Caller, action: &str) -> Result<(), Response> {
    let req = DecideRequest {
        tenant_id: Some(caller.tenant_id),
        principal: Some(caller.principal()),
        action: Some(action.into()),
        resource: Some(ResourceRef::new(RULE, "current", Classification::Internal)),
        purpose: None,
        elevation_id: None,
    };
    match state.decider.decide(&req).await {
        Ok(Decision::Permit) => Ok(()),
        Ok(Decision::Deny | Decision::PurposeMissing) => {
            Err(error(StatusCode::FORBIDDEN, "forbidden"))
        }
        Err(_) => Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "access_decision_unavailable",
        )),
    }
}

type RuleRow = (
    String,
    i32,
    Option<Uuid>,
    i32,
    i32,
    serde_json::Value,
    DateTime<Utc>,
    Option<Uuid>,
);

fn rule_json(row: RuleRow) -> serde_json::Value {
    let (rule_id, revision, tenant_id, freshness, lookback, conditions, published_at, by) = row;
    json!({
        "rule_id": rule_id,
        "revision": revision,
        "source": if tenant_id.is_some() { "tenant" } else { "platform" },
        "freshness_seconds": freshness,
        "lookback_seconds": lookback,
        "conditions": conditions,
        "published_at": published_at,
        "published_by": by,
    })
}

const SELECT: &str = "SELECT rule_id, revision, tenant_id, freshness_seconds, lookback_seconds, \
     conditions, published_at, published_by FROM health.rules";

pub async fn get_rule(State(state): State<HealthState>, headers: HeaderMap) -> Response {
    let caller = match caller(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    if let Err(r) = authorize(&state, &caller, "read").await {
        return r;
    }
    let row: Result<Option<RuleRow>, _> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT} WHERE tenant_id = $1 OR tenant_id IS NULL \
         ORDER BY (tenant_id IS NULL), revision DESC, published_at DESC LIMIT 1"
    )))
    .bind(caller.tenant_id)
    .fetch_optional(&state.db)
    .await;
    match row {
        Ok(Some(row)) => Json(rule_json(row)).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, "not_found"),
        Err(e) => {
            tracing::warn!(error = %e, "health rule store unavailable");
            error(StatusCode::SERVICE_UNAVAILABLE, "rule_store_unavailable")
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublishBody {
    freshness_seconds: i32,
    lookback_seconds: i32,
    conditions: Vec<Condition>,
}

pub async fn put_rule(
    State(state): State<HealthState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let caller = match caller(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    if let Err(r) = authorize(&state, &caller, "write").await {
        return r;
    }
    // A revision records who published it; API keys have no accountable user.
    let Some(user_id) = caller.user_id else {
        return error(StatusCode::FORBIDDEN, "user_session_required");
    };
    let body: PublishBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => return invalid(&e.to_string()),
    };
    if let Err(message) = validate_rule(
        body.freshness_seconds,
        body.lookback_seconds,
        &body.conditions,
    ) {
        return invalid(&message);
    }
    let rule_id = format!("tenant-{}", caller.tenant_id);
    let conditions = json!(body.conditions);
    // Concurrent publishers race on the primary key; the loser retries with the next revision.
    for _ in 0..5 {
        let row: Result<Option<RuleRow>, _> = sqlx::query_as(
            "INSERT INTO health.rules (rule_id, revision, tenant_id, freshness_seconds, \
             lookback_seconds, conditions, published_by) \
             SELECT $1, COALESCE(MAX(revision), 0) + 1, $2, $3, $4, $5, $6 \
             FROM health.rules WHERE rule_id = $1 \
             ON CONFLICT (rule_id, revision) DO NOTHING \
             RETURNING rule_id, revision, tenant_id, freshness_seconds, lookback_seconds, \
             conditions, published_at, published_by",
        )
        .bind(&rule_id)
        .bind(caller.tenant_id)
        .bind(body.freshness_seconds)
        .bind(body.lookback_seconds)
        .bind(&conditions)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await;
        match row {
            Ok(Some(row)) => {
                tracing::info!(
                    tenant_id = %caller.tenant_id,
                    %user_id,
                    revision = row.1,
                    "health rule published"
                );
                return (StatusCode::CREATED, Json(rule_json(row))).into_response();
            }
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!(error = %e, "health rule store unavailable");
                return error(StatusCode::SERVICE_UNAVAILABLE, "rule_store_unavailable");
            }
        }
    }
    error(StatusCode::CONFLICT, "concurrent_publish")
}
