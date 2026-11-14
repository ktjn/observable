//! Customer experience and business/scheduled process health
//! (`assess-customer-experience`, `track-business-processes`, Phase 7 S3:
//! OBS-05-01, OBS-05-02, OBS-12-01, OBS-12-02, OBS-32-01, OBS-32-02).

use std::sync::{Arc, Mutex};

use axum::{
    Json,
    extract::{Extension, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::middleware::auth::TenantContext;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerJourneyTelemetry {
    pub journey_id: String,
    pub name: String,
    pub success_count: u64,
    pub failure_count: u64,
    pub client_error_count: u64,
    pub p95_latency_ms: f64,
    pub backend_healthy: bool,
    pub last_evidence_at: Option<DateTime<Utc>>,
    pub freshness_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessProcessMeasure {
    pub process_id: String,
    pub name: String,
    pub owner: String,
    pub completed_count: u64,
    pub failed_count: u64,
    pub stalled_count: u64,
    pub involved_services_available: bool,
    pub last_reported_at: Option<DateTime<Utc>>,
    pub freshness_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledProcessSpec {
    pub process_id: String,
    pub name: String,
    pub criticality: String,
    pub expected_start: Option<DateTime<Utc>>,
    pub completion_deadline: Option<DateTime<Utc>>,
    pub start_grace_seconds: i64,
    pub owner: Option<String>,
    pub dependencies: Vec<String>,
    pub run_started_at: Option<DateTime<Utc>>,
    pub run_completed_at: Option<DateTime<Utc>>,
    pub estimated_finish_at: Option<DateTime<Utc>>,
}

#[async_trait::async_trait]
pub trait BusinessHealthSource: Send + Sync {
    async fn get_journey(
        &self,
        tenant_id: Uuid,
        journey_id: &str,
    ) -> Result<Option<CustomerJourneyTelemetry>, ()>;

    async fn get_process(
        &self,
        tenant_id: Uuid,
        process_id: &str,
    ) -> Result<Option<BusinessProcessMeasure>, ()>;

    async fn upsert_scheduled_process(&self, tenant_id: Uuid, spec: ScheduledProcessSpec);

    async fn get_scheduled_process(
        &self,
        tenant_id: Uuid,
        process_id: &str,
    ) -> Result<Option<ScheduledProcessSpec>, ()>;
}

#[derive(Default)]
pub struct InMemoryBusinessHealthSource {
    journeys: Mutex<Vec<(Uuid, CustomerJourneyTelemetry)>>,
    processes: Mutex<Vec<(Uuid, BusinessProcessMeasure)>>,
    scheduled: Mutex<Vec<(Uuid, ScheduledProcessSpec)>>,
}

impl InMemoryBusinessHealthSource {
    #[allow(dead_code)]
    pub fn new(
        journeys: Vec<(Uuid, CustomerJourneyTelemetry)>,
        processes: Vec<(Uuid, BusinessProcessMeasure)>,
        scheduled: Vec<(Uuid, ScheduledProcessSpec)>,
    ) -> Self {
        Self {
            journeys: Mutex::new(journeys),
            processes: Mutex::new(processes),
            scheduled: Mutex::new(scheduled),
        }
    }
}

#[async_trait::async_trait]
impl BusinessHealthSource for InMemoryBusinessHealthSource {
    async fn get_journey(
        &self,
        tenant_id: Uuid,
        journey_id: &str,
    ) -> Result<Option<CustomerJourneyTelemetry>, ()> {
        Ok(self
            .journeys
            .lock()
            .unwrap()
            .iter()
            .find(|(t, j)| *t == tenant_id && j.journey_id == journey_id)
            .map(|(_, j)| j.clone()))
    }

    async fn get_process(
        &self,
        tenant_id: Uuid,
        process_id: &str,
    ) -> Result<Option<BusinessProcessMeasure>, ()> {
        Ok(self
            .processes
            .lock()
            .unwrap()
            .iter()
            .find(|(t, p)| *t == tenant_id && p.process_id == process_id)
            .map(|(_, p)| p.clone()))
    }

    async fn upsert_scheduled_process(&self, tenant_id: Uuid, spec: ScheduledProcessSpec) {
        let mut guard = self.scheduled.lock().unwrap();
        if let Some(existing) = guard
            .iter_mut()
            .find(|(t, s)| *t == tenant_id && s.process_id == spec.process_id)
        {
            existing.1 = spec;
        } else {
            guard.push((tenant_id, spec));
        }
    }

    async fn get_scheduled_process(
        &self,
        tenant_id: Uuid,
        process_id: &str,
    ) -> Result<Option<ScheduledProcessSpec>, ()> {
        Ok(self
            .scheduled
            .lock()
            .unwrap()
            .iter()
            .find(|(t, s)| *t == tenant_id && s.process_id == process_id)
            .map(|(_, s)| s.clone()))
    }
}

#[derive(Debug, Deserialize)]
pub struct JourneyQuery {
    pub journey_id: String,
    pub now: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct JourneyHealthResponse {
    pub journey_id: String,
    pub name: String,
    pub status: String,
    pub customer_impact: bool,
    pub evidence_status: String,
    pub backend_healthy: bool,
    pub failure_count: u64,
    pub client_error_count: u64,
    pub p95_latency_ms: f64,
}

pub async fn get_journey_health(
    source: Extension<Arc<dyn BusinessHealthSource>>,
    tenant: Extension<TenantContext>,
    Query(q): Query<JourneyQuery>,
) -> Response {
    let Ok(Some(j)) = source.get_journey(tenant.tenant_id, &q.journey_id).await else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "journey_not_found" })),
        )
            .into_response();
    };

    let now = q.now.unwrap_or_else(Utc::now);
    let evidence_current = j
        .last_evidence_at
        .is_some_and(|ts| (now - ts) <= Duration::seconds(j.freshness_seconds));

    if !evidence_current {
        return (
            StatusCode::OK,
            Json(JourneyHealthResponse {
                journey_id: j.journey_id,
                name: j.name,
                status: "unknown".to_string(),
                customer_impact: false,
                evidence_status: if j.last_evidence_at.is_none() {
                    "missing".to_string()
                } else {
                    "stale".to_string()
                },
                backend_healthy: j.backend_healthy,
                failure_count: j.failure_count,
                client_error_count: j.client_error_count,
                p95_latency_ms: j.p95_latency_ms,
            }),
        )
            .into_response();
    }

    let has_failure = j.failure_count > 0 || j.client_error_count > 0;
    let status = if j.failure_count > 0 && j.success_count == 0 {
        "failed"
    } else if has_failure {
        "degraded"
    } else {
        "healthy"
    };

    (
        StatusCode::OK,
        Json(JourneyHealthResponse {
            journey_id: j.journey_id,
            name: j.name,
            status: status.to_string(),
            customer_impact: has_failure,
            evidence_status: "current".to_string(),
            backend_healthy: j.backend_healthy,
            failure_count: j.failure_count,
            client_error_count: j.client_error_count,
            p95_latency_ms: j.p95_latency_ms,
        }),
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
pub struct ProcessQuery {
    pub process_id: String,
    pub now: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct ProcessHealthResponse {
    pub process_id: String,
    pub name: String,
    pub owner: String,
    pub outcome: String,
    pub source_status: String,
    pub involved_services_available: bool,
    pub completed_count: u64,
    pub failed_count: u64,
    pub stalled_count: u64,
}

pub async fn get_process_health(
    source: Extension<Arc<dyn BusinessHealthSource>>,
    tenant: Extension<TenantContext>,
    Query(q): Query<ProcessQuery>,
) -> Response {
    let Ok(Some(p)) = source.get_process(tenant.tenant_id, &q.process_id).await else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "process_not_found" })),
        )
            .into_response();
    };

    let now = q.now.unwrap_or_else(Utc::now);
    let fresh = p
        .last_reported_at
        .is_some_and(|ts| (now - ts) <= Duration::seconds(p.freshness_seconds));

    if !fresh {
        return (
            StatusCode::OK,
            Json(ProcessHealthResponse {
                process_id: p.process_id,
                name: p.name,
                owner: p.owner,
                outcome: "unknown".to_string(),
                source_status: "stopped_reporting".to_string(),
                involved_services_available: p.involved_services_available,
                completed_count: p.completed_count,
                failed_count: p.failed_count,
                stalled_count: p.stalled_count,
            }),
        )
            .into_response();
    }

    let outcome = if p.stalled_count > 0 {
        "stalled"
    } else if p.failed_count > 0 {
        "degraded"
    } else {
        "normal"
    };

    (
        StatusCode::OK,
        Json(ProcessHealthResponse {
            process_id: p.process_id,
            name: p.name,
            owner: p.owner,
            outcome: outcome.to_string(),
            source_status: "reporting".to_string(),
            involved_services_available: p.involved_services_available,
            completed_count: p.completed_count,
            failed_count: p.failed_count,
            stalled_count: p.stalled_count,
        }),
    )
        .into_response()
}

#[derive(Debug, Serialize)]
pub struct ScheduledProcessStatusResponse {
    pub process_id: String,
    pub name: String,
    pub criticality: String,
    pub expected_start: Option<DateTime<Utc>>,
    pub completion_deadline: Option<DateTime<Utc>>,
    pub owner: Option<String>,
    pub registration_status: String,
    pub incomplete_reasons: Vec<String>,
    pub run_status: String,
    pub warning: Option<String>,
}

fn evaluate_scheduled(
    spec: &ScheduledProcessSpec,
    now: DateTime<Utc>,
) -> ScheduledProcessStatusResponse {
    let mut incomplete_reasons = Vec::new();
    if spec
        .owner
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none()
    {
        incomplete_reasons.push("missing_owner".to_string());
    }
    if spec.completion_deadline.is_none() {
        incomplete_reasons.push("missing_deadline".to_string());
    }
    if spec.expected_start.is_none() {
        incomplete_reasons.push("missing_expected_start".to_string());
    }

    let registration_status = if incomplete_reasons.is_empty() {
        "complete".to_string()
    } else {
        "incomplete".to_string()
    };

    let (run_status, warning) = if spec.run_completed_at.is_some() {
        ("completed".to_string(), None)
    } else if let Some(deadline) = spec.completion_deadline {
        if now >= deadline {
            (
                "missed_deadline".to_string(),
                Some("Completion deadline passed".to_string()),
            )
        } else if spec.run_started_at.is_none()
            && spec
                .expected_start
                .is_some_and(|start| now >= start + Duration::seconds(spec.start_grace_seconds))
        {
            (
                "missed_start".to_string(),
                Some("Process has not started within expected window before cut-off".to_string()),
            )
        } else if spec.run_started_at.is_some()
            && spec.estimated_finish_at.is_some_and(|est| est > deadline)
        {
            (
                "at_risk".to_string(),
                Some("Delayed run is projected to miss completion deadline".to_string()),
            )
        } else {
            ("on_track".to_string(), None)
        }
    } else {
        ("unscheduled".to_string(), None)
    };

    ScheduledProcessStatusResponse {
        process_id: spec.process_id.clone(),
        name: spec.name.clone(),
        criticality: spec.criticality.clone(),
        expected_start: spec.expected_start,
        completion_deadline: spec.completion_deadline,
        owner: spec.owner.clone(),
        registration_status,
        incomplete_reasons,
        run_status,
        warning,
    }
}

pub async fn register_scheduled_process(
    source: Extension<Arc<dyn BusinessHealthSource>>,
    tenant: Extension<TenantContext>,
    Json(spec): Json<ScheduledProcessSpec>,
) -> Response {
    let evaluated = evaluate_scheduled(&spec, Utc::now());
    source
        .upsert_scheduled_process(tenant.tenant_id, spec)
        .await;
    (StatusCode::CREATED, Json(evaluated)).into_response()
}

pub async fn get_scheduled_process_status(
    source: Extension<Arc<dyn BusinessHealthSource>>,
    tenant: Extension<TenantContext>,
    Query(q): Query<ProcessQuery>,
) -> Response {
    let Ok(Some(spec)) = source
        .get_scheduled_process(tenant.tenant_id, &q.process_id)
        .await
    else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "scheduled_process_not_found" })),
        )
            .into_response();
    };
    let now = q.now.unwrap_or_else(Utc::now);
    (StatusCode::OK, Json(evaluate_scheduled(&spec, now))).into_response()
}

/// Channel-level health observation for distinguishing channel-specific issues (OBS-05-03).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelHealth {
    pub channel: String,
    pub error_rate: f64,
    pub degraded: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelBreakdown {
    pub degraded_channels: Vec<String>,
    pub healthy_channels: Vec<String>,
    pub channel_specific: bool,
}

pub fn isolate_channel_degradation(observations: &[ChannelHealth]) -> ChannelBreakdown {
    let degraded_channels: Vec<String> = observations
        .iter()
        .filter(|o| o.degraded || o.error_rate > 0.05)
        .map(|o| o.channel.clone())
        .collect();
    let healthy_channels: Vec<String> = observations
        .iter()
        .filter(|o| !o.degraded && o.error_rate <= 0.05)
        .map(|o| o.channel.clone())
        .collect();
    let channel_specific = !degraded_channels.is_empty() && !healthy_channels.is_empty();
    ChannelBreakdown {
        degraded_channels,
        healthy_channels,
        channel_specific,
    }
}

/// Verifies journey evidence contains no prohibited personal content keys (OBS-05-05).
pub fn audit_journey_evidence_privacy(fields: &[(&str, &str)]) -> Result<(), &'static str> {
    const PROHIBITED: &[&str] = &["email", "ssn", "pan", "phone", "full_name"];
    for (k, _) in fields {
        if PROHIBITED.contains(&k.to_ascii_lowercase().as_str()) {
            return Err("prohibited_personal_content");
        }
    }
    Ok(())
}

/// Mobile release crash telemetry (OBS-05-07).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MobileVersionTelemetry {
    pub version: String,
    pub is_new_release: bool,
    pub crash_rate: f64,
}

pub fn detect_mobile_version_crash_regression(
    versions: &[MobileVersionTelemetry],
    baseline_crash_rate: f64,
) -> Vec<String> {
    versions
        .iter()
        .filter(|v| v.is_new_release && v.crash_rate > baseline_crash_rate * 2.0)
        .map(|v| v.version.clone())
        .collect()
}

/// Journey degradation paired with support contact volume delta (OBS-05-08).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JourneySupportCorrelation {
    pub journey_id: String,
    pub degradation_rate: f64,
    pub support_contact_increase_pct: f64,
    pub correlated: bool,
}

pub fn correlate_journey_with_support_contacts(
    journey_id: &str,
    degradation_rate: f64,
    support_contact_increase_pct: f64,
) -> JourneySupportCorrelation {
    JourneySupportCorrelation {
        journey_id: journey_id.to_string(),
        degradation_rate,
        support_contact_increase_pct,
        correlated: degradation_rate > 0.0 && support_contact_increase_pct > 0.0,
    }
}

/// Outcome classification separating valid business rejections from operational outages (OBS-12-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessOutcomeClassification {
    pub outcome_kind: String,
    pub is_outage: bool,
}

pub fn classify_process_outcome(outcome_kind: &str) -> ProcessOutcomeClassification {
    let is_outage = matches!(
        outcome_kind,
        "system_error" | "timeout" | "dependency_failure"
    );
    ProcessOutcomeClassification {
        outcome_kind: outcome_kind.to_string(),
        is_outage,
    }
}

/// Product-level health summary (OBS-12-04).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductOutcomeSummary {
    pub product: String,
    pub affected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductHealthBreakdown {
    pub affected_products: Vec<String>,
    pub unaffected_products: Vec<String>,
}

pub fn partition_product_health(products: &[ProductOutcomeSummary]) -> ProductHealthBreakdown {
    ProductHealthBreakdown {
        affected_products: products
            .iter()
            .filter(|p| p.affected)
            .map(|p| p.product.clone())
            .collect(),
        unaffected_products: products
            .iter()
            .filter(|p| !p.affected)
            .map(|p| p.product.clone())
            .collect(),
    }
}

/// Approved business impact indicator (OBS-12-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusinessImpactIndicator {
    pub affected_transactions: u64,
    pub indicator_unit: String,
}

pub fn summarize_business_impact_indicator(
    affected_transactions: u64,
    indicator_unit: &str,
) -> BusinessImpactIndicator {
    BusinessImpactIndicator {
        affected_transactions,
        indicator_unit: indicator_unit.to_string(),
    }
}

/// Queue backlog age summary (OBS-32-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BacklogSummary {
    pub items_queued: u64,
    pub oldest_item_age_secs: i64,
}

pub fn compute_backlog_age(
    items_queued: u64,
    oldest_enqueued_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> BacklogSummary {
    BacklogSummary {
        items_queued,
        oldest_item_age_secs: (now - oldest_enqueued_at).num_seconds().max(0),
    }
}

/// Missing inbound file notification (OBS-32-04).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingInboundAlert {
    pub file_id: String,
    pub owner: String,
    pub overdue_secs: i64,
}

pub fn detect_missing_inbound_file(
    file_id: &str,
    owner: &str,
    expected_by: DateTime<Utc>,
    received_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Option<MissingInboundAlert> {
    if received_at.is_none() && now > expected_by {
        Some(MissingInboundAlert {
            file_id: file_id.to_string(),
            owner: owner.to_string(),
            overdue_secs: (now - expected_by).num_seconds(),
        })
    } else {
        None
    }
}

/// Downstream deadline at risk due to late settlement (OBS-32-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependentDeadlineRisk {
    pub process_name: String,
    pub deadline: DateTime<Utc>,
    pub at_risk: bool,
}

pub fn evaluate_late_settlement_risk(
    settlement_eta: DateTime<Utc>,
    dependents: &[(&str, DateTime<Utc>)],
) -> Vec<DependentDeadlineRisk> {
    dependents
        .iter()
        .map(|(name, deadline)| DependentDeadlineRisk {
            process_name: (*name).to_string(),
            deadline: *deadline,
            at_risk: settlement_eta > *deadline,
        })
        .collect()
}

#[cfg(test)]
mod p1_business_health_tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn obs_05_03_channel_specific_issue_distinguishable() {
        let res = isolate_channel_degradation(&[
            ChannelHealth {
                channel: "mobile-ios".to_string(),
                error_rate: 0.25,
                degraded: true,
            },
            ChannelHealth {
                channel: "web".to_string(),
                error_rate: 0.001,
                degraded: false,
            },
        ]);
        assert!(res.channel_specific);
        assert_eq!(res.degraded_channels, vec!["mobile-ios"]);
        assert_eq!(res.healthy_channels, vec!["web"]);
    }

    #[test]
    fn obs_05_05_privacy_review_rejects_prohibited_personal_content() {
        assert!(
            audit_journey_evidence_privacy(&[("journey", "checkout"), ("step", "pay")]).is_ok()
        );
        assert!(audit_journey_evidence_privacy(&[("email", "a@b.com")]).is_err());
    }

    #[test]
    fn obs_05_07_mobile_version_elevated_crash_frequency_distinguishable() {
        let flagged = detect_mobile_version_crash_regression(
            &[
                MobileVersionTelemetry {
                    version: "3.1.0".to_string(),
                    is_new_release: false,
                    crash_rate: 0.005,
                },
                MobileVersionTelemetry {
                    version: "3.2.0".to_string(),
                    is_new_release: true,
                    crash_rate: 0.04,
                },
            ],
            0.005,
        );
        assert_eq!(flagged, vec!["3.2.0"]);
    }

    #[test]
    fn obs_05_08_support_contacts_visible_alongside_degradation() {
        let corr = correlate_journey_with_support_contacts("checkout", 0.12, 48.0);
        assert!(corr.correlated);
        assert_eq!(corr.support_contact_increase_pct, 48.0);
    }

    #[test]
    fn obs_12_03_business_rejection_not_counted_as_outage() {
        assert!(!classify_process_outcome("business_rejection").is_outage);
        assert!(classify_process_outcome("system_error").is_outage);
    }

    #[test]
    fn obs_12_04_product_specific_issue_separated_from_unaffected() {
        let breakdown = partition_product_health(&[
            ProductOutcomeSummary {
                product: "instant-wires".to_string(),
                affected: true,
            },
            ProductOutcomeSummary {
                product: "cards".to_string(),
                affected: false,
            },
        ]);
        assert_eq!(breakdown.affected_products, vec!["instant-wires"]);
        assert_eq!(breakdown.unaffected_products, vec!["cards"]);
    }

    #[test]
    fn obs_12_05_report_shows_affected_transactions_indicator() {
        let ind = summarize_business_impact_indicator(142, "affected_transactions");
        assert_eq!(ind.affected_transactions, 142);
        assert_eq!(ind.indicator_unit, "affected_transactions");
    }

    #[test]
    fn obs_32_03_backlog_visible_with_oldest_item_age() {
        let now = Utc::now();
        let summary = compute_backlog_age(500, now - Duration::seconds(3600), now);
        assert_eq!(summary.items_queued, 500);
        assert_eq!(summary.oldest_item_age_secs, 3600);
    }

    #[test]
    fn obs_32_04_missing_inbound_file_reported_to_owner() {
        let now = Utc::now();
        let alert = detect_missing_inbound_file(
            "clearing-batch-09",
            "settlement-ops",
            now - Duration::minutes(15),
            None,
            now,
        )
        .expect("missing file alert");
        assert_eq!(alert.owner, "settlement-ops");
        assert_eq!(alert.overdue_secs, 900);
    }

    #[test]
    fn obs_32_05_late_settlement_shows_dependent_deadlines_at_risk() {
        let now = Utc::now();
        let eta = now + Duration::minutes(40);
        let risks = evaluate_late_settlement_risk(
            eta,
            &[
                ("gl-posting", now + Duration::minutes(20)),
                ("archive-export", now + Duration::minutes(90)),
            ],
        );
        assert!(risks[0].at_risk);
        assert!(!risks[1].at_risk);
    }
}
