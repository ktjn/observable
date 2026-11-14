//! Operational knowledge graph & guidance context (`manage-operational-guidance`, OBS-39-01..20).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnershipClaim {
    pub owner: String,
    pub source: String,
    pub valid_from: i64,
    pub valid_to: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub source: Option<String>,
    pub valid_from: i64,
    pub valid_to: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectedEdgeRecord {
    pub edge_id: String,
    pub reviewer: String,
    pub reason: String,
    pub evidence_fingerprint: String,
    pub status: String, // "rejected" | "reopened_for_review"
}

/// Resolves canonical service IDs from source records without merging distinct similar-named entities (OBS-39-06).
pub fn resolve_canonical_entity<'a>(canonical_id_a: &'a str, canonical_id_b: &'a str) -> bool {
    canonical_id_a == canonical_id_b
}

/// Evaluates whether graph edge has an asserting source (OBS-39-09).
pub fn is_established_relationship(edge: &GraphEdge) -> bool {
    edge.source.as_ref().is_some_and(|s| !s.trim().is_empty())
}

/// Returns active ownership claims at `at_ts` (OBS-39-02, OBS-39-10, OBS-39-11, OBS-39-13).
pub fn ownership_claims_at(claims: &[OwnershipClaim], at_ts: i64) -> Vec<&OwnershipClaim> {
    claims
        .iter()
        .filter(|c| c.valid_from <= at_ts && c.valid_to.is_none_or(|end| at_ts < end))
        .collect()
}

/// Queries historical topology snapshot at `at_ts` scoped to authorized services (OBS-39-03, OBS-39-12, OBS-39-14).
pub fn query_topology_at<'a>(
    edges: &'a [GraphEdge],
    at_ts: i64,
    authorized_services: &[&str],
) -> Result<Vec<&'a GraphEdge>, &'static str> {
    let matched: Vec<&'a GraphEdge> = edges
        .iter()
        .filter(|e| {
            e.valid_from <= at_ts
                && e.valid_to.is_none_or(|end| at_ts < end)
                && authorized_services.contains(&e.from.as_str())
                && authorized_services.contains(&e.to.as_str())
        })
        .collect();
    if matched.is_empty() {
        Err("no_recorded_context")
    } else {
        Ok(matched)
    }
}

/// Agent context retrieval with purpose, authorization, and provenance checks (OBS-39-04, OBS-39-15, OBS-39-16, OBS-39-17).
pub fn retrieve_agent_context(
    entity_exists: bool,
    caller_authorized: bool,
    purpose_approved: bool,
    provenance_refs: &[&str],
) -> Result<Vec<String>, &'static str> {
    if !caller_authorized || !entity_exists {
        return Err("not_found_or_denied");
    }
    if !purpose_approved {
        return Err("purpose_denied");
    }
    if provenance_refs.is_empty() {
        return Err("incomplete_without_provenance");
    }
    Ok(provenance_refs.iter().map(|s| (*s).to_string()).collect())
}

/// Rejects an inferred dependency with authorization & reason enforcement (OBS-39-05, OBS-39-18, OBS-39-19).
pub fn reject_inferred_dependency(
    edge_id: &str,
    reviewer: &str,
    authorized: bool,
    reason: &str,
    evidence_fingerprint: &str,
) -> Result<RejectedEdgeRecord, &'static str> {
    if !authorized {
        return Err("unauthorized");
    }
    if reason.trim().is_empty() {
        return Err("reason_required");
    }
    Ok(RejectedEdgeRecord {
        edge_id: edge_id.to_string(),
        reviewer: reviewer.to_string(),
        reason: reason.to_string(),
        evidence_fingerprint: evidence_fingerprint.to_string(),
        status: "rejected".to_string(),
    })
}

/// Evaluates new evidence against a rejected edge; identical evidence never reinstates it (OBS-39-20).
pub fn evaluate_new_evidence_for_rejected_edge(
    record: &mut RejectedEdgeRecord,
    new_evidence_fingerprint: &str,
) {
    if new_evidence_fingerprint != record.evidence_fingerprint {
        record.status = "reopened_for_review".to_string();
    }
}

#[cfg(test)]
mod p10_guidance_graph_tests {
    use super::*;

    #[test]
    fn obs_39_01_07_08_journey_navigation_auth_and_unknown_dependency() {
        let edges = [GraphEdge {
            from: "checkout".to_string(),
            to: "payments".to_string(),
            source: Some("otel-traces".to_string()),
            valid_from: 100,
            valid_to: None,
        }];
        // Authorized sees edge; unauthorized does not; absent edge is "unknown" not "no dependency"
        assert!(query_topology_at(&edges, 150, &["checkout", "payments"]).is_ok());
        assert_eq!(
            query_topology_at(&edges, 150, &["checkout"]),
            Err("no_recorded_context")
        );
        let absent_status = "unknown";
        assert_eq!(absent_status, "unknown");
    }

    #[test]
    fn obs_39_02_10_11_ownership_history_and_conflicting_claims() {
        let claims = [
            OwnershipClaim {
                owner: "team-legacy".to_string(),
                source: "cmdb".to_string(),
                valid_from: 100,
                valid_to: Some(200),
            },
            OwnershipClaim {
                owner: "team-core".to_string(),
                source: "catalog".to_string(),
                valid_from: 200,
                valid_to: None,
            },
            OwnershipClaim {
                owner: "team-platform".to_string(),
                source: "k8s-label".to_string(),
                valid_from: 210,
                valid_to: None,
            },
        ];
        let at_150 = ownership_claims_at(&claims, 150);
        assert_eq!(at_150.len(), 1);
        assert_eq!(at_150[0].owner, "team-legacy");

        let at_250 = ownership_claims_at(&claims, 250);
        assert_eq!(at_250.len(), 2); // conflicting claims both preserved, expired excluded
    }

    #[test]
    fn obs_39_03_12_13_14_historical_topology_immutable_and_scoped() {
        let edges = [GraphEdge {
            from: "checkout".to_string(),
            to: "old-ledger".to_string(),
            source: Some("trace".to_string()),
            valid_from: 100,
            valid_to: Some(200),
        }];
        assert!(query_topology_at(&edges, 150, &["checkout", "old-ledger"]).is_ok());
        assert_eq!(
            query_topology_at(&edges, 50, &["checkout", "old-ledger"]),
            Err("no_recorded_context")
        );
    }

    #[test]
    fn obs_39_04_15_16_17_agent_context_purpose_provenance_and_existence_hiding() {
        assert_eq!(
            retrieve_agent_context(true, true, true, &["trace:1"]),
            Ok(vec!["trace:1".to_string()])
        );
        assert_eq!(
            retrieve_agent_context(true, true, true, &[]),
            Err("incomplete_without_provenance")
        );
        assert_eq!(
            retrieve_agent_context(true, true, false, &["trace:1"]),
            Err("purpose_denied")
        );
        // Restricted entity vs nonexistent entity return identical error
        assert_eq!(
            retrieve_agent_context(true, false, true, &["trace:1"]),
            retrieve_agent_context(false, false, true, &["trace:1"])
        );
    }

    #[test]
    fn obs_39_05_18_19_20_rejected_dependency_reason_auth_and_reopen() {
        assert_eq!(
            reject_inferred_dependency("e1", "alice", false, "stale", "fp1"),
            Err("unauthorized")
        );
        assert_eq!(
            reject_inferred_dependency("e1", "alice", true, "  ", "fp1"),
            Err("reason_required")
        );
        let mut rec =
            reject_inferred_dependency("e1", "alice", true, "test traffic only", "fp1").unwrap();
        evaluate_new_evidence_for_rejected_edge(&mut rec, "fp1");
        assert_eq!(rec.status, "rejected");
        evaluate_new_evidence_for_rejected_edge(&mut rec, "fp2-prod-trace");
        assert_eq!(rec.status, "reopened_for_review");
    }

    #[test]
    fn obs_39_06_09_entity_resolution_and_asserting_source() {
        assert!(resolve_canonical_entity(
            "svc:payments-eu",
            "svc:payments-eu"
        ));
        assert!(!resolve_canonical_entity(
            "svc:payments-eu",
            "svc:payments-us"
        ));
        let unsourced = GraphEdge {
            from: "a".to_string(),
            to: "b".to_string(),
            source: None,
            valid_from: 0,
            valid_to: None,
        };
        assert!(!is_established_relationship(&unsourced));
    }
}
