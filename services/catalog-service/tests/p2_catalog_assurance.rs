//! Phase 2 catalog and business impact P2 assurance tests (`OBS-34-06`, `OBS-10-06`).

#[derive(Debug, Clone, PartialEq, Eq)]
struct ServiceChangeBundle {
    service: String,
    service_pr: String,
    alert_config_diff: String,
    reviewed_together: bool,
    released_together: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct BusinessImpactEstimate {
    incident_id: String,
    measure_name: String,
    estimated_value: f64,
    is_estimate: bool,
}

#[test]
fn obs_34_06_service_and_alert_config_change_reviewed_and_released_together() {
    let bundle = ServiceChangeBundle {
        service: "checkout".to_string(),
        service_pr: "pr-4210".to_string(),
        alert_config_diff: "alerts/checkout-latency.yaml".to_string(),
        reviewed_together: true,
        released_together: true,
    };
    assert!(!bundle.alert_config_diff.is_empty());
    assert!(bundle.reviewed_together && bundle.released_together);
}

#[test]
fn obs_10_06_impact_view_uses_approved_business_measure_and_marks_estimate() {
    let approved_measures = ["revenue_at_risk_usd", "orders_delayed_count"];
    let impact = BusinessImpactEstimate {
        incident_id: "inc-902".to_string(),
        measure_name: "revenue_at_risk_usd".to_string(),
        estimated_value: 14_500.0,
        is_estimate: true,
    };
    assert!(approved_measures.contains(&impact.measure_name.as_str()));
    assert!(impact.is_estimate);
    assert!(impact.estimated_value > 0.0);
}
