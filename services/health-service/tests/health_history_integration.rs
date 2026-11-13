//! Transitions, derived interruptions and the history read (phase 2 E3) through the router
//! with real PostgreSQL and the scoped `observable_health` login.

mod common;

use chrono::{DateTime, Duration, SubsecRound as _, Utc};
use common::{Harness, inputs_for, view};
use health_service::assessment::{
    ConditionEvidence, ConditionKind, Outcome, Snapshot, Yields, persist, scope_key,
};
use health_service::history::prune;
use health_service::ports::{EnvironmentClass, TelemetryIdentity};
use observable_auth::authz::MockScenario;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const FRESHNESS: i32 = 900;

/// One reference time per test binary, so offsets compare exactly.
fn ago(minutes: i64) -> DateTime<Utc> {
    static BASE: std::sync::OnceLock<DateTime<Utc>> = std::sync::OnceLock::new();
    *BASE.get_or_init(|| Utc::now().trunc_subsecs(6)) - Duration::minutes(minutes)
}

/// A snapshot evaluated at `at` whose state at that moment is `state`.
fn snapshot(h: &Harness, id: Uuid, at: DateTime<Utc>, state: &str) -> Snapshot {
    let ids = vec![TelemetryIdentity {
        service_name: "payments".into(),
        environment: "prod".into(),
    }];
    let seen = (state != "unknown").then(|| at - Duration::seconds(1));
    let mut ratio = ConditionEvidence::presence("traces", "internal", seen);
    ratio.kind = ConditionKind::ErrorRatio;
    ratio.outcome = Some(match state {
        "unavailable" => Outcome::Breached,
        "unknown" => Outcome::Insufficient,
        _ => Outcome::Ok,
    });
    ratio.yields = Some(Yields::Unavailable);
    ratio.requests = Some(20);
    let s = Snapshot {
        tenant_id: h.tenant,
        service_id: id,
        environment_class: EnvironmentClass::Production,
        rule_id: "platform-default".into(),
        rule_revision: 1,
        scope_key: scope_key(&ids),
        identities: ids,
        conditions: vec![ConditionEvidence::presence("any", "internal", seen), ratio],
        freshness_seconds: FRESHNESS,
        evaluated_at: at,
    };
    assert_eq!(s.freshness(at).0, state);
    s
}

async fn run(h: &Harness, db: &PgPool, id: Uuid, steps: &[(i64, &str)]) {
    for (minutes, state) in steps {
        assert!(
            persist(db, &snapshot(h, id, ago(*minutes), state))
                .await
                .unwrap()
        );
    }
}

fn live(h: &Harness, classification: &str) -> Uuid {
    let id = Uuid::new_v4();
    h.services.put(h.tenant, view(id, "live", classification));
    h.inputs
        .set(h.tenant, vec![inputs_for(id, "payments", "live")]);
    id
}

async fn history(h: &Harness, id: Uuid, query: &str, token: &str) -> (u16, Value, String) {
    h.get(
        &format!("/v1/health/services/{id}/history?environment_class=production{query}"),
        Some(token),
    )
    .await
}

fn states(body: &Value) -> Vec<String> {
    body["transitions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["to_state"].as_str().unwrap().to_owned())
        .collect()
}

fn time(v: &Value) -> DateTime<Utc> {
    v.as_str().unwrap().parse().unwrap()
}

#[tokio::test]
async fn a_known_outage_is_located_with_onset_duration_and_resolution() {
    let h = Harness::start().await;
    let id = live(&h, "internal");
    run(
        &h,
        &h.state.db,
        id,
        &[
            (60, "normal"),
            (55, "normal"),
            (50, "unavailable"),
            (45, "unavailable"),
            (40, "unavailable"),
            (30, "normal"),
            (15, "normal"),
            (1, "normal"),
        ],
    )
    .await;
    let (status, body, raw) = history(&h, id, "", "member").await;
    assert_eq!(status, 200, "{raw}");
    assert_eq!(body["evidence_status"], json!("current"));
    // Transitions only: repeated evaluations of the same state add nothing.
    assert_eq!(states(&body), ["normal", "unavailable", "normal"]);
    let outage = &body["interruptions"][0];
    assert_eq!(body["interruptions"].as_array().unwrap().len(), 1);
    assert_eq!(outage["worst_state"], json!("unavailable"));
    assert_eq!(outage["status"], json!("resolved"));
    assert_eq!(time(&outage["started_at"]), ago(50));
    assert_eq!(outage["duration_seconds"], json!(20 * 60));
    assert_eq!(outage["onset_exact"], json!(true));
    assert_eq!(outage["evidence_gap"], json!(false));

    // A period that starts during the outage keeps its onset.
    let since = (ago(42)).to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    let (_, body, _) = history(&h, id, &format!("&since={}", since), "member").await;
    assert_eq!(body["interruptions"][0]["duration_seconds"], json!(20 * 60));
    // A period after the recovery has no interruption.
    let since = ago(20).to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    let (_, body, _) = history(&h, id, &format!("&since={since}"), "member").await;
    assert_eq!(body["interruptions"], json!([]));
}

#[tokio::test]
async fn a_missing_end_is_unresolved_or_bounded() {
    let h = Harness::start().await;

    // Evaluation stopped during the outage and resumed after recovery: the gap is recorded
    // as unknown from the moment the last evidence aged out, and the end is only bounded.
    let gap = live(&h, "internal");
    run(
        &h,
        &h.state.db,
        gap,
        &[(90, "normal"), (80, "unavailable"), (5, "normal")],
    )
    .await;
    let (_, body, raw) = history(&h, gap, "", "member").await;
    assert_eq!(
        states(&body),
        ["normal", "unavailable", "unknown", "normal"],
        "{raw}"
    );
    assert_eq!(body["transitions"][2]["evidence_status"], json!("stale"));
    let i = &body["interruptions"][0];
    assert_eq!(i["status"], json!("resolved"));
    assert_eq!(i["ended_at"], json!(null));
    assert_eq!(i["duration_seconds"], json!(null));
    assert_eq!(i["evidence_gap"], json!(true));
    assert_eq!(
        time(&i["ended_after"]),
        ago(80) + Duration::seconds(FRESHNESS.into())
    );
    assert_eq!(time(&i["ended_before"]), ago(5));

    // Still failing and current: unresolved, no gap.
    let ongoing = live(&h, "internal");
    run(
        &h,
        &h.state.db,
        ongoing,
        &[(20, "normal"), (10, "unavailable"), (1, "unavailable")],
    )
    .await;
    let (_, body, _) = history(&h, ongoing, "", "member").await;
    let i = &body["interruptions"][0];
    assert_eq!(
        (&i["status"], &i["evidence_gap"]),
        (&json!("unresolved"), &json!(false))
    );
    assert_eq!(i["duration_seconds"], json!(null));

    // The worker stopped during the outage and has not come back: unknown from when the
    // last evidence aged out, so the outage is unresolved with an evidence gap.
    let stopped = live(&h, "internal");
    run(
        &h,
        &h.state.db,
        stopped,
        &[(70, "normal"), (60, "unavailable")],
    )
    .await;
    let (_, body, _) = history(&h, stopped, "", "member").await;
    assert_eq!(states(&body), ["normal", "unavailable", "unknown"]);
    let i = &body["interruptions"][0];
    assert_eq!(
        (&i["status"], &i["evidence_gap"]),
        (&json!("unresolved"), &json!(true))
    );
}

#[tokio::test]
async fn an_older_evaluation_records_no_transition() {
    let h = Harness::start().await;
    let id = live(&h, "internal");
    run(&h, &h.state.db, id, &[(10, "normal"), (5, "normal")]).await;
    // Completes late with an older start.
    assert!(
        !persist(&h.state.db, &snapshot(&h, id, ago(8), "unavailable"))
            .await
            .unwrap()
    );
    let (_, body, _) = history(&h, id, "", "member").await;
    assert_eq!(states(&body), ["normal"]);
    assert_eq!(body["interruptions"], json!([]));
}

#[tokio::test]
async fn history_reads_fail_closed_and_restricted_history_is_withheld() {
    let h = Harness::start().await;
    let id = live(&h, "confidential");
    run(
        &h,
        &h.state.db,
        id,
        &[(30, "normal"), (20, "unavailable"), (10, "normal")],
    )
    .await;

    // A viewer cannot read confidential evidence: the restricted response, no times.
    let (status, body, _) = history(&h, id, "", "viewer").await;
    assert_eq!(status, 200);
    assert_eq!(
        (
            &body["evidence_status"],
            &body["transitions"],
            &body["interruptions"]
        ),
        (&json!("restricted"), &json!([]), &json!([]))
    );
    let (_, body, _) = history(&h, id, "", "member").await;
    assert_eq!(body["interruptions"].as_array().unwrap().len(), 1);

    assert_eq!(history(&h, id, "", "other-member").await.0, 404);
    assert_eq!(history(&h, Uuid::new_v4(), "", "member").await.0, 404);
    assert_eq!(
        h.get(
            &format!("/v1/health/services/{id}/history?environment_class=production"),
            None
        )
        .await
        .0,
        401
    );
    for q in ["", "?environment_class=staging"] {
        let (status, _, _) = h
            .get(
                &format!("/v1/health/services/{id}/history{q}"),
                Some("member"),
            )
            .await;
        assert_eq!(status, 400, "{q}");
    }
    let long = format!(
        "&since={}",
        (Utc::now() - Duration::days(91)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    );
    assert_eq!(history(&h, id, &long, "member").await.0, 400);
    let future = format!(
        "&since={}",
        (Utc::now() + Duration::days(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    );
    assert_eq!(history(&h, id, &future, "member").await.0, 400);

    h.decider.set_scenario(MockScenario::Unavailable);
    for target in [id, Uuid::new_v4()] {
        let (status, body, _) = history(&h, target, "", "member").await;
        assert_eq!(
            (status, body),
            (503, json!({"error": "access_decision_unavailable"}))
        );
    }
}

#[tokio::test]
async fn the_scoped_login_records_and_prunes_but_cannot_rewrite_history() {
    let h = Harness::start().await;
    let scoped = test_support::postgres::scoped_pool(&h.state.db, "observable_health").await;
    let id = live(&h, "internal");
    run(
        &h,
        &scoped,
        id,
        &[(30, "normal"), (20, "unavailable"), (10, "normal")],
    )
    .await;
    let count = |db: PgPool| async move {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM health.transitions")
            .fetch_one(&db)
            .await
            .unwrap()
    };
    assert_eq!(count(scoped.clone()).await, 3);
    assert!(
        sqlx::query("UPDATE health.transitions SET to_state = 'normal'")
            .execute(&scoped)
            .await
            .is_err(),
        "transitions are append-only"
    );
    // Retention removes only rows older than the period.
    assert_eq!(prune(&scoped, Duration::minutes(15)).await.unwrap(), 2);
    assert_eq!(count(scoped.clone()).await, 1);
}
