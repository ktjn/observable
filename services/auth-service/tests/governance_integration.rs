//! Real Postgres + HTTP integration tests for data retention, legal holds
//! (`OBS-14-03`), and unified governance change-audit attribution (`OBS-14-04`).

use std::sync::Arc;

use auth_service::authz::{AuthzState, router};
use observable_auth::governance::{
    AuditTargetKind, GovernanceAuditEntry, IncidentReconstruction, RecordComplianceIncidentEvent,
    RecordGovernanceChange, RetentionDecision, RetentionOutcome,
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

const TOKEN: &str = "governance-internal-token";

async fn serve(db: PgPool) -> String {
    let app = router(AuthzState {
        db,
        internal_token: Arc::new(TOKEN.into()),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    format!("http://{addr}")
}

#[tokio::test]
async fn data_owner_can_review_retention_policy_and_verify_sample_decision() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let base = serve(db).await;
    let client = reqwest::Client::new();
    let tenant_id = Uuid::new_v4();

    // Create/update retention policy for confidential logs.
    let upsert = client
        .put(format!("{base}/internal/governance/retention-policies"))
        .header("X-Internal-Token", TOKEN)
        .json(&json!({
            "tenant_id": tenant_id,
            "data_domain": "logs",
            "classification": "confidential",
            "purpose": "security_audit",
            "retention_days": 30,
            "owner_id": "owner-sec-ops"
        }))
        .send()
        .await
        .expect("upsert");
    assert_eq!(upsert.status(), 200);

    // Data owner reviews applicable policies.
    let review = client
        .get(format!(
            "{base}/internal/governance/retention-policies/{tenant_id}"
        ))
        .header("X-Internal-Token", TOKEN)
        .send()
        .await
        .expect("review");
    assert_eq!(review.status(), 200);
    let body: serde_json::Value = review.json().await.expect("json");
    assert_eq!(body["policies"].as_array().unwrap().len(), 1);
    assert_eq!(body["policies"][0]["retention_days"], 30);
    assert_eq!(body["policies"][0]["owner_id"], "owner-sec-ops");
    assert_eq!(body["policies"][0]["purpose"], "security_audit");

    // Verify a sample retention decision within the 30-day window.
    let eval = client
        .post(format!("{base}/internal/governance/retention/evaluate"))
        .header("X-Internal-Token", TOKEN)
        .json(&json!({
            "tenant_id": tenant_id,
            "data_domain": "logs",
            "classification": "confidential",
            "age_days": 14
        }))
        .send()
        .await
        .expect("evaluate");
    assert_eq!(eval.status(), 200);
    let decision: RetentionDecision = eval.json().await.expect("decision");
    assert_eq!(decision.outcome, RetentionOutcome::Retain);
    assert!(decision.retrievable);
    assert!(!decision.legal_hold_active);
}

#[tokio::test]
async fn expired_data_is_not_retrievable_unless_under_active_legal_hold() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let base = serve(db).await;
    let client = reqwest::Client::new();
    let tenant_id = Uuid::new_v4();

    client
        .put(format!("{base}/internal/governance/retention-policies"))
        .header("X-Internal-Token", TOKEN)
        .json(&json!({
            "tenant_id": tenant_id,
            "data_domain": "traces",
            "classification": "restricted",
            "purpose": "incident_investigation",
            "retention_days": 14,
            "owner_id": "owner-platform"
        }))
        .send()
        .await
        .expect("upsert");

    // Past 14d retention with no legal hold -> Purge, NOT retrievable.
    let expired: RetentionDecision = client
        .post(format!("{base}/internal/governance/retention/evaluate"))
        .header("X-Internal-Token", TOKEN)
        .json(&json!({
            "tenant_id": tenant_id,
            "data_domain": "traces",
            "classification": "restricted",
            "age_days": 45
        }))
        .send()
        .await
        .expect("evaluate expired")
        .json()
        .await
        .expect("json");
    assert_eq!(expired.outcome, RetentionOutcome::Purge);
    assert!(!expired.retrievable);

    // Place an active legal hold -> Hold, retrievable despite being past retention.
    let hold_resp = client
        .post(format!("{base}/internal/governance/legal-holds"))
        .header("X-Internal-Token", TOKEN)
        .json(&json!({
            "tenant_id": tenant_id,
            "data_domain": "traces",
            "reason": "litigation-case-2026-42",
            "placed_by": "counsel-alice",
            "active": true
        }))
        .send()
        .await
        .expect("place hold");
    assert_eq!(hold_resp.status(), 200);

    let held: RetentionDecision = client
        .post(format!("{base}/internal/governance/retention/evaluate"))
        .header("X-Internal-Token", TOKEN)
        .json(&json!({
            "tenant_id": tenant_id,
            "data_domain": "traces",
            "classification": "restricted",
            "age_days": 45
        }))
        .send()
        .await
        .expect("evaluate held")
        .json()
        .await
        .expect("json");
    assert_eq!(held.outcome, RetentionOutcome::Hold);
    assert!(held.retrievable);
    assert!(held.legal_hold_active);

    // Release the legal hold -> back to Purge, NOT retrievable.
    client
        .post(format!("{base}/internal/governance/legal-holds"))
        .header("X-Internal-Token", TOKEN)
        .json(&json!({
            "tenant_id": tenant_id,
            "data_domain": "traces",
            "reason": "resolved",
            "placed_by": "counsel-alice",
            "active": false
        }))
        .send()
        .await
        .expect("release hold");

    let after_release: RetentionDecision = client
        .post(format!("{base}/internal/governance/retention/evaluate"))
        .header("X-Internal-Token", TOKEN)
        .json(&json!({
            "tenant_id": tenant_id,
            "data_domain": "traces",
            "classification": "restricted",
            "age_days": 45
        }))
        .send()
        .await
        .expect("evaluate after release")
        .json()
        .await
        .expect("json");
    assert_eq!(after_release.outcome, RetentionOutcome::Purge);
    assert!(!after_release.retrievable);
}

#[tokio::test]
async fn audit_reviewer_can_determine_who_changed_alert_policy_or_objective_and_when() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let base = serve(db).await;
    let client = reqwest::Client::new();
    let tenant_id = Uuid::new_v4();

    let changes = [
        RecordGovernanceChange {
            tenant_id,
            actor_id: "alice@example.test".into(),
            actor_kind: "user".into(),
            target_kind: AuditTargetKind::Alert,
            target_id: "alert-high-error-rate".into(),
            action: "update".into(),
            route: "PUT /v1/alerts/alert-high-error-rate".into(),
            details: json!({ "threshold": 0.05 }),
        },
        RecordGovernanceChange {
            tenant_id,
            actor_id: "bob@example.test".into(),
            actor_kind: "user".into(),
            target_kind: AuditTargetKind::AccessPolicy,
            target_id: "policy-restricted-read".into(),
            action: "grant".into(),
            route: "POST /internal/authz/grants".into(),
            details: json!({ "role": "security_reviewer" }),
        },
        RecordGovernanceChange {
            tenant_id,
            actor_id: "carol@example.test".into(),
            actor_kind: "user".into(),
            target_kind: AuditTargetKind::Objective,
            target_id: "slo-checkout-availability".into(),
            action: "update".into(),
            route: "PUT /v1/slos/slo-checkout-availability".into(),
            details: json!({ "target": 0.999 }),
        },
    ];

    for change in &changes {
        let resp = client
            .post(format!("{base}/internal/governance/audit"))
            .header("X-Internal-Token", TOKEN)
            .json(change)
            .send()
            .await
            .expect("record audit");
        assert_eq!(resp.status(), 201);
    }

    let entries: Vec<GovernanceAuditEntry> = client
        .get(format!(
            "{base}/internal/governance/audit?tenant_id={tenant_id}"
        ))
        .header("X-Internal-Token", TOKEN)
        .send()
        .await
        .expect("list audit")
        .json()
        .await
        .expect("json");
    assert_eq!(entries.len(), 3);
    for expected in &changes {
        let found = entries
            .iter()
            .find(|e| e.target_id == expected.target_id)
            .expect("entry present");
        assert_eq!(found.actor_id, expected.actor_id);
        assert_eq!(found.target_kind, expected.target_kind);
        assert_eq!(found.route, expected.route);
    }
}

#[tokio::test]
async fn changes_to_alert_policy_or_objective_via_any_route_record_actor_and_time() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let base = serve(db).await;
    let client = reqwest::Client::new();
    let tenant_id = Uuid::new_v4();

    // Unattributed change without actor_id must fail closed (400).
    let missing_actor = client
        .post(format!("{base}/internal/governance/audit"))
        .header("X-Internal-Token", TOKEN)
        .json(&RecordGovernanceChange {
            tenant_id,
            actor_id: "   ".into(),
            actor_kind: "user".into(),
            target_kind: AuditTargetKind::Alert,
            target_id: "alert-1".into(),
            action: "delete".into(),
            route: "DELETE /v1/alerts/alert-1".into(),
            details: json!({}),
        })
        .send()
        .await
        .expect("send");
    assert_eq!(missing_actor.status(), 400);

    // Record changes across public HTTP, internal API, and CLI/automation routes.
    for (kind, id, actor, route) in [
        (
            AuditTargetKind::Alert,
            "alert-p0",
            "api-user-1",
            "POST /v1/alerts",
        ),
        (
            AuditTargetKind::AccessPolicy,
            "policy-p0",
            "svc-provisioner",
            "POST /internal/authz/policies",
        ),
        (
            AuditTargetKind::Objective,
            "obj-p0",
            "cli-operator",
            "POST /v1/admin/slos/import",
        ),
    ] {
        let resp = client
            .post(format!("{base}/internal/governance/audit"))
            .header("X-Internal-Token", TOKEN)
            .json(&RecordGovernanceChange {
                tenant_id,
                actor_id: actor.into(),
                actor_kind: "principal".into(),
                target_kind: kind,
                target_id: id.into(),
                action: "upsert".into(),
                route: route.into(),
                details: json!({}),
            })
            .send()
            .await
            .expect("record");
        assert_eq!(resp.status(), 201);
        let entry: GovernanceAuditEntry = resp.json().await.expect("entry");
        assert!(!entry.actor_id.is_empty());
        assert!(!entry.route.is_empty());
        assert!(entry.occurred_at.timestamp() > 0);
    }

    // Filter by target_kind=objective returns only objective changes with actor and timestamp.
    let obj_entries: Vec<GovernanceAuditEntry> = client
        .get(format!(
            "{base}/internal/governance/audit?tenant_id={tenant_id}&target_kind=objective"
        ))
        .header("X-Internal-Token", TOKEN)
        .send()
        .await
        .expect("list objectives")
        .json()
        .await
        .expect("json");
    assert_eq!(obj_entries.len(), 1);
    assert_eq!(obj_entries[0].actor_id, "cli-operator");
    assert_eq!(obj_entries[0].route, "POST /v1/admin/slos/import");
    assert!(obj_entries[0].occurred_at.timestamp() > 0);
}

#[tokio::test]
async fn audit_sample_shows_incident_chronology_and_decision_makers() {
    use chrono::Duration;

    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let base = serve(db).await;
    let client = reqwest::Client::new();
    let tenant_id = Uuid::new_v4();
    let incident_id = Uuid::new_v4();
    let t0 = chrono::Utc::now() - Duration::days(10);

    let events = [
        (
            "triggered",
            "alert-evaluator",
            "Error budget burn rate > 14x",
            0,
        ),
        (
            "acknowledged",
            "oncall-alice",
            "Paged primary responder; investigating checkout-db",
            5,
        ),
        (
            "mitigated",
            "lead-bob",
            "Rolled back deployment v2.14.3 after trace correlation",
            18,
        ),
        (
            "resolved",
            "oncall-alice",
            "Latency and 5xx returned within SLO for 15 minutes",
            35,
        ),
    ];

    for (event_type, decision_maker, rationale, mins) in events {
        let resp = client
            .post(format!("{base}/internal/compliance/incidents/events"))
            .header("X-Internal-Token", TOKEN)
            .json(&RecordComplianceIncidentEvent {
                tenant_id,
                incident_id,
                title: "Checkout 5xx spike".into(),
                severity: "critical".into(),
                event_type: event_type.into(),
                decision_maker: decision_maker.into(),
                rationale: rationale.into(),
                retention_days: 90,
                details: json!({ "service": "checkout" }),
                occurred_at: Some(t0 + Duration::minutes(mins)),
            })
            .send()
            .await
            .expect("record event");
        assert_eq!(resp.status(), 201);
    }

    let recon: IncidentReconstruction = client
        .get(format!(
            "{base}/internal/compliance/incidents/{incident_id}/reconstruct?tenant_id={tenant_id}"
        ))
        .header("X-Internal-Token", TOKEN)
        .send()
        .await
        .expect("reconstruct")
        .json()
        .await
        .expect("json");

    assert_eq!(recon.chronology.len(), 4);
    assert_eq!(
        recon
            .chronology
            .iter()
            .map(|e| e.event_type.as_str())
            .collect::<Vec<_>>(),
        vec!["triggered", "acknowledged", "mitigated", "resolved"]
    );
    assert_eq!(
        recon.decision_makers,
        vec!["alert-evaluator", "oncall-alice", "lead-bob"]
    );
    assert!(recon.within_retention);
    assert!(recon.complete_without_human_memory);
}

#[tokio::test]
async fn sampled_incident_within_retention_is_reconstructed_without_memory() {
    use chrono::Duration;

    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let base = serve(db).await;
    let client = reqwest::Client::new();
    let tenant_id = Uuid::new_v4();
    let incident_id = Uuid::new_v4();

    // Missing rationale or decision_maker must be rejected at write time so reconstruction never depends on memory.
    let missing_rationale = client
        .post(format!("{base}/internal/compliance/incidents/events"))
        .header("X-Internal-Token", TOKEN)
        .json(&RecordComplianceIncidentEvent {
            tenant_id,
            incident_id,
            title: "Payments degradation".into(),
            severity: "critical".into(),
            event_type: "mitigated".into(),
            decision_maker: "responder-1".into(),
            rationale: "   ".into(),
            retention_days: 90,
            details: json!({}),
            occurred_at: None,
        })
        .send()
        .await
        .expect("send");
    assert_eq!(missing_rationale.status(), 400);

    // Record a 60-day-old incident within a 90-day retention policy.
    let occurred_at = chrono::Utc::now() - Duration::days(60);
    let resp = client
        .post(format!("{base}/internal/compliance/incidents/events"))
        .header("X-Internal-Token", TOKEN)
        .json(&RecordComplianceIncidentEvent {
            tenant_id,
            incident_id,
            title: "Payments degradation".into(),
            severity: "critical".into(),
            event_type: "mitigated".into(),
            decision_maker: "responder-1".into(),
            rationale: "Scaled connection pool from 20 to 60 and drained stalled pod".into(),
            retention_days: 90,
            details: json!({ "deployment": "dep-8821" }),
            occurred_at: Some(occurred_at),
        })
        .send()
        .await
        .expect("record");
    assert_eq!(resp.status(), 201);

    let recon: IncidentReconstruction = client
        .get(format!(
            "{base}/internal/compliance/incidents/{incident_id}/reconstruct?tenant_id={tenant_id}"
        ))
        .header("X-Internal-Token", TOKEN)
        .send()
        .await
        .expect("reconstruct")
        .json()
        .await
        .expect("json");
    assert!(recon.within_retention);
    assert!(recon.complete_without_human_memory);
    assert_eq!(
        recon.chronology[0].rationale,
        "Scaled connection pool from 20 to 60 and drained stalled pod"
    );
    assert_eq!(recon.chronology[0].details["deployment"], "dep-8821");
}
