//! Phase 2 reporting, compliance and cross-team coordination assurance (`OBS-13-07`, `OBS-15-06`,
//! `OBS-16-06`), pending relocation to their providers in W5b (docs/assurance-relocations.md).
//! OBS-09-07, 11-05, 11-08, 11-09 and 17-06 moved to alerting and control in W5a.

use std::collections::BTreeSet;

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

#[cfg(test)]
mod p2_assurance_tests {
    use super::*;

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
}
