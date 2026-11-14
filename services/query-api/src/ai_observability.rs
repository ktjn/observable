//! AI service observability helpers and tests (`observe-ai-services`, OBS-35-01..08).

use serde::{Deserialize, Serialize};

/// Evaluated answer quality drop detection for customer-facing assistants (OBS-35-01).
pub fn detect_quality_drop(baseline_score: f64, current_score: f64, max_drop: f64) -> bool {
    (baseline_score - current_score) > max_drop
}

/// Feature-level token/cost consumption spike attribution (OBS-35-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureConsumptionSpike {
    pub feature_name: String,
    pub owner: String,
    pub tokens_used: u64,
    pub baseline_tokens: u64,
}

pub fn detect_feature_consumption_spike(
    records: &[FeatureConsumptionSpike],
    multiplier: u64,
) -> Vec<&FeatureConsumptionSpike> {
    records
        .iter()
        .filter(|r| r.tokens_used > r.baseline_tokens.saturating_mul(multiplier))
        .collect()
}

/// Prompt/instruction revision paired with quality evaluation shift (OBS-35-03).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstructionQualityShift {
    pub instruction_version: String,
    pub quality_before: f64,
    pub quality_after: f64,
    pub delta: f64,
}

pub fn record_instruction_quality_shift(
    instruction_version: &str,
    quality_before: f64,
    quality_after: f64,
) -> InstructionQualityShift {
    InstructionQualityShift {
        instruction_version: instruction_version.to_string(),
        quality_before,
        quality_after,
        delta: quality_after - quality_before,
    }
}

/// Reconstructable agent action trace showing reasoning context and selected tool call (OBS-35-04).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentActionTrace {
    pub interaction_id: String,
    pub context_inputs: Vec<String>,
    pub selected_action: String,
    pub rationale: String,
}

/// Flags controlled red-team/safety tests that elicit restricted content (OBS-35-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafetyTestAlert {
    pub test_id: String,
    pub owner: String,
    pub elicited_restricted_content: bool,
    pub flagged_to_owner: bool,
}

pub fn evaluate_safety_probe(
    test_id: &str,
    owner: &str,
    elicited_restricted_content: bool,
) -> SafetyTestAlert {
    SafetyTestAlert {
        test_id: test_id.to_string(),
        owner: owner.to_string(),
        elicited_restricted_content,
        flagged_to_owner: elicited_restricted_content,
    }
}

/// Material shift in input topic distribution (OBS-35-06).
pub fn detect_topic_shift(share_before: f64, share_after: f64, threshold: f64) -> bool {
    (share_after - share_before).abs() >= threshold
}

/// Links negative user feedback directly to the originating interaction ID (OBS-35-07).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackInteractionLink {
    pub feedback_id: String,
    pub interaction_id: String,
    pub sentiment: String,
}

/// Evaluates agent interaction goal outcome independently of individual tool-call statuses (OBS-35-08).
pub fn evaluate_agent_goal_outcome(
    tool_calls_succeeded: bool,
    user_objective_achieved: bool,
) -> &'static str {
    let _ = tool_calls_succeeded;
    if user_objective_achieved {
        "succeeded"
    } else {
        "failed"
    }
}

#[cfg(test)]
mod p10_ai_observability_tests {
    use super::*;

    #[test]
    fn obs_35_01_quality_drop_visible_for_assistant() {
        assert!(detect_quality_drop(0.92, 0.74, 0.10));
        assert!(!detect_quality_drop(0.92, 0.89, 0.10));
    }

    #[test]
    fn obs_35_02_consumption_spike_attributable_to_feature_and_owner() {
        let spikes = [FeatureConsumptionSpike {
            feature_name: "auto-summarize".to_string(),
            owner: "ai-ux-team".to_string(),
            tokens_used: 900_000,
            baseline_tokens: 100_000,
        }];
        let flagged = detect_feature_consumption_spike(&spikes, 3);
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0].owner, "ai-ux-team");
    }

    #[test]
    fn obs_35_03_instruction_change_shown_with_quality_shift() {
        let shift = record_instruction_quality_shift("prompt-v14", 0.88, 0.71);
        assert_eq!(shift.instruction_version, "prompt-v14");
        assert!(shift.delta < 0.0);
    }

    #[test]
    fn obs_35_04_reconstructs_why_agent_took_incorrect_action() {
        let trace = AgentActionTrace {
            interaction_id: "ix-441".to_string(),
            context_inputs: vec!["stale_runbook:v1".to_string()],
            selected_action: "restart_primary_db".to_string(),
            rationale: "Matched keyword 'timeout' in stale_runbook:v1".to_string(),
        };
        assert_eq!(trace.selected_action, "restart_primary_db");
        assert!(!trace.rationale.is_empty());
    }

    #[test]
    fn obs_35_05_restricted_content_elicitation_flagged_to_owner() {
        let alert = evaluate_safety_probe("redteam-pii-01", "ai-safety-owner", true);
        assert!(alert.flagged_to_owner);
        assert_eq!(alert.owner, "ai-safety-owner");
    }

    #[test]
    fn obs_35_06_material_shift_in_input_topics_reported() {
        assert!(detect_topic_shift(0.05, 0.32, 0.15));
    }

    #[test]
    fn obs_35_07_negative_feedback_links_to_originating_interaction() {
        let link = FeedbackInteractionLink {
            feedback_id: "fb-99".to_string(),
            interaction_id: "ix-441".to_string(),
            sentiment: "negative".to_string(),
        };
        assert_eq!(link.interaction_id, "ix-441");
    }

    #[test]
    fn obs_35_08_goal_outcome_independent_of_tool_call_success() {
        assert_eq!(evaluate_agent_goal_outcome(true, false), "failed");
        assert_eq!(evaluate_agent_goal_outcome(true, true), "succeeded");
    }
}
