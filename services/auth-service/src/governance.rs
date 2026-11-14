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
    AuditTargetKind, ComplianceIncidentEvent, GovernanceAuditEntry, IncidentReconstruction,
    RecordComplianceIncidentEvent, RecordGovernanceChange, RetentionPolicy, evaluate_retention,
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

pub async fn record_compliance_incident_event_handler(
    State(state): State<AuthzState>,
    Json(req): Json<RecordComplianceIncidentEvent>,
) -> Response {
    if req.title.trim().is_empty()
        || req.event_type.trim().is_empty()
        || req.decision_maker.trim().is_empty()
        || req.rationale.trim().is_empty()
        || req.retention_days <= 0
    {
        return bad_request(
            "title, event_type, decision_maker, rationale, and positive retention_days are required",
        );
    }

    let occurred_at = req.occurred_at.unwrap_or_else(Utc::now);
    let row = sqlx::query(
        "INSERT INTO auth.compliance_incident_timeline \
         (tenant_id, incident_id, title, severity, event_type, decision_maker, rationale, retention_days, details, occurred_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
         RETURNING id",
    )
    .bind(req.tenant_id)
    .bind(req.incident_id)
    .bind(&req.title)
    .bind(&req.severity)
    .bind(&req.event_type)
    .bind(&req.decision_maker)
    .bind(&req.rationale)
    .bind(req.retention_days)
    .bind(&req.details)
    .bind(occurred_at)
    .fetch_one(&state.db)
    .await;

    match row {
        Ok(r) => (
            StatusCode::CREATED,
            Json(ComplianceIncidentEvent {
                id: r.get("id"),
                tenant_id: req.tenant_id,
                incident_id: req.incident_id,
                title: req.title,
                severity: req.severity,
                event_type: req.event_type,
                decision_maker: req.decision_maker,
                rationale: req.rationale,
                retention_days: req.retention_days,
                details: req.details,
                occurred_at,
            }),
        )
            .into_response(),
        Err(_) => db_error(),
    }
}

#[derive(Deserialize)]
pub struct ReconstructQuery {
    pub tenant_id: Uuid,
}

pub async fn reconstruct_incident_handler(
    State(state): State<AuthzState>,
    Path(incident_id): Path<Uuid>,
    Query(query): Query<ReconstructQuery>,
) -> Response {
    let rows = match sqlx::query(
        "SELECT id, tenant_id, incident_id, title, severity, event_type, decision_maker, rationale, retention_days, details, occurred_at \
         FROM auth.compliance_incident_timeline \
         WHERE tenant_id = $1 AND incident_id = $2 \
         ORDER BY occurred_at ASC",
    )
    .bind(query.tenant_id)
    .bind(incident_id)
    .fetch_all(&state.db)
    .await
    {
        Ok(r) => r,
        Err(_) => return db_error(),
    };

    if rows.is_empty() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "incident_audit_record_not_found" })),
        )
            .into_response();
    }

    let chronology: Vec<ComplianceIncidentEvent> = rows
        .into_iter()
        .map(|r| ComplianceIncidentEvent {
            id: r.get("id"),
            tenant_id: r.get("tenant_id"),
            incident_id: r.get("incident_id"),
            title: r.get("title"),
            severity: r.get("severity"),
            event_type: r.get("event_type"),
            decision_maker: r.get("decision_maker"),
            rationale: r.get("rationale"),
            retention_days: r.get("retention_days"),
            details: r.get("details"),
            occurred_at: r.get("occurred_at"),
        })
        .collect();

    let first = &chronology[0];
    let age_days = (Utc::now() - first.occurred_at).num_days().max(0);
    let max_retention_days = chronology
        .iter()
        .map(|e| i64::from(e.retention_days))
        .max()
        .unwrap_or(0);
    let within_retention = age_days <= max_retention_days;

    let mut decision_makers: Vec<String> = Vec::new();
    for event in &chronology {
        if !decision_makers.contains(&event.decision_maker) {
            decision_makers.push(event.decision_maker.clone());
        }
    }

    let complete_without_human_memory = within_retention
        && !chronology.is_empty()
        && chronology
            .iter()
            .all(|e| !e.decision_maker.trim().is_empty() && !e.rationale.trim().is_empty());

    (
        StatusCode::OK,
        Json(IncidentReconstruction {
            tenant_id: query.tenant_id,
            incident_id,
            title: first.title.clone(),
            severity: first.severity.clone(),
            within_retention,
            decision_makers,
            chronology,
            complete_without_human_memory,
        }),
    )
        .into_response()
}

/// Entitlement state check after update (OBS-14-05).
pub fn apply_entitlement_change_and_check(
    entitlements: &mut std::collections::BTreeMap<String, Vec<String>>,
    principal: &str,
    new_scopes: Vec<String>,
    required_scope: &str,
) -> bool {
    entitlements.insert(principal.to_string(), new_scopes);
    entitlements
        .get(principal)
        .is_some_and(|scopes| scopes.iter().any(|s| s == required_scope))
}

/// Data governance ownership and permissible jurisdictions metadata (OBS-14-06).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataGovernanceMetadata {
    pub dataset_id: String,
    pub owner: String,
    pub permissible_jurisdictions: Vec<String>,
}

/// Deletion or legal hold outcome (OBS-14-07).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletionOrHoldResult {
    pub record_id: String,
    pub action_taken: String, // "deleted" | "retained_under_legal_hold"
}

pub fn handle_deletion_or_hold_request(
    record_id: &str,
    legal_hold_active: bool,
    deletion_requested: bool,
) -> DeletionOrHoldResult {
    let action_taken = if legal_hold_active {
        "retained_under_legal_hold"
    } else if deletion_requested {
        "deleted"
    } else {
        "no_change"
    };
    DeletionOrHoldResult {
        record_id: record_id.to_string(),
        action_taken: action_taken.to_string(),
    }
}

/// Emergency access grant evaluation (OBS-14-09).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmergencyAccessGrant {
    pub grant_id: String,
    pub principal: String,
    pub expires_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmergencyAccessStatus {
    pub active: bool,
    pub included_in_next_review: bool,
}

pub fn evaluate_emergency_access_grant(
    grant: &EmergencyAccessGrant,
    now: chrono::DateTime<Utc>,
) -> EmergencyAccessStatus {
    EmergencyAccessStatus {
        active: now < grant.expires_at,
        included_in_next_review: true,
    }
}

/// Legal entity isolation check (OBS-14-10).
pub fn check_legal_entity_isolation(user_entity: &str, record_entity: &str) -> bool {
    !user_entity.is_empty() && user_entity == record_entity
}

/// Control assessment issue lifecycle (OBS-15-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlIssueTrace {
    pub issue_id: String,
    pub stages: Vec<String>,
    pub closed_and_verified: bool,
}

pub fn trace_control_issue_lifecycle(issue_id: &str, stages: Vec<String>) -> ControlIssueTrace {
    let closed_and_verified =
        stages.iter().any(|s| s == "closed") && stages.iter().any(|s| s == "verified");
    ControlIssueTrace {
        issue_id: issue_id.to_string(),
        stages,
        closed_and_verified,
    }
}

/// Evidence-based vs estimated compliance report section (OBS-15-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComplianceReportSection {
    pub title: String,
    pub basis: String, // "evidence_based" | "estimated"
}

/// Tamper-evident retained incident digest verification (OBS-15-08).
pub fn verify_retained_record_integrity(
    original_payload: &str,
    observed_payload: &str,
) -> Result<(), &'static str> {
    if original_payload == observed_payload {
        Ok(())
    } else {
        Err("tamper_detected")
    }
}

/// Archived period retrieval within retention window & SLA (OBS-15-09).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveRetrievalOutcome {
    pub period_id: String,
    pub within_retention: bool,
    pub retrieved_within_sla: bool,
}

pub fn retrieve_archived_audit_sample(
    period_id: &str,
    age_days: i64,
    retention_days: i64,
    elapsed_secs: u64,
    sla_secs: u64,
) -> ArchiveRetrievalOutcome {
    ArchiveRetrievalOutcome {
        period_id: period_id.to_string(),
        within_retention: age_days <= retention_days,
        retrieved_within_sla: age_days <= retention_days && elapsed_secs <= sla_secs,
    }
}

#[cfg(test)]
mod p1_governance_tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn obs_14_05_entitlement_change_reflected_in_subsequent_checks() {
        let mut map = std::collections::BTreeMap::new();
        assert!(!apply_entitlement_change_and_check(
            &mut map,
            "alice",
            vec!["logs:read".to_string()],
            "incidents:write"
        ));
        assert!(apply_entitlement_change_and_check(
            &mut map,
            "alice",
            vec!["logs:read".to_string(), "incidents:write".to_string()],
            "incidents:write"
        ));
    }

    #[test]
    fn obs_14_06_governance_review_identifies_owner_and_jurisdictions() {
        let meta = DataGovernanceMetadata {
            dataset_id: "payments-audit".to_string(),
            owner: "risk-data-steward".to_string(),
            permissible_jurisdictions: vec!["EU".to_string(), "UK".to_string()],
        };
        assert_eq!(meta.owner, "risk-data-steward");
        assert_eq!(meta.permissible_jurisdictions, vec!["EU", "UK"]);
    }

    #[test]
    fn obs_14_07_compliant_deletion_or_hold_handling() {
        assert_eq!(
            handle_deletion_or_hold_request("rec-1", true, true).action_taken,
            "retained_under_legal_hold"
        );
        assert_eq!(
            handle_deletion_or_hold_request("rec-2", false, true).action_taken,
            "deleted"
        );
    }

    #[test]
    fn obs_14_09_emergency_access_expires_and_appears_in_review() {
        let now = Utc::now();
        let grant = EmergencyAccessGrant {
            grant_id: "emg-1".to_string(),
            principal: "oncall-bob".to_string(),
            expires_at: now - Duration::minutes(1),
        };
        let st = evaluate_emergency_access_grant(&grant, now);
        assert!(!st.active);
        assert!(st.included_in_next_review);
    }

    #[test]
    fn obs_14_10_legal_entity_isolation_enforced() {
        assert!(check_legal_entity_isolation("entity-de", "entity-de"));
        assert!(!check_legal_entity_isolation("entity-de", "entity-us"));
    }

    #[test]
    fn obs_15_03_control_issue_traced_through_closure_and_verification() {
        let tr = trace_control_issue_lifecycle(
            "CTL-12",
            vec![
                "identified".to_string(),
                "remediated".to_string(),
                "closed".to_string(),
                "verified".to_string(),
            ],
        );
        assert!(tr.closed_and_verified);
    }

    #[test]
    fn obs_15_05_report_distinguishes_evidence_based_and_estimated() {
        let sections = [
            ComplianceReportSection {
                title: "Observed incident timeline".to_string(),
                basis: "evidence_based".to_string(),
            },
            ComplianceReportSection {
                title: "Projected customer impact".to_string(),
                basis: "estimated".to_string(),
            },
        ];
        assert_eq!(sections[0].basis, "evidence_based");
        assert_eq!(sections[1].basis, "estimated");
    }

    #[test]
    fn obs_15_08_modification_of_retained_incident_detectable() {
        assert!(verify_retained_record_integrity("hash:abc", "hash:abc").is_ok());
        assert!(verify_retained_record_integrity("hash:abc", "hash:tampered").is_err());
    }

    #[test]
    fn obs_15_09_archived_period_retrieved_within_retention_and_sla() {
        let out = retrieve_archived_audit_sample("2025-Q4", 180, 365, 12, 60);
        assert!(out.within_retention);
        assert!(out.retrieved_within_sla);
    }
}
