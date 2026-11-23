//! Operational context exchange helpers not yet behind a route (W2c of
//! docs/plans/wire-unrouted-capabilities.md): OBS-16-03, 16-07, 16-08 and 16-09.
//! The routed exchange (hand-off, export, notification, investigation hand-off) lives in
//! `crate::response`.

use serde::{Deserialize, Serialize};

/// Service visibility entry combining internal and outsourced critical services (OBS-16-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceVisibilityEntry {
    pub service_name: String,
    pub ownership: String, // "internal" | "outsourced"
    pub business_critical: bool,
}

pub fn merge_service_visibility_catalog(
    internal: Vec<ServiceVisibilityEntry>,
    outsourced: Vec<ServiceVisibilityEntry>,
) -> Vec<ServiceVisibilityEntry> {
    let mut merged = internal;
    merged.extend(outsourced);
    merged
}

/// Least-privilege automation status read check (OBS-16-07).
pub fn enforce_automation_scoped_status_read(role: &str, requested_action: &str) -> bool {
    role == "status-reader-automation" && requested_action == "read:service-status"
}

/// Shared security degradation view for security + operations teams (OBS-16-08).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedSecurityIncidentView {
    pub service_name: String,
    pub visible_to_roles: Vec<String>,
}

pub fn share_security_degradation_view(service_name: &str) -> SharedSecurityIncidentView {
    SharedSecurityIncidentView {
        service_name: service_name.to_string(),
        visible_to_roles: vec!["security".to_string(), "operations".to_string()],
    }
}

/// Standard portable objective + evidence bundle (OBS-16-09).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StandardObjectiveBundle {
    pub schema: String,
    pub objective_name: String,
    pub target_pct_bps: u32,
    pub evidence_summary: String,
}

pub fn export_standard_objective_bundle(
    objective_name: &str,
    target_pct_bps: u32,
    evidence_summary: &str,
) -> StandardObjectiveBundle {
    StandardObjectiveBundle {
        schema: "openslo/v1".to_string(),
        objective_name: objective_name.to_string(),
        target_pct_bps,
        evidence_summary: evidence_summary.to_string(),
    }
}

#[cfg(test)]
mod p1_context_exchange_tests {
    use super::*;

    #[test]
    fn obs_16_03_outsourced_critical_service_visible_alongside_internal() {
        let merged = merge_service_visibility_catalog(
            vec![ServiceVisibilityEntry {
                service_name: "checkout".to_string(),
                ownership: "internal".to_string(),
                business_critical: true,
            }],
            vec![ServiceVisibilityEntry {
                service_name: "card-processor-vendor".to_string(),
                ownership: "outsourced".to_string(),
                business_critical: true,
            }],
        );
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[1].ownership, "outsourced");
    }

    #[test]
    fn obs_16_07_automation_scoped_to_service_status_only() {
        assert!(enforce_automation_scoped_status_read(
            "status-reader-automation",
            "read:service-status"
        ));
        assert!(!enforce_automation_scoped_status_read(
            "status-reader-automation",
            "read:raw-pii-logs"
        ));
    }

    #[test]
    fn obs_16_08_suspected_attack_visible_to_security_and_operations() {
        let view = share_security_degradation_view("api-edge");
        assert!(view.visible_to_roles.contains(&"security".to_string()));
        assert!(view.visible_to_roles.contains(&"operations".to_string()));
    }

    #[test]
    fn obs_16_09_objective_and_evidence_exported_in_standard_form() {
        let bundle = export_standard_objective_bundle("checkout-availability", 9990, "99.94% 28d");
        let json = serde_json::to_string(&bundle).expect("serialize");
        let roundtrip: StandardObjectiveBundle = serde_json::from_str(&json).expect("parse");
        assert_eq!(roundtrip.schema, "openslo/v1");
        assert_eq!(roundtrip.objective_name, "checkout-availability");
    }
}
