//! Phase 2 alerting, contractual SLA, predictive warning, and procedure execution assurance (`OBS-06-10`, `OBS-07-12`, `OBS-33-05`).

/// Data classification label distinguishing measured evidence from estimates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataBasisLabel {
    Evidence,
    Estimate,
}

/// Contractual SLA review outcome for a billing/reporting period (`OBS-06-10`).
#[derive(Debug, Clone, PartialEq)]
pub struct ContractualSlaReview {
    pub contract_id: String,
    pub period: String,
    pub target_attainment: f64,
    pub observed_attainment: f64,
    pub obligation_credit_usd: f64,
    pub basis_label: DataBasisLabel,
}

/// Evaluate contractual SLA attainment and resulting credit/penalty obligation, labeled as evidence or estimate.
pub fn evaluate_contractual_sla(
    contract_id: &str,
    period: &str,
    target_attainment: f64,
    observed_attainment: f64,
    monthly_fee_usd: f64,
    is_measured_evidence: bool,
) -> ContractualSlaReview {
    let shortfall = (target_attainment - observed_attainment).max(0.0);
    let obligation_credit_usd = if shortfall > 0.0 {
        (monthly_fee_usd * (shortfall * 10.0)).min(monthly_fee_usd)
    } else {
        0.0
    };
    ContractualSlaReview {
        contract_id: contract_id.to_string(),
        period: period.to_string(),
        target_attainment,
        observed_attainment,
        obligation_credit_usd,
        basis_label: if is_measured_evidence {
            DataBasisLabel::Evidence
        } else {
            DataBasisLabel::Estimate
        },
    }
}

/// Predictive capacity/breach warning labeled as a forecast with its basis disclosed (`OBS-07-12`).
#[derive(Debug, Clone, PartialEq)]
pub struct PredictiveWarning {
    pub resource: String,
    pub label: &'static str,
    pub predicted_utilization_pct: f64,
    pub threshold_pct: f64,
    pub horizon_hours: u32,
    pub basis_disclosure: String,
}

/// Generate a predictive breach warning when projected utilization reaches or exceeds the threshold.
pub fn evaluate_capacity_forecast(
    resource: &str,
    current_pct: f64,
    hourly_growth_pct: f64,
    horizon_hours: u32,
    threshold_pct: f64,
) -> Option<PredictiveWarning> {
    let predicted = current_pct + hourly_growth_pct * f64::from(horizon_hours);
    if predicted >= threshold_pct {
        Some(PredictiveWarning {
            resource: resource.to_string(),
            label: "forecast",
            predicted_utilization_pct: predicted,
            threshold_pct,
            horizon_hours,
            basis_disclosure: format!(
                "linear trend +{hourly_growth_pct:.2}%/h from {current_pct:.1}% over {horizon_hours}h"
            ),
        })
    } else {
        None
    }
}

/// Recorded execution step for an approved operational procedure (`OBS-33-05`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureStepRecord {
    pub step_name: String,
    pub actor: String,
    pub inputs: String,
    pub outputs: String,
    pub result: String,
}

/// Audit log for an approved, executable operational procedure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutedProcedure {
    pub procedure_id: String,
    pub approved_by: String,
    pub steps: Vec<ProcedureStepRecord>,
}

/// Execute an operational procedure step only when automation is approved, recording actor, inputs, outputs, and result.
pub fn record_procedure_step(
    procedure: &mut ExecutedProcedure,
    automation_permitted: bool,
    step_name: &str,
    actor: &str,
    inputs: &str,
    outputs: &str,
    result: &str,
) -> Result<(), &'static str> {
    if !automation_permitted || procedure.approved_by.trim().is_empty() {
        return Err("procedure automation not approved");
    }
    procedure.steps.push(ProcedureStepRecord {
        step_name: step_name.to_string(),
        actor: actor.to_string(),
        inputs: inputs.to_string(),
        outputs: outputs.to_string(),
        result: result.to_string(),
    });
    Ok(())
}

#[cfg(test)]
mod p2_alerting_tests {
    use super::*;

    #[test]
    fn obs_06_10_contractual_sla_review_labels_evidence_and_estimate() {
        let measured = evaluate_contractual_sla("ctr-1", "2026-10", 0.999, 0.995, 10_000.0, true);
        assert_eq!(measured.basis_label, DataBasisLabel::Evidence);
        assert!(measured.obligation_credit_usd > 0.0);

        let estimated =
            evaluate_contractual_sla("ctr-1", "2026-11-mtd", 0.999, 0.9995, 10_000.0, false);
        assert_eq!(estimated.basis_label, DataBasisLabel::Estimate);
        assert_eq!(estimated.obligation_credit_usd, 0.0);
    }

    #[test]
    fn obs_07_12_predictive_capacity_warning_is_labeled_forecast_with_basis() {
        let warn = evaluate_capacity_forecast("clickhouse-disk", 78.0, 2.5, 6, 90.0)
            .expect("should forecast breach");
        assert_eq!(warn.label, "forecast");
        assert!(warn.predicted_utilization_pct >= 90.0);
        assert!(warn.basis_disclosure.contains("linear trend"));
    }

    #[test]
    fn obs_33_05_executed_procedure_records_step_actor_and_result() {
        let mut proc = ExecutedProcedure {
            procedure_id: "rb-drain-node".to_string(),
            approved_by: "sre-lead".to_string(),
            steps: Vec::new(),
        };
        assert!(
            record_procedure_step(
                &mut proc, false, "drain", "bot", "node=n1", "denied", "blocked"
            )
            .is_err()
        );
        record_procedure_step(
            &mut proc,
            true,
            "drain",
            "oncall-1",
            "node=n1",
            "drained_pods=12",
            "ok",
        )
        .unwrap();
        assert_eq!(proc.steps.len(), 1);
        assert_eq!(proc.steps[0].actor, "oncall-1");
        assert_eq!(proc.steps[0].result, "ok");
    }
}
