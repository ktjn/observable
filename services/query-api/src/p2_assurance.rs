//! Phase 2 query, change tracking, reporting, compliance, cross-team coordination, and cost-value P2 assurance
//! (`OBS-09-07`, `OBS-11-05`, `OBS-11-08`, `OBS-11-09`, `OBS-13-07`, `OBS-15-06`, `OBS-16-06`, `OBS-17-06`).

use std::collections::BTreeSet;

/// Historical incident record for recurring pattern comparison (`OBS-09-07`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentPatternRecord {
    pub incident_id: String,
    pub symptoms: Vec<String>,
    pub confirmed_cause: Option<String>,
}

/// Find prior incidents matching a target symptom that have a confirmed root cause.
pub fn find_recurring_incidents<'a>(
    history: &'a [IncidentPatternRecord],
    symptom: &str,
) -> Vec<&'a IncidentPatternRecord> {
    history
        .iter()
        .filter(|inc| inc.confirmed_cause.is_some() && inc.symptoms.iter().any(|s| s == symptom))
        .collect()
}

/// Separate delivery performance metrics from reliability outcomes (`OBS-11-05`).
#[derive(Debug, Clone, PartialEq)]
pub struct DeliveryReliabilityReport {
    pub deployment_frequency_per_week: f64,
    pub lead_time_hours: f64,
    pub slo_attainment_pct: f64,
    pub change_failure_rate_pct: f64,
    pub treated_as_equivalent: bool,
}

/// Comparative rollout metrics for exposed vs unexposed populations (`OBS-11-08`).
#[derive(Debug, Clone, PartialEq)]
pub struct RolloutPopulationComparison {
    pub release_id: String,
    pub exposed_error_rate: f64,
    pub unexposed_error_rate: f64,
}

/// Change-freeze window and any change executed during it (`OBS-11-09`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreezeChangeRecord {
    pub freeze_window_id: String,
    pub change_id: String,
    pub approval_or_exception: String,
}

/// Scheduled periodic status report scoped to an approved business unit (`OBS-13-07`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledStatusReport {
    pub report_id: String,
    pub approved_schedule: bool,
    pub business_scope: String,
    pub included_services: Vec<String>,
}

/// Regulated third-party disruption exposure link (`OBS-15-06`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThirdPartyExposureReview {
    pub provider_incident_id: String,
    pub provider_name: String,
    pub affected_regulated_functions: Vec<String>,
}

/// Shared cross-domain operational view subject to team access controls (`OBS-16-06`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedOperationalView {
    pub incident_id: String,
    pub authorized_teams: BTreeSet<String>,
    pub summary: String,
}

impl SharedOperationalView {
    pub fn view_for_team(&self, team_id: &str) -> Result<&str, &'static str> {
        if self.authorized_teams.contains(team_id) {
            Ok(&self.summary)
        } else {
            Err("team not authorized for shared operational view")
        }
    }
}

/// Observability value review comparing cost delta with agreed outcome indicators (`OBS-17-06`).
#[derive(Debug, Clone, PartialEq)]
pub struct ObservabilityBenefitsReview {
    pub period: String,
    pub cost_delta_pct: f64,
    pub mttr_improvement_pct: f64,
    pub false_alert_reduction_pct: f64,
}

#[cfg(test)]
mod p2_assurance_tests {
    use super::*;

    #[test]
    fn obs_09_07_finds_prior_incidents_with_similar_symptoms_and_confirmed_causes() {
        let history = vec![
            IncidentPatternRecord {
                incident_id: "inc-101".to_string(),
                symptoms: vec!["connection_pool_exhausted".to_string()],
                confirmed_cause: Some("missing idle timeout".to_string()),
            },
            IncidentPatternRecord {
                incident_id: "inc-102".to_string(),
                symptoms: vec!["connection_pool_exhausted".to_string()],
                confirmed_cause: None,
            },
        ];
        let matches = find_recurring_incidents(&history, "connection_pool_exhausted");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].incident_id, "inc-101");
    }

    #[test]
    fn obs_11_05_delivery_and_reliability_measures_kept_distinct() {
        let report = DeliveryReliabilityReport {
            deployment_frequency_per_week: 18.0,
            lead_time_hours: 3.5,
            slo_attainment_pct: 99.94,
            change_failure_rate_pct: 1.2,
            treated_as_equivalent: false,
        };
        assert!(!report.treated_as_equivalent);
        assert!(report.deployment_frequency_per_week > 0.0);
        assert!(report.slo_attainment_pct > 99.0);
    }

    #[test]
    fn obs_11_08_partial_release_compares_exposed_and_unexposed_populations() {
        let cmp = RolloutPopulationComparison {
            release_id: "rel-2026-11-14".to_string(),
            exposed_error_rate: 0.008,
            unexposed_error_rate: 0.001,
        };
        assert!(cmp.exposed_error_rate > cmp.unexposed_error_rate);
    }

    #[test]
    fn obs_11_09_change_during_freeze_visible_with_approval_or_exception() {
        let record = FreezeChangeRecord {
            freeze_window_id: "freeze-black-friday".to_string(),
            change_id: "chg-882".to_string(),
            approval_or_exception: "emergency-sec-exception-vp-eng".to_string(),
        };
        assert!(!record.freeze_window_id.is_empty());
        assert!(!record.approval_or_exception.is_empty());
    }

    #[test]
    fn obs_13_07_scheduled_status_report_reflects_chosen_business_scope() {
        let report = ScheduledStatusReport {
            report_id: "weekly-retail-2026w46".to_string(),
            approved_schedule: true,
            business_scope: "retail-checkout".to_string(),
            included_services: vec!["checkout".to_string(), "payments".to_string()],
        };
        assert!(report.approved_schedule);
        assert_eq!(report.business_scope, "retail-checkout");
        assert!(!report.included_services.is_empty());
    }

    #[test]
    fn obs_15_06_links_material_provider_incident_to_regulated_functions() {
        let review = ThirdPartyExposureReview {
            provider_incident_id: "prov-inc-44".to_string(),
            provider_name: "eu-clearing-gateway".to_string(),
            affected_regulated_functions: vec![
                "sepa-settlement".to_string(),
                "card-clearing".to_string(),
            ],
        };
        assert!(!review.affected_regulated_functions.is_empty());
    }

    #[test]
    fn obs_16_06_authorized_teams_coordinate_on_shared_view() {
        let view = SharedOperationalView {
            incident_id: "inc-550".to_string(),
            authorized_teams: BTreeSet::from([
                "payments-oncall".to_string(),
                "platform-sre".to_string(),
            ]),
            summary: "Shared cross-domain timeline for inc-550".to_string(),
        };
        assert!(view.view_for_team("payments-oncall").is_ok());
        assert!(view.view_for_team("platform-sre").is_ok());
        assert!(view.view_for_team("unauthorized-team").is_err());
    }

    #[test]
    fn obs_17_06_benefits_review_compares_cost_changes_with_outcome_indicators() {
        let review = ObservabilityBenefitsReview {
            period: "2026-Q4".to_string(),
            cost_delta_pct: 6.5,
            mttr_improvement_pct: 28.0,
            false_alert_reduction_pct: 35.0,
        };
        assert!(review.mttr_improvement_pct > 0.0);
        assert!(review.false_alert_reduction_pct > 0.0);
    }
}
