//! Data retention, legal-hold, and unified governance change-audit contracts
//! (`OBS-14-03`, `OBS-14-04`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionOutcome {
    Retain,
    Hold,
    Purge,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub tenant_id: Uuid,
    pub data_domain: String,
    pub classification: String,
    pub purpose: String,
    pub retention_days: i32,
    pub owner_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionDecision {
    pub tenant_id: Uuid,
    pub data_domain: String,
    pub classification: String,
    pub purpose: String,
    pub retention_days: i32,
    pub age_days: i64,
    pub legal_hold_active: bool,
    pub outcome: RetentionOutcome,
    pub retrievable: bool,
    pub reason: String,
}

/// Evaluates whether data of `age_days` under `policy` is retrievable.
///
/// Data past its `retention_days` is never retrievable (`Purge`) unless an
/// active legal hold (`Hold`) preserves it.
pub fn evaluate_retention(
    policy: &RetentionPolicy,
    age_days: i64,
    legal_hold_active: bool,
) -> RetentionDecision {
    let within_retention = age_days <= i64::from(policy.retention_days);
    let (outcome, retrievable, reason) = if within_retention {
        (
            RetentionOutcome::Retain,
            true,
            format!(
                "within {}d retention window for {}",
                policy.retention_days, policy.purpose
            ),
        )
    } else if legal_hold_active {
        (
            RetentionOutcome::Hold,
            true,
            format!(
                "past {}d retention window but preserved by active legal hold",
                policy.retention_days
            ),
        )
    } else {
        (
            RetentionOutcome::Purge,
            false,
            format!(
                "past {}d retention window with no active legal hold; not retrievable",
                policy.retention_days
            ),
        )
    };

    RetentionDecision {
        tenant_id: policy.tenant_id,
        data_domain: policy.data_domain.clone(),
        classification: policy.classification.clone(),
        purpose: policy.purpose.clone(),
        retention_days: policy.retention_days,
        age_days,
        legal_hold_active,
        outcome,
        retrievable,
        reason,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditTargetKind {
    Alert,
    AccessPolicy,
    Objective,
}

impl AuditTargetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Alert => "alert",
            Self::AccessPolicy => "access_policy",
            Self::Objective => "objective",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordGovernanceChange {
    pub tenant_id: Uuid,
    pub actor_id: String,
    pub actor_kind: String,
    pub target_kind: AuditTargetKind,
    pub target_id: String,
    pub action: String,
    pub route: String,
    #[serde(default)]
    pub details: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GovernanceAuditEntry {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub actor_id: String,
    pub actor_kind: String,
    pub target_kind: AuditTargetKind,
    pub target_id: String,
    pub action: String,
    pub route: String,
    pub details: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordComplianceIncidentEvent {
    pub tenant_id: Uuid,
    pub incident_id: Uuid,
    pub title: String,
    pub severity: String,
    pub event_type: String,
    pub decision_maker: String,
    pub rationale: String,
    pub retention_days: i32,
    #[serde(default)]
    pub details: serde_json::Value,
    pub occurred_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComplianceIncidentEvent {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub incident_id: Uuid,
    pub title: String,
    pub severity: String,
    pub event_type: String,
    pub decision_maker: String,
    pub rationale: String,
    pub retention_days: i32,
    pub details: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IncidentReconstruction {
    pub tenant_id: Uuid,
    pub incident_id: Uuid,
    pub title: String,
    pub severity: String,
    pub within_retention: bool,
    pub decision_makers: Vec<String>,
    pub chronology: Vec<ComplianceIncidentEvent>,
    pub complete_without_human_memory: bool,
}
