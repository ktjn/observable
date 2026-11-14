//! Business transaction lookup and path inspection (`trace-transactions`,
//! Phase 7 S1: OBS-31-01, OBS-31-02, OBS-31-03).
//!
//! `GET /v1/transactions/lookup?reference=&purpose=` enforces stated-purpose,
//! role, and data-classification authorization, records every lookup attempt
//! (allowed and refused) in the audit sink with actor, purpose, and reference,
//! and surfaces delayed steps or incomplete path gaps without assuming completion.

use std::sync::{Arc, Mutex};

use axum::{
    Json,
    extract::{Extension, Query},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::middleware::auth::TenantContext;

const ALLOWED_PURPOSES: [&str; 4] = [
    "customer_support",
    "incident_investigation",
    "compliance_audit",
    "fraud_review",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransactionStep {
    pub sequence_index: u32,
    pub service_name: String,
    pub operation: String,
    pub duration_ms: u64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRecord {
    pub reference: String,
    pub status: String,
    pub data_classification: String,
    pub allowed_purposes: Vec<String>,
    pub steps: Vec<TransactionStep>,
    pub expected_step_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransactionLookupAudit {
    pub tenant_id: Uuid,
    pub actor: String,
    pub role: String,
    pub purpose: Option<String>,
    pub reference: String,
    pub outcome: String,
    pub recorded_at: DateTime<Utc>,
}

#[async_trait::async_trait]
pub trait TransactionLookupSource: Send + Sync {
    async fn find_by_reference(
        &self,
        tenant_id: Uuid,
        reference: &str,
    ) -> Result<Option<TransactionRecord>, ()>;
}

#[async_trait::async_trait]
pub trait TransactionAuditSink: Send + Sync {
    async fn record(&self, entry: TransactionLookupAudit);
    async fn list(&self, tenant_id: Uuid) -> Vec<TransactionLookupAudit>;
}

#[derive(Default)]
pub struct InMemoryTransactionSource {
    records: Mutex<Vec<(Uuid, TransactionRecord)>>,
}

impl InMemoryTransactionSource {
    #[allow(dead_code)]
    pub fn with_records(records: Vec<(Uuid, TransactionRecord)>) -> Self {
        Self {
            records: Mutex::new(records),
        }
    }
}

#[async_trait::async_trait]
impl TransactionLookupSource for InMemoryTransactionSource {
    async fn find_by_reference(
        &self,
        tenant_id: Uuid,
        reference: &str,
    ) -> Result<Option<TransactionRecord>, ()> {
        let guard = self.records.lock().unwrap();
        Ok(guard
            .iter()
            .find(|(t, r)| *t == tenant_id && r.reference == reference)
            .map(|(_, r)| r.clone()))
    }
}

#[derive(Default)]
pub struct InMemoryTransactionAuditSink {
    entries: Mutex<Vec<TransactionLookupAudit>>,
}

#[async_trait::async_trait]
impl TransactionAuditSink for InMemoryTransactionAuditSink {
    async fn record(&self, entry: TransactionLookupAudit) {
        self.entries.lock().unwrap().push(entry);
    }

    async fn list(&self, tenant_id: Uuid) -> Vec<TransactionLookupAudit> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.tenant_id == tenant_id)
            .cloned()
            .collect()
    }
}

#[derive(Debug, Deserialize)]
pub struct TransactionLookupQuery {
    pub reference: String,
    pub purpose: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TransactionLookupResponse {
    pub reference: String,
    pub status: String,
    pub data_classification: String,
    pub purpose: String,
    pub steps: Vec<TransactionStep>,
    pub held_at_step: Option<String>,
    pub path_complete: bool,
    pub gap_disclosure: Option<String>,
}

fn role_allows_purpose_and_classification(role: &str, purpose: &str, classification: &str) -> bool {
    match role {
        "tenant_admin" | "admin" => true,
        "auditor" => purpose == "compliance_audit",
        "responder" | "operator" | "sre" => {
            matches!(purpose, "incident_investigation" | "customer_support")
                && classification != "restricted"
        }
        "support_agent" => {
            purpose == "customer_support" && matches!(classification, "public" | "internal")
        }
        _ => false,
    }
}

pub async fn lookup_transaction(
    source: Extension<Arc<dyn TransactionLookupSource>>,
    audit_sink: Extension<Arc<dyn TransactionAuditSink>>,
    tenant: Extension<TenantContext>,
    headers: HeaderMap,
    Query(q): Query<TransactionLookupQuery>,
) -> Response {
    let reference = q.reference.trim().to_string();
    let purpose = q
        .purpose
        .or_else(|| {
            headers
                .get("x-access-purpose")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let actor = tenant
        .user_id
        .map(|u| format!("user:{u}"))
        .unwrap_or_else(|| format!("role:{}", tenant.role));

    let Some(ref stated_purpose) = purpose else {
        audit_sink
            .record(TransactionLookupAudit {
                tenant_id: tenant.tenant_id,
                actor,
                role: tenant.role.clone(),
                purpose: None,
                reference: reference.clone(),
                outcome: "refused_missing_purpose".to_string(),
                recorded_at: Utc::now(),
            })
            .await;
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "purpose_required",
                "message": "A stated access purpose is required for transaction lookups"
            })),
        )
            .into_response();
    };

    if !ALLOWED_PURPOSES.contains(&stated_purpose.as_str()) {
        audit_sink
            .record(TransactionLookupAudit {
                tenant_id: tenant.tenant_id,
                actor,
                role: tenant.role.clone(),
                purpose: Some(stated_purpose.clone()),
                reference: reference.clone(),
                outcome: "refused_invalid_purpose".to_string(),
                recorded_at: Utc::now(),
            })
            .await;
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "invalid_purpose",
                "message": format!("Purpose '{stated_purpose}' is not permitted")
            })),
        )
            .into_response();
    }

    let Ok(found) = source.find_by_reference(tenant.tenant_id, &reference).await else {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": "lookup_unavailable" })),
        )
            .into_response();
    };

    let Some(rec) = found else {
        audit_sink
            .record(TransactionLookupAudit {
                tenant_id: tenant.tenant_id,
                actor,
                role: tenant.role.clone(),
                purpose: Some(stated_purpose.clone()),
                reference: reference.clone(),
                outcome: "not_found".to_string(),
                recorded_at: Utc::now(),
            })
            .await;
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "transaction_not_found" })),
        )
            .into_response();
    };

    let purpose_permitted = (rec.allowed_purposes.is_empty()
        || rec.allowed_purposes.iter().any(|p| p == stated_purpose))
        && role_allows_purpose_and_classification(
            &tenant.role,
            stated_purpose,
            &rec.data_classification,
        );

    if !purpose_permitted {
        audit_sink
            .record(TransactionLookupAudit {
                tenant_id: tenant.tenant_id,
                actor,
                role: tenant.role.clone(),
                purpose: Some(stated_purpose.clone()),
                reference: reference.clone(),
                outcome: "refused_unauthorized".to_string(),
                recorded_at: Utc::now(),
            })
            .await;
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "access_denied",
                "message": "Lookup refused for caller role, purpose, or data classification"
            })),
        )
            .into_response();
    }

    audit_sink
        .record(TransactionLookupAudit {
            tenant_id: tenant.tenant_id,
            actor,
            role: tenant.role.clone(),
            purpose: Some(stated_purpose.clone()),
            reference: reference.clone(),
            outcome: "allowed".to_string(),
            recorded_at: Utc::now(),
        })
        .await;

    let mut steps = rec.steps;
    steps.sort_by_key(|s| s.sequence_index);

    let has_gap = steps.len() < rec.expected_step_count
        || steps
            .windows(2)
            .any(|w| w[1].sequence_index != w[0].sequence_index + 1);
    let path_complete = !has_gap && !steps.is_empty();

    let held_at_step = steps
        .iter()
        .find(|s| s.status == "delayed" || s.status == "held")
        .or_else(|| {
            steps
                .iter()
                .filter(|s| s.duration_ms >= 1000)
                .max_by_key(|s| s.duration_ms)
        })
        .map(|s| format!("{}:{}", s.service_name, s.operation));

    let gap_disclosure = if !path_complete {
        Some(format!(
            "Incomplete path evidence: observed {} of {} expected steps",
            steps.len(),
            rec.expected_step_count
        ))
    } else {
        None
    };

    // Never report an incomplete path as completed (OBS-31-02 negative).
    let effective_status = if !path_complete && rec.status == "completed" {
        "incomplete".to_string()
    } else {
        rec.status
    };

    (
        StatusCode::OK,
        Json(TransactionLookupResponse {
            reference: rec.reference,
            status: effective_status,
            data_classification: rec.data_classification,
            purpose: stated_purpose.clone(),
            steps,
            held_at_step,
            path_complete,
            gap_disclosure,
        }),
    )
        .into_response()
}

pub async fn list_transaction_audit(
    audit_sink: Extension<Arc<dyn TransactionAuditSink>>,
    tenant: Extension<TenantContext>,
) -> Response {
    let entries = audit_sink.list(tenant.tenant_id).await;
    (StatusCode::OK, Json(json!({ "items": entries }))).into_response()
}
