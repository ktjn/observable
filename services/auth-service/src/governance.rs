//! Governance handlers for data retention policies, legal holds (`OBS-14-03`),
//! and unified change-audit attribution (`OBS-14-04`).

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use observable_auth::governance::{
    AuditTargetKind, GovernanceAuditEntry, RecordGovernanceChange, RetentionPolicy,
    evaluate_retention,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::authz::AuthzState;

#[derive(Deserialize)]
pub struct UpsertRetentionPolicyRequest {
    pub tenant_id: Uuid,
    pub data_domain: String,
    pub classification: String,
    pub purpose: String,
    pub retention_days: i32,
    pub owner_id: String,
}

#[derive(Deserialize)]
pub struct EvaluateRetentionRequest {
    pub tenant_id: Uuid,
    pub data_domain: String,
    pub classification: String,
    pub age_days: i64,
}

#[derive(Deserialize)]
pub struct LegalHoldRequest {
    pub tenant_id: Uuid,
    pub data_domain: String,
    pub reason: String,
    pub placed_by: String,
    pub active: bool,
}

#[derive(Serialize)]
pub struct LegalHoldRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub data_domain: String,
    pub reason: String,
    pub placed_by: String,
    pub active: bool,
    pub placed_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct RetentionPolicyReviewResponse {
    pub tenant_id: Uuid,
    pub policies: Vec<RetentionPolicy>,
    pub active_legal_holds: Vec<LegalHoldRecord>,
}

#[derive(Deserialize)]
pub struct AuditQuery {
    pub tenant_id: Uuid,
    pub target_kind: Option<AuditTargetKind>,
}

fn bad_request(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
}

fn db_error() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "governance_store_unavailable" })),
    )
        .into_response()
}

pub async fn upsert_retention_policy(
    State(state): State<AuthzState>,
    Json(req): Json<UpsertRetentionPolicyRequest>,
) -> Response {
    if req.retention_days <= 0
        || req.data_domain.trim().is_empty()
        || req.purpose.trim().is_empty()
        || req.owner_id.trim().is_empty()
    {
        return bad_request("invalid retention policy fields");
    }

    let res = sqlx::query(
        "INSERT INTO auth.data_retention_policies \
         (tenant_id, data_domain, classification, purpose, retention_days, owner_id, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, now()) \
         ON CONFLICT (tenant_id, data_domain, classification) \
         DO UPDATE SET purpose = EXCLUDED.purpose, retention_days = EXCLUDED.retention_days, \
         owner_id = EXCLUDED.owner_id, updated_at = now()",
    )
    .bind(req.tenant_id)
    .bind(&req.data_domain)
    .bind(&req.classification)
    .bind(&req.purpose)
    .bind(req.retention_days)
    .bind(&req.owner_id)
    .execute(&state.db)
    .await;

    match res {
        Ok(_) => (
            StatusCode::OK,
            Json(RetentionPolicy {
                tenant_id: req.tenant_id,
                data_domain: req.data_domain,
                classification: req.classification,
                purpose: req.purpose,
                retention_days: req.retention_days,
                owner_id: req.owner_id,
            }),
        )
            .into_response(),
        Err(_) => db_error(),
    }
}

pub async fn review_retention_policies(
    State(state): State<AuthzState>,
    Path(tenant_id): Path<Uuid>,
) -> Response {
    let policy_rows = match sqlx::query(
        "SELECT tenant_id, data_domain, classification, purpose, retention_days, owner_id \
         FROM auth.data_retention_policies WHERE tenant_id = $1 \
         ORDER BY data_domain, classification",
    )
    .bind(tenant_id)
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => rows,
        Err(_) => return db_error(),
    };

    let hold_rows = match sqlx::query(
        "SELECT id, tenant_id, data_domain, reason, placed_by, active, placed_at \
         FROM auth.data_legal_holds WHERE tenant_id = $1 AND active = true \
         ORDER BY placed_at DESC",
    )
    .bind(tenant_id)
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => rows,
        Err(_) => return db_error(),
    };

    let policies = policy_rows
        .into_iter()
        .map(|r| RetentionPolicy {
            tenant_id: r.get("tenant_id"),
            data_domain: r.get("data_domain"),
            classification: r.get("classification"),
            purpose: r.get("purpose"),
            retention_days: r.get("retention_days"),
            owner_id: r.get("owner_id"),
        })
        .collect();

    let active_legal_holds = hold_rows
        .into_iter()
        .map(|r| LegalHoldRecord {
            id: r.get("id"),
            tenant_id: r.get("tenant_id"),
            data_domain: r.get("data_domain"),
            reason: r.get("reason"),
            placed_by: r.get("placed_by"),
            active: r.get("active"),
            placed_at: r.get("placed_at"),
        })
        .collect();

    (
        StatusCode::OK,
        Json(RetentionPolicyReviewResponse {
            tenant_id,
            policies,
            active_legal_holds,
        }),
    )
        .into_response()
}

pub async fn set_legal_hold(
    State(state): State<AuthzState>,
    Json(req): Json<LegalHoldRequest>,
) -> Response {
    if req.data_domain.trim().is_empty() || req.placed_by.trim().is_empty() {
        return bad_request("invalid legal hold fields");
    }

    if !req.active {
        let res = sqlx::query(
            "UPDATE auth.data_legal_holds SET active = false, released_at = now() \
             WHERE tenant_id = $1 AND data_domain = $2 AND active = true",
        )
        .bind(req.tenant_id)
        .bind(&req.data_domain)
        .execute(&state.db)
        .await;
        return match res {
            Ok(_) => (StatusCode::OK, Json(json!({ "active": false }))).into_response(),
            Err(_) => db_error(),
        };
    }

    let row = sqlx::query(
        "INSERT INTO auth.data_legal_holds (tenant_id, data_domain, reason, placed_by, active) \
         VALUES ($1, $2, $3, $4, true) \
         RETURNING id, tenant_id, data_domain, reason, placed_by, active, placed_at",
    )
    .bind(req.tenant_id)
    .bind(&req.data_domain)
    .bind(&req.reason)
    .bind(&req.placed_by)
    .fetch_one(&state.db)
    .await;

    match row {
        Ok(r) => (
            StatusCode::OK,
            Json(LegalHoldRecord {
                id: r.get("id"),
                tenant_id: r.get("tenant_id"),
                data_domain: r.get("data_domain"),
                reason: r.get("reason"),
                placed_by: r.get("placed_by"),
                active: r.get("active"),
                placed_at: r.get("placed_at"),
            }),
        )
            .into_response(),
        Err(_) => db_error(),
    }
}

pub async fn evaluate_retention_handler(
    State(state): State<AuthzState>,
    Json(req): Json<EvaluateRetentionRequest>,
) -> Response {
    let policy_row = match sqlx::query(
        "SELECT tenant_id, data_domain, classification, purpose, retention_days, owner_id \
         FROM auth.data_retention_policies \
         WHERE tenant_id = $1 AND data_domain = $2 AND classification = $3",
    )
    .bind(req.tenant_id)
    .bind(&req.data_domain)
    .bind(&req.classification)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(r)) => RetentionPolicy {
            tenant_id: r.get("tenant_id"),
            data_domain: r.get("data_domain"),
            classification: r.get("classification"),
            purpose: r.get("purpose"),
            retention_days: r.get("retention_days"),
            owner_id: r.get("owner_id"),
        },
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "retention_policy_not_found" })),
            )
                .into_response();
        }
        Err(_) => return db_error(),
    };

    let hold_active: bool = match sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM auth.data_legal_holds \
         WHERE tenant_id = $1 AND data_domain = $2 AND active = true)",
    )
    .bind(req.tenant_id)
    .bind(&req.data_domain)
    .fetch_one(&state.db)
    .await
    {
        Ok(v) => v,
        Err(_) => return db_error(),
    };

    let decision = evaluate_retention(&policy_row, req.age_days, hold_active);
    (StatusCode::OK, Json(decision)).into_response()
}

pub async fn record_governance_change_handler(
    State(state): State<AuthzState>,
    Json(req): Json<RecordGovernanceChange>,
) -> Response {
    if req.actor_id.trim().is_empty()
        || req.target_id.trim().is_empty()
        || req.action.trim().is_empty()
        || req.route.trim().is_empty()
    {
        return bad_request("actor_id, target_id, action, and route are required");
    }

    let row = sqlx::query(
        "INSERT INTO auth.governance_audit_log \
         (tenant_id, actor_id, actor_kind, target_kind, target_id, action, route, details) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         RETURNING id, occurred_at",
    )
    .bind(req.tenant_id)
    .bind(&req.actor_id)
    .bind(&req.actor_kind)
    .bind(req.target_kind.as_str())
    .bind(&req.target_id)
    .bind(&req.action)
    .bind(&req.route)
    .bind(&req.details)
    .fetch_one(&state.db)
    .await;

    match row {
        Ok(r) => (
            StatusCode::CREATED,
            Json(GovernanceAuditEntry {
                id: r.get("id"),
                tenant_id: req.tenant_id,
                actor_id: req.actor_id,
                actor_kind: req.actor_kind,
                target_kind: req.target_kind,
                target_id: req.target_id,
                action: req.action,
                route: req.route,
                details: req.details,
                occurred_at: r.get("occurred_at"),
            }),
        )
            .into_response(),
        Err(_) => db_error(),
    }
}

pub async fn list_governance_changes_handler(
    State(state): State<AuthzState>,
    Query(query): Query<AuditQuery>,
) -> Response {
    let rows = match query.target_kind {
        Some(kind) => {
            sqlx::query(
                "SELECT id, tenant_id, actor_id, actor_kind, target_kind, target_id, action, route, details, occurred_at \
                 FROM auth.governance_audit_log \
                 WHERE tenant_id = $1 AND target_kind = $2 \
                 ORDER BY occurred_at DESC",
            )
            .bind(query.tenant_id)
            .bind(kind.as_str())
            .fetch_all(&state.db)
            .await
        }
        None => {
            sqlx::query(
                "SELECT id, tenant_id, actor_id, actor_kind, target_kind, target_id, action, route, details, occurred_at \
                 FROM auth.governance_audit_log \
                 WHERE tenant_id = $1 \
                 ORDER BY occurred_at DESC",
            )
            .bind(query.tenant_id)
            .fetch_all(&state.db)
            .await
        }
    };

    let rows = match rows {
        Ok(r) => r,
        Err(_) => return db_error(),
    };

    let entries: Vec<GovernanceAuditEntry> = rows
        .into_iter()
        .map(|r| {
            let kind_str: &str = r.get("target_kind");
            let target_kind = match kind_str {
                "alert" => AuditTargetKind::Alert,
                "access_policy" => AuditTargetKind::AccessPolicy,
                _ => AuditTargetKind::Objective,
            };
            GovernanceAuditEntry {
                id: r.get("id"),
                tenant_id: r.get("tenant_id"),
                actor_id: r.get("actor_id"),
                actor_kind: r.get("actor_kind"),
                target_kind,
                target_id: r.get("target_id"),
                action: r.get("action"),
                route: r.get("route"),
                details: r.get("details"),
                occurred_at: r.get("occurred_at"),
            }
        })
        .collect();

    (StatusCode::OK, Json(entries)).into_response()
}
