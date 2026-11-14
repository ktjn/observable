//! Operational assistant and autonomous incident investigation domain (`OBS-18-01..10`, `OBS-38-01..28`).

use std::collections::{BTreeMap, BTreeSet};

/// Evidence reference backing a summary, hypothesis, or decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRef {
    pub id: String,
    pub service: String,
    pub restricted: bool,
    pub summary: String,
}

/// Anomaly signal with magnitude and baseline noise threshold.
#[derive(Debug, Clone, PartialEq)]
pub struct AnomalySignal {
    pub id: String,
    pub service: String,
    pub deviation_sigma: f64,
    pub summary: String,
}

/// Summary generated for an operator, separating findings from proposed actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlainSummary {
    pub narrative: String,
    pub supporting_evidence: Vec<String>,
    pub verified_root_causes: Vec<String>,
    pub proposed_actions: Vec<String>,
}

/// Build a plain-language summary linked to authorized evidence, distinguishing verified root causes from proposed actions.
pub fn build_plain_summary(
    narrative: &str,
    evidence: &[EvidenceRef],
    allowed_services: &BTreeSet<String>,
    verified_root_causes: Vec<String>,
    proposed_actions: Vec<String>,
) -> PlainSummary {
    let supporting_evidence = evidence
        .iter()
        .filter(|e| !e.restricted && allowed_services.contains(&e.service))
        .map(|e| e.id.clone())
        .collect();
    PlainSummary {
        narrative: narrative.to_string(),
        supporting_evidence,
        verified_root_causes,
        proposed_actions,
    }
}

/// Rank anomalies, surfacing significant signals (>= min_sigma) and deprioritizing negligible variations.
pub fn prioritize_anomalies(
    signals: &[AnomalySignal],
    min_sigma: f64,
) -> (Vec<&AnomalySignal>, Vec<&AnomalySignal>) {
    let mut significant = Vec::new();
    let mut deprioritized = Vec::new();
    for signal in signals {
        if signal.deviation_sigma >= min_sigma {
            significant.push(signal);
        } else {
            deprioritized.push(signal);
        }
    }
    significant.sort_by(|a, b| b.deviation_sigma.total_cmp(&a.deviation_sigma));
    (significant, deprioritized)
}

/// Action impact level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionImpact {
    Low,
    High,
}

/// Proposed remediation action with safety, blast radius, and rollback metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionProposal {
    pub id: String,
    pub target: String,
    pub impact: ActionImpact,
    pub expected_effect: String,
    pub blast_radius: String,
    pub prerequisites: Vec<String>,
    pub recovery_route: String,
    pub approved_by: Option<String>,
    pub approved_target: Option<String>,
}

/// Validate that a proposal specifies effect, blast radius, prerequisites, and a recovery route.
pub fn validate_proposal(proposal: &ActionProposal) -> Result<(), &'static str> {
    if proposal.expected_effect.trim().is_empty()
        || proposal.blast_radius.trim().is_empty()
        || proposal.prerequisites.is_empty()
    {
        return Err("proposal missing expected effect, blast radius, or prerequisites");
    }
    if proposal.recovery_route.trim().is_empty() {
        return Err("remediation without recovery route cannot be executed");
    }
    Ok(())
}

/// Outcome of executing an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionOutcome {
    Recovered,
    Failed,
}

/// Immutable audit record for an action execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionAuditRecord {
    pub proposal_id: String,
    pub approved_by: String,
    pub action_executed: String,
    pub outcome: ExecutionOutcome,
}

/// Post-action telemetry used to verify recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostActionTelemetry {
    pub telemetry_present: bool,
    pub recovery_criteria_passed: bool,
}

/// Evaluate recovery status from post-action telemetry rather than command exit status alone.
pub fn evaluate_recovery(
    command_succeeded: bool,
    telemetry: PostActionTelemetry,
) -> ExecutionOutcome {
    if command_succeeded && telemetry.telemetry_present && telemetry.recovery_criteria_passed {
        ExecutionOutcome::Recovered
    } else {
        ExecutionOutcome::Failed
    }
}

/// Global/tenant automated action execution switch.
#[derive(Debug, Clone, Default)]
pub struct AutomationControl {
    pub automated_actions_enabled: bool,
}

impl AutomationControl {
    pub fn disable_automated_actions(
        &mut self,
        operator_authorized: bool,
    ) -> Result<(), &'static str> {
        if !operator_authorized {
            return Err("unauthorized operator");
        }
        self.automated_actions_enabled = false;
        Ok(())
    }
}

/// Execute an action proposal if validated, enabled, and properly approved for the exact target, writing an audit record.
pub fn execute_proposal(
    proposal: &ActionProposal,
    designated_approver: &str,
    automation: &AutomationControl,
    command_succeeded: bool,
    telemetry: PostActionTelemetry,
) -> Result<ActionAuditRecord, &'static str> {
    if !automation.automated_actions_enabled {
        return Err("automated action path disabled");
    }
    validate_proposal(proposal)?;
    if proposal.impact == ActionImpact::High {
        let Some(approver) = proposal.approved_by.as_deref() else {
            return Err("high-impact action requires approval");
        };
        if approver != designated_approver {
            return Err("approver is not designated");
        }
        if proposal.approved_target.as_deref() != Some(proposal.target.as_str()) {
            return Err("approval target mismatch");
        }
    }
    let outcome = evaluate_recovery(command_succeeded, telemetry);
    Ok(ActionAuditRecord {
        proposal_id: proposal.id.clone(),
        approved_by: proposal
            .approved_by
            .clone()
            .unwrap_or_else(|| "auto-low-impact".to_string()),
        action_executed: format!("remediate:{}", proposal.target),
        outcome,
    })
}

/// Recommendation decision outcome for review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecommendationOutcome {
    Accepted,
    Rejected,
    Inaccurate,
}

/// Logged recommendation decision tied to an incident and operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecommendationDecision {
    pub id: String,
    pub incident_id: String,
    pub assistant_output_id: String,
    pub decided_by: String,
    pub outcome: RecommendationOutcome,
    pub adjudicated_correct: Option<bool>,
    pub false_positive: bool,
    pub unsafe_attempt_denied: bool,
}

/// Summary metrics over assistant recommendations and adjudicated incidents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantReviewReport {
    pub accepted: usize,
    pub rejected: usize,
    pub inaccurate: usize,
    pub successful_adjudicated: usize,
    pub false_positives: usize,
    pub denied_unsafe_attempts: usize,
}

/// Summarize accepted/rejected/inaccurate recommendations; never counts inaccurate or unadjudicated items as successful.
pub fn summarize_recommendations(decisions: &[RecommendationDecision]) -> AssistantReviewReport {
    let mut report = AssistantReviewReport {
        accepted: 0,
        rejected: 0,
        inaccurate: 0,
        successful_adjudicated: 0,
        false_positives: 0,
        denied_unsafe_attempts: 0,
    };
    for d in decisions {
        match d.outcome {
            RecommendationOutcome::Accepted => report.accepted += 1,
            RecommendationOutcome::Rejected => report.rejected += 1,
            RecommendationOutcome::Inaccurate => report.inaccurate += 1,
        }
        if d.adjudicated_correct == Some(true) && d.outcome != RecommendationOutcome::Inaccurate {
            report.successful_adjudicated += 1;
        }
        if d.false_positive {
            report.false_positives += 1;
        }
        if d.unsafe_attempt_denied {
            report.denied_unsafe_attempts += 1;
        }
    }
    report
}

/// Period evaluation point with method identifier for trend comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct PeriodMeasure {
    pub period: String,
    pub accuracy_rate: f64,
    pub evaluation_method: String,
}

/// Trend point annotated with whether the evaluation method changed from the prior period.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotatedTrendPoint {
    pub period: String,
    pub accuracy_rate: f64,
    pub method_changed: bool,
}

/// Build evaluation trend across periods, flagging any evaluation method change.
pub fn build_evaluation_trend(measures: &[PeriodMeasure]) -> Vec<AnnotatedTrendPoint> {
    let mut out = Vec::with_capacity(measures.len());
    let mut prev_method: Option<&str> = None;
    for m in measures {
        let method_changed = prev_method.is_some_and(|prev| prev != m.evaluation_method);
        out.push(AnnotatedTrendPoint {
            period: m.period.clone(),
            accuracy_rate: m.accuracy_rate,
            method_changed,
        });
        prev_method = Some(&m.evaluation_method);
    }
    out
}

/// Step within an agent action plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStep {
    pub id: String,
    pub tool_name: String,
    pub capability: String,
    pub approved: bool,
    pub executed: bool,
}

/// Lifecycle state of an autonomous investigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvestigationState {
    Active,
    Paused,
    Cancelled,
    Completed,
    StoppedIncomplete,
}

/// Hypothesis kind evaluated during investigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HypothesisKind {
    Deployment,
    Database,
}

/// Hypothesis with supporting and contradicting citations.
#[derive(Debug, Clone, PartialEq)]
pub struct Hypothesis {
    pub kind: HypothesisKind,
    pub confidence: f64,
    pub supporting_observations: Vec<String>,
    pub contradicting_observations: Vec<String>,
    pub missing_evidence: Vec<String>,
    pub refuted: bool,
    pub confirmed: bool,
}

impl Hypothesis {
    /// Mark confirmed only when there is no unresolved contradicting evidence.
    pub fn confirm(&mut self) -> Result<(), &'static str> {
        if !self.contradicting_observations.is_empty() {
            return Err("unresolved contradicting evidence prevents confirmation");
        }
        self.confirmed = true;
        Ok(())
    }
}

/// Triggering event for an incident investigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerEvent {
    pub event_id: String,
    pub incident_id: String,
    pub matched_rule: Option<String>,
}

/// Autonomous investigation record with scope, budget, permissions, and evidence.
#[derive(Debug, Clone)]
pub struct Investigation {
    pub id: String,
    pub incident_id: String,
    pub owner_id: String,
    pub state: InvestigationState,
    pub scope: BTreeSet<String>,
    pub granted_capabilities: BTreeSet<String>,
    pub denied_capabilities: BTreeSet<String>,
    pub triggering_events: Vec<String>,
    pub matched_rules: Vec<String>,
    pub evidence: Vec<String>,
    pub timeline: Vec<String>,
    pub hypotheses: Vec<Hypothesis>,
    pub budget_limit: usize,
    pub steps_taken: usize,
    pub retry_limit: usize,
    pub consecutive_failures: usize,
    pub audit_log: Vec<String>,
}

/// Registry managing investigations and deduplicating by incident.
#[derive(Debug, Default)]
pub struct InvestigationManager {
    pub investigations: BTreeMap<String, Investigation>,
    incident_to_investigation: BTreeMap<String, String>,
    seen_events: BTreeSet<String>,
}

impl InvestigationManager {
    /// Start or attach to an existing investigation for a qualifying incident event.
    pub fn ingest_event(
        &mut self,
        event: TriggerEvent,
        scope: BTreeSet<String>,
        granted_capabilities: BTreeSet<String>,
        budget_limit: usize,
        retry_limit: usize,
    ) -> Result<String, &'static str> {
        let Some(rule) = event.matched_rule else {
            return Err("ineligible event: no matching eligibility rule");
        };
        if event.event_id.trim().is_empty() {
            return Err("cannot create investigation without triggering event");
        }
        if let Some(existing_id) = self
            .incident_to_investigation
            .get(&event.incident_id)
            .cloned()
        {
            if self.seen_events.insert(event.event_id.clone())
                && let Some(inv) = self.investigations.get_mut(&existing_id)
            {
                inv.triggering_events.push(event.event_id);
                if !inv.matched_rules.contains(&rule) {
                    inv.matched_rules.push(rule);
                }
            }
            return Ok(existing_id);
        }
        self.seen_events.insert(event.event_id.clone());
        let inv_id = format!("inv-{}", event.incident_id);
        self.incident_to_investigation
            .insert(event.incident_id.clone(), inv_id.clone());
        self.investigations.insert(
            inv_id.clone(),
            Investigation {
                id: inv_id.clone(),
                incident_id: event.incident_id,
                owner_id: "system-agent".to_string(),
                state: InvestigationState::Active,
                scope,
                granted_capabilities,
                denied_capabilities: BTreeSet::new(),
                triggering_events: vec![event.event_id],
                matched_rules: vec![rule],
                evidence: Vec::new(),
                timeline: vec!["investigation_started".to_string()],
                hypotheses: Vec::new(),
                budget_limit,
                steps_taken: 0,
                retry_limit,
                consecutive_failures: 0,
                audit_log: Vec::new(),
            },
        );
        Ok(inv_id)
    }

    pub fn pause(&mut self, inv_id: &str) -> Result<(), &'static str> {
        let inv = self.investigations.get_mut(inv_id).ok_or("not found")?;
        inv.state = InvestigationState::Paused;
        inv.timeline.push("paused".to_string());
        Ok(())
    }

    pub fn resume(&mut self, inv_id: &str) -> Result<(), &'static str> {
        let inv = self.investigations.get_mut(inv_id).ok_or("not found")?;
        if inv.state == InvestigationState::Cancelled {
            return Err("cannot resume cancelled investigation");
        }
        inv.state = InvestigationState::Active;
        inv.timeline.push("resumed".to_string());
        Ok(())
    }

    pub fn cancel(&mut self, inv_id: &str) -> Result<(), &'static str> {
        let inv = self.investigations.get_mut(inv_id).ok_or("not found")?;
        inv.state = InvestigationState::Cancelled;
        inv.timeline.push("cancelled".to_string());
        Ok(())
    }

    pub fn takeover(
        &mut self,
        inv_id: &str,
        responder_id: &str,
        authorized: bool,
    ) -> Result<(), &'static str> {
        if !authorized {
            return Err("unauthorized responder cannot take over");
        }
        let inv = self.investigations.get_mut(inv_id).ok_or("not found")?;
        inv.owner_id = responder_id.to_string();
        inv.timeline.push(format!("takeover:{responder_id}"));
        Ok(())
    }

    /// Execute a step if active, within scope/permissions/budget/retry limits, and approved.
    pub fn execute_step(
        &mut self,
        inv_id: &str,
        step: &mut PlanStep,
        target_service: &str,
        tool_succeeded: bool,
    ) -> Result<(), &'static str> {
        let inv = self.investigations.get_mut(inv_id).ok_or("not found")?;
        if inv.state == InvestigationState::Cancelled {
            return Err("cancelled investigation performs no privileged actions");
        }
        if inv.state != InvestigationState::Active {
            return Err("investigation is not active");
        }
        if !inv.scope.contains(target_service) {
            inv.audit_log
                .push(format!("denied_out_of_scope:{target_service}"));
            return Err("action outside declared scope");
        }
        if !step.approved {
            inv.audit_log.push(format!("denied_unapproved:{}", step.id));
            return Err("step not approved");
        }
        if inv.denied_capabilities.contains(&step.capability)
            || !inv.granted_capabilities.contains(&step.capability)
        {
            inv.denied_capabilities.insert(step.capability.clone());
            inv.audit_log.push(format!(
                "denied_capability:{}:{}",
                step.tool_name, step.capability
            ));
            return Err("capability denied");
        }
        if inv.steps_taken >= inv.budget_limit {
            inv.state = InvestigationState::StoppedIncomplete;
            inv.timeline.push("stopped_incomplete_budget".to_string());
            return Err("budget reached");
        }
        if inv.consecutive_failures >= inv.retry_limit {
            inv.state = InvestigationState::StoppedIncomplete;
            inv.timeline.push("stopped_incomplete_retries".to_string());
            return Err("retry limit reached");
        }
        inv.steps_taken += 1;
        if tool_succeeded {
            inv.consecutive_failures = 0;
            step.executed = true;
            inv.timeline.push(format!("executed:{}", step.id));
            if inv.steps_taken >= inv.budget_limit {
                inv.state = InvestigationState::StoppedIncomplete;
                inv.timeline.push("stopped_incomplete_budget".to_string());
            }
            Ok(())
        } else {
            inv.consecutive_failures += 1;
            inv.timeline.push(format!("failed_tool:{}", step.tool_name));
            if inv.consecutive_failures >= inv.retry_limit {
                inv.state = InvestigationState::StoppedIncomplete;
                inv.timeline.push("stopped_incomplete_retries".to_string());
            }
            Err("tool call failed")
        }
    }
}

/// Format an executive conclusion ensuring unrefuted alternative hypotheses are disclosed.
pub fn conclusion_options(hypotheses: &[Hypothesis]) -> Vec<HypothesisKind> {
    hypotheses
        .iter()
        .filter(|h| !h.refuted)
        .map(|h| h.kind)
        .collect()
}

/// Onboarding prerequisite status for a new service owner (`OBS-19-07`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnboardingCoverageCheck {
    pub service: String,
    pub missing_prerequisites: Vec<String>,
    pub coverage_established: bool,
}

/// Evaluate whether a new service owner has all required onboarding prerequisites for baseline coverage.
pub fn evaluate_onboarding_coverage(
    service: &str,
    completed_steps: &BTreeSet<String>,
    required_prerequisites: &[&str],
) -> OnboardingCoverageCheck {
    let missing_prerequisites: Vec<String> = required_prerequisites
        .iter()
        .filter(|req| !completed_steps.contains(**req))
        .map(|req| (*req).to_string())
        .collect();
    OnboardingCoverageCheck {
        service: service.to_string(),
        coverage_established: missing_prerequisites.is_empty(),
        missing_prerequisites,
    }
}

#[cfg(test)]
mod p10_ops_assistant_tests {
    use super::*;

    fn sample_evidence() -> Vec<EvidenceRef> {
        vec![
            EvidenceRef {
                id: "ev-log-1".to_string(),
                service: "checkout".to_string(),
                restricted: false,
                summary: "502 surge on checkout".to_string(),
            },
            EvidenceRef {
                id: "ev-secret-1".to_string(),
                service: "payments-vault".to_string(),
                restricted: true,
                summary: "restricted vault trace".to_string(),
            },
        ]
    }

    #[test]
    fn obs_18_01_03_07_summary_evidence_distinction_and_access() {
        let allowed = BTreeSet::from(["checkout".to_string()]);
        let summary = build_plain_summary(
            "Checkout 502s spiked after deploy v42",
            &sample_evidence(),
            &allowed,
            vec!["missing db pool config".to_string()],
            vec!["rollback checkout to v41".to_string()],
        );
        assert_eq!(summary.supporting_evidence, vec!["ev-log-1".to_string()]);
        assert!(
            !summary
                .supporting_evidence
                .contains(&"ev-secret-1".to_string())
        );
        assert_ne!(summary.verified_root_causes, summary.proposed_actions);
    }

    #[test]
    fn obs_18_02_prioritizes_significant_anomalies() {
        let signals = vec![
            AnomalySignal {
                id: "noise".to_string(),
                service: "checkout".to_string(),
                deviation_sigma: 0.4,
                summary: "minor jitter".to_string(),
            },
            AnomalySignal {
                id: "spike".to_string(),
                service: "checkout".to_string(),
                deviation_sigma: 6.2,
                summary: "error rate spike".to_string(),
            },
        ];
        let (sig, deprio) = prioritize_anomalies(&signals, 3.0);
        assert_eq!(sig.len(), 1);
        assert_eq!(sig[0].id, "spike");
        assert_eq!(deprio.len(), 1);
        assert_eq!(deprio[0].id, "noise");
    }

    #[test]
    fn obs_18_04_06_38_05_20_21_22_23_24_25_approval_safety_and_recovery() {
        let mut automation = AutomationControl {
            automated_actions_enabled: true,
        };
        let mut proposal = ActionProposal {
            id: "prop-1".to_string(),
            target: "checkout".to_string(),
            impact: ActionImpact::High,
            expected_effect: "restore 200 OK".to_string(),
            blast_radius: "checkout pods".to_string(),
            prerequisites: vec!["healthy v41 image".to_string()],
            recovery_route: "redeploy v42".to_string(),
            approved_by: None,
            approved_target: None,
        };
        // Blocked without approval
        assert!(
            execute_proposal(
                &proposal,
                "oncall-lead",
                &automation,
                true,
                PostActionTelemetry {
                    telemetry_present: true,
                    recovery_criteria_passed: true,
                },
            )
            .is_err()
        );

        // Approval for another target cannot be reused
        proposal.approved_by = Some("oncall-lead".to_string());
        proposal.approved_target = Some("catalog".to_string());
        assert!(
            execute_proposal(
                &proposal,
                "oncall-lead",
                &automation,
                true,
                PostActionTelemetry {
                    telemetry_present: true,
                    recovery_criteria_passed: true,
                },
            )
            .is_err()
        );

        // Missing recovery route blocked
        proposal.approved_target = Some("checkout".to_string());
        proposal.recovery_route = String::new();
        assert!(validate_proposal(&proposal).is_err());

        // Command succeeds but service degraded or telemetry missing -> Failed
        proposal.recovery_route = "redeploy v42".to_string();
        let degraded = execute_proposal(
            &proposal,
            "oncall-lead",
            &automation,
            true,
            PostActionTelemetry {
                telemetry_present: true,
                recovery_criteria_passed: false,
            },
        )
        .unwrap();
        assert_eq!(degraded.outcome, ExecutionOutcome::Failed);
        assert_eq!(degraded.approved_by, "oncall-lead");

        let missing_telemetry = execute_proposal(
            &proposal,
            "oncall-lead",
            &automation,
            true,
            PostActionTelemetry {
                telemetry_present: false,
                recovery_criteria_passed: true,
            },
        )
        .unwrap();
        assert_eq!(missing_telemetry.outcome, ExecutionOutcome::Failed);

        // Recovered only when recovery criteria pass with telemetry
        let recovered = execute_proposal(
            &proposal,
            "oncall-lead",
            &automation,
            true,
            PostActionTelemetry {
                telemetry_present: true,
                recovery_criteria_passed: true,
            },
        )
        .unwrap();
        assert_eq!(recovered.outcome, ExecutionOutcome::Recovered);

        // Authorized operator disables automated action path
        assert!(automation.disable_automated_actions(false).is_err());
        automation.disable_automated_actions(true).unwrap();
        assert!(
            execute_proposal(
                &proposal,
                "oncall-lead",
                &automation,
                true,
                PostActionTelemetry {
                    telemetry_present: true,
                    recovery_criteria_passed: true,
                },
            )
            .is_err()
        );
    }

    #[test]
    fn obs_18_05_08_38_07_26_27_28_review_audit_and_trends() {
        let decisions = vec![
            RecommendationDecision {
                id: "dec-1".to_string(),
                incident_id: "inc-1".to_string(),
                assistant_output_id: "out-1".to_string(),
                decided_by: "alice".to_string(),
                outcome: RecommendationOutcome::Accepted,
                adjudicated_correct: Some(true),
                false_positive: false,
                unsafe_attempt_denied: false,
            },
            RecommendationDecision {
                id: "dec-2".to_string(),
                incident_id: "inc-2".to_string(),
                assistant_output_id: "out-2".to_string(),
                decided_by: "bob".to_string(),
                outcome: RecommendationOutcome::Inaccurate,
                adjudicated_correct: Some(true), // even if accepted/marked, inaccurate is never counted successful
                false_positive: true,
                unsafe_attempt_denied: true,
            },
            RecommendationDecision {
                id: "dec-3".to_string(),
                incident_id: "inc-3".to_string(),
                assistant_output_id: "out-3".to_string(),
                decided_by: "carol".to_string(),
                outcome: RecommendationOutcome::Rejected,
                adjudicated_correct: None,
                false_positive: false,
                unsafe_attempt_denied: false,
            },
        ];
        let report = summarize_recommendations(&decisions);
        assert_eq!(report.accepted, 1);
        assert_eq!(report.rejected, 1);
        assert_eq!(report.inaccurate, 1);
        assert_eq!(report.successful_adjudicated, 1);
        assert_eq!(report.false_positives, 1);
        assert_eq!(report.denied_unsafe_attempts, 1);

        let trend = build_evaluation_trend(&[
            PeriodMeasure {
                period: "2026-W01".to_string(),
                accuracy_rate: 0.80,
                evaluation_method: "rubric-v1".to_string(),
            },
            PeriodMeasure {
                period: "2026-W02".to_string(),
                accuracy_rate: 0.91,
                evaluation_method: "rubric-v2".to_string(),
            },
        ]);
        assert!(!trend[0].method_changed);
        assert!(trend[1].method_changed);
    }

    #[test]
    fn obs_18_09_10_38_01_to_19_investigations_hypotheses_and_bounds() {
        let mut mgr = InvestigationManager::default();
        let scope = BTreeSet::from(["checkout".to_string()]);
        let caps = BTreeSet::from(["read_metrics".to_string()]);

        // Ineligible event creates no investigation
        assert!(
            mgr.ingest_event(
                TriggerEvent {
                    event_id: "evt-0".to_string(),
                    incident_id: "inc-0".to_string(),
                    matched_rule: None,
                },
                scope.clone(),
                caps.clone(),
                2,
                2,
            )
            .is_err()
        );

        // Qualifying events for same incident attach to single investigation
        let inv_id = mgr
            .ingest_event(
                TriggerEvent {
                    event_id: "evt-1".to_string(),
                    incident_id: "inc-1".to_string(),
                    matched_rule: Some("rule-5xx-surge".to_string()),
                },
                scope.clone(),
                caps.clone(),
                2,
                2,
            )
            .unwrap();
        let inv_id_2 = mgr
            .ingest_event(
                TriggerEvent {
                    event_id: "evt-2".to_string(),
                    incident_id: "inc-1".to_string(),
                    matched_rule: Some("rule-5xx-surge".to_string()),
                },
                scope,
                caps,
                2,
                2,
            )
            .unwrap();
        assert_eq!(inv_id, inv_id_2);
        assert_eq!(mgr.investigations.len(), 1);
        assert_eq!(mgr.investigations[&inv_id].triggering_events.len(), 2);

        // Hypotheses comparison & contradicting evidence check
        let mut deploy_h = Hypothesis {
            kind: HypothesisKind::Deployment,
            confidence: 0.7,
            supporting_observations: vec!["deploy v42 at 10:00".to_string()],
            contradicting_observations: vec!["errors started at 09:50".to_string()],
            missing_evidence: vec!["pod diff".to_string()],
            refuted: false,
            confirmed: false,
        };
        assert!(deploy_h.confirm().is_err());
        let db_h = Hypothesis {
            kind: HypothesisKind::Database,
            confidence: 0.6,
            supporting_observations: vec!["db lock wait".to_string()],
            contradicting_observations: vec![],
            missing_evidence: vec!["pg_stat_activity".to_string()],
            refuted: false,
            confirmed: false,
        };
        assert_eq!(
            conclusion_options(&[deploy_h, db_h]),
            vec![HypothesisKind::Deployment, HypothesisKind::Database]
        );

        // Out-of-scope, unapproved, and denied-capability bypass checks
        let mut out_of_scope = PlanStep {
            id: "s0".to_string(),
            tool_name: "metrics".to_string(),
            capability: "read_metrics".to_string(),
            approved: true,
            executed: false,
        };
        assert!(
            mgr.execute_step(&inv_id, &mut out_of_scope, "billing", true)
                .is_err()
        );

        let mut denied_cap = PlanStep {
            id: "s1".to_string(),
            tool_name: "kubectl".to_string(),
            capability: "restart_pod".to_string(),
            approved: true,
            executed: false,
        };
        assert!(
            mgr.execute_step(&inv_id, &mut denied_cap, "checkout", true)
                .is_err()
        );
        let mut bypass_tool = PlanStep {
            id: "s2".to_string(),
            tool_name: "ssh_runner".to_string(),
            capability: "restart_pod".to_string(),
            approved: true,
            executed: false,
        };
        assert!(
            mgr.execute_step(&inv_id, &mut bypass_tool, "checkout", true)
                .is_err()
        );

        // Pause, takeover, resume preserve evidence and scope
        mgr.investigations
            .get_mut(&inv_id)
            .unwrap()
            .evidence
            .push("ev-1".to_string());
        mgr.pause(&inv_id).unwrap();
        assert!(mgr.takeover(&inv_id, "oncall-2", false).is_err());
        mgr.takeover(&inv_id, "oncall-2", true).unwrap();
        mgr.resume(&inv_id).unwrap();
        assert_eq!(
            mgr.investigations[&inv_id].evidence,
            vec!["ev-1".to_string()]
        );

        // Budget limit stops investigation cleanly
        let mut s_ok = PlanStep {
            id: "s3".to_string(),
            tool_name: "metrics".to_string(),
            capability: "read_metrics".to_string(),
            approved: true,
            executed: false,
        };
        mgr.execute_step(&inv_id, &mut s_ok, "checkout", true)
            .unwrap();
        let mut s_ok2 = s_ok.clone();
        s_ok2.id = "s4".to_string();
        mgr.execute_step(&inv_id, &mut s_ok2, "checkout", true)
            .unwrap();
        assert_eq!(
            mgr.investigations[&inv_id].state,
            InvestigationState::StoppedIncomplete
        );

        // Cancelled investigation retains evidence & performs no privileged actions
        mgr.cancel(&inv_id).unwrap();
        assert_eq!(
            mgr.investigations[&inv_id].evidence,
            vec!["ev-1".to_string()]
        );
        assert!(
            mgr.execute_step(&inv_id, &mut s_ok, "checkout", true)
                .is_err()
        );
    }

    #[test]
    fn obs_19_07_onboarding_scenario_reports_missing_prerequisites() {
        let required = [
            "owner_assigned",
            "otel_sdk_configured",
            "baseline_alert_attached",
        ];
        let partial = BTreeSet::from(["owner_assigned".to_string()]);
        let incomplete = evaluate_onboarding_coverage("orders-new", &partial, &required);
        assert!(!incomplete.coverage_established);
        assert_eq!(
            incomplete.missing_prerequisites,
            vec![
                "otel_sdk_configured".to_string(),
                "baseline_alert_attached".to_string(),
            ]
        );

        let complete = BTreeSet::from([
            "owner_assigned".to_string(),
            "otel_sdk_configured".to_string(),
            "baseline_alert_attached".to_string(),
        ]);
        let ready = evaluate_onboarding_coverage("orders-new", &complete, &required);
        assert!(ready.coverage_established);
        assert!(ready.missing_prerequisites.is_empty());
    }
}
