//! Data-quality issues and the worker self-status (phase 2 E4) through the router with real
//! PostgreSQL and the scoped `observable_health` login.

mod common;

use std::time::Duration as StdDuration;

use common::{Harness, inputs_for, ns_ago, view};
use health_service::app;
use health_service::assessment::{evaluate_all, start_evaluation_worker};
use health_service::ports::{EvidenceTimes, TelemetryIdentity};
use observable_auth::authz::MockScenario;
use serde_json::{Value, json};
use uuid::Uuid;

fn prod(name: &str) -> TelemetryIdentity {
    TelemetryIdentity {
        service_name: name.into(),
        environment: "prod".into(),
    }
}

fn logs_at(h: &Harness, name: &str, seconds_ago: i64) {
    h.evidence.record(
        h.tenant,
        &prod(name),
        EvidenceTimes {
            logs: Some(ns_ago(seconds_ago)),
            ..Default::default()
        },
    );
}

fn live(h: &Harness, name: &str, classification: &str) -> Uuid {
    let id = Uuid::new_v4();
    h.services.put(h.tenant, view(id, "live", classification));
    h.inputs.set(h.tenant, vec![inputs_for(id, name, "live")]);
    id
}

async fn issues(h: &Harness, id: Uuid, token: &str) -> (u16, Value) {
    let (status, body, raw) = h
        .get(
            &format!("/v1/health/services/{id}/issues?environment_class=production"),
            Some(token),
        )
        .await;
    assert!(status != 500, "{raw}");
    (status, body)
}

fn kinds(body: &Value, open: bool) -> Vec<String> {
    body["issues"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["resolved_at"].is_null() == open)
        .map(|i| {
            format!(
                "{}:{}",
                i["kind"].as_str().unwrap(),
                i["service_name"].as_str().unwrap()
            )
        })
        .collect()
}

#[tokio::test]
async fn a_stopped_source_opens_an_issue_that_resolves_when_evidence_returns() {
    let h = Harness::start().await;
    let id = live(&h, "payments", "internal");
    logs_at(&h, "payments", 5);
    h.evaluate().await;
    let (status, body) = issues(&h, id, "member").await;
    assert_eq!((status, &body["open"]), (200, &json!(0)));
    // Staging never had evidence, so it never stopped.
    assert_eq!(body["issues"], json!([]));

    // Last evidence older than one freshness bound (300 s), within the lookback.
    logs_at(&h, "payments", 400);
    h.evaluate().await;
    h.evaluate().await;
    let (_, body) = issues(&h, id, "member").await;
    assert_eq!(kinds(&body, true), ["source_stopped:payments"]);
    let issue = &body["issues"][0];
    assert_eq!(issue["environment"], json!("prod"));
    assert!(!issue["detail"]["last_evidence_at"].is_null());
    let opened = issue["opened_at"].clone();
    assert_ne!(issue["last_seen_at"], opened, "refreshed, not reopened");
    // Health itself is unknown, never normal.
    let (_, health, _) = h.health(id, "production", "member").await;
    assert_eq!(health["state"], json!("unknown"));

    // Silent beyond the lookback: still stopped, still the same issue.
    logs_at(&h, "payments", 90_000);
    h.evaluate().await;
    let (_, body) = issues(&h, id, "member").await;
    assert_eq!(kinds(&body, true), ["source_stopped:payments"]);
    assert_eq!(body["issues"][0]["opened_at"], opened);

    // Evidence returns: resolved, and still disclosed as resolved.
    logs_at(&h, "payments", 5);
    h.evaluate().await;
    let (_, body) = issues(&h, id, "member").await;
    assert_eq!(body["open"], json!(0));
    assert_eq!(kinds(&body, false), ["source_stopped:payments"]);

    // It can stop again: a new issue.
    logs_at(&h, "payments", 400);
    h.evaluate().await;
    let (_, body) = issues(&h, id, "member").await;
    assert_eq!(kinds(&body, true), ["source_stopped:payments"]);
    assert_eq!(kinds(&body, false), ["source_stopped:payments"]);
}

#[tokio::test]
async fn inconsistent_inputs_and_failed_evaluation_raise_issues_until_they_clear() {
    let h = Harness::start().await;
    let id = live(&h, "payments", "internal");
    logs_at(&h, "payments", 5);

    // Another record binds the same identity: ambiguous inputs.
    let other = Uuid::new_v4();
    h.inputs.set(
        h.tenant,
        vec![
            inputs_for(id, "payments", "live"),
            inputs_for(other, "payments", "live"),
        ],
    );
    h.evaluate().await;
    let (_, body) = issues(&h, id, "member").await;
    assert_eq!(kinds(&body, true), ["inconsistent_inputs:"]);
    h.inputs
        .set(h.tenant, vec![inputs_for(id, "payments", "live")]);
    h.evaluate().await;
    let (_, body) = issues(&h, id, "member").await;
    assert_eq!(kinds(&body, true), Vec::<String>::new());
    assert_eq!(kinds(&body, false), ["inconsistent_inputs:"]);

    // Evidence cannot be read.
    h.evidence.set_unavailable();
    h.evaluate().await;
    let (_, body) = issues(&h, id, "member").await;
    assert_eq!(kinds(&body, true), ["evaluation_failed:"]);
    logs_at(&h, "payments", 5);
    h.evaluate().await;
    let (_, body) = issues(&h, id, "member").await;
    assert_eq!(body["open"], json!(0));

    // A retired service has no current issues.
    logs_at(&h, "payments", 400);
    h.evaluate().await;
    assert_eq!(issues(&h, id, "member").await.1["open"], json!(1));
    h.inputs
        .set(h.tenant, vec![inputs_for(id, "payments", "retired")]);
    h.evaluate().await;
    assert_eq!(issues(&h, id, "member").await.1["open"], json!(0));
}

#[tokio::test]
async fn issue_reads_fail_closed_and_restricted_evidence_withholds_issues() {
    let h = Harness::start().await;
    let id = live(&h, "payments", "confidential");
    logs_at(&h, "payments", 400);
    h.evaluate().await;

    let (status, body) = issues(&h, id, "viewer").await;
    assert_eq!(status, 200);
    assert_eq!(
        (&body["evidence_status"], &body["issues"]),
        (&json!("restricted"), &json!([]))
    );
    assert_eq!(issues(&h, id, "member").await.1["open"], json!(1));
    assert_eq!(issues(&h, id, "other-member").await.0, 404);
    assert_eq!(issues(&h, Uuid::new_v4(), "member").await.0, 404);
    let (status, _, _) = h
        .get(&format!("/v1/health/services/{id}/issues"), Some("member"))
        .await;
    assert_eq!(status, 400);
    h.decider.set_scenario(MockScenario::Unavailable);
    for target in [id, Uuid::new_v4()] {
        assert_eq!(
            issues(&h, target, "member").await,
            (503, json!({"error": "access_decision_unavailable"}))
        );
    }
}

#[tokio::test]
async fn the_scoped_login_reconciles_issues() {
    let h = Harness::start().await;
    let scoped = test_support::postgres::scoped_pool(&h.state.db, "observable_health").await;
    let id = live(&h, "payments", "internal");
    logs_at(&h, "payments", 400);
    let s = evaluate_all(&scoped, &h.inputs, &h.evidence).await;
    assert_eq!(s.failed, 0);
    logs_at(&h, "payments", 5);
    evaluate_all(&scoped, &h.inputs, &h.evidence).await;
    let (_, body) = issues(&h, id, "member").await;
    assert_eq!(kinds(&body, false), ["source_stopped:payments"]);
}

async fn self_status(app: &axum::Router, token: Option<&str>) -> (u16, Value) {
    use http_body_util::BodyExt as _;
    use tower::ServiceExt;
    let mut req = axum::http::Request::get("/internal/health/self-status");
    if let Some(t) = token {
        req = req.header("X-Internal-Token", t);
    }
    let resp = app
        .clone()
        .oneshot(req.body(axum::body::Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn a_stopped_worker_is_visible_in_self_status() {
    let h = Harness::start().await;
    live(&h, "payments", "internal");
    logs_at(&h, "payments", 5);

    // Without a configured token every caller is refused.
    assert_eq!(self_status(&h.app, Some("")).await.0, 401);
    let state = h.state.clone().with_internal_token("internal-secret");
    let internal = app(state.clone());
    assert_eq!(self_status(&internal, None).await.0, 401);
    assert_eq!(self_status(&internal, Some("wrong")).await.0, 401);
    let (status, body) = self_status(&internal, Some("internal-secret")).await;
    assert_eq!((status, &body["state"]), (200, &json!("not_started")));

    let worker = tokio::spawn(start_evaluation_worker(
        state.db.clone(),
        h.inputs.clone(),
        h.evidence.clone(),
        StdDuration::from_secs(1),
        chrono::Duration::days(400),
        state.worker.clone(),
    ));
    let mut body = Value::Null;
    for _ in 0..50 {
        body = self_status(&internal, Some("internal-secret")).await.1;
        if body["state"] == json!("current") {
            break;
        }
        tokio::time::sleep(StdDuration::from_millis(100)).await;
    }
    assert_eq!(body["state"], json!("current"));
    assert_eq!(body["function"], json!("assess"));
    assert_eq!(body["interval_seconds"], json!(1));
    assert_eq!(body["last_pass"]["tenants"], json!(1));
    assert_eq!(body["last_pass"]["stored"], json!(2));
    // Counts only: no tenant or service identifiers.
    let raw = body.to_string();
    assert!(!raw.contains(&h.tenant.to_string()) && !raw.contains("payments"));

    // The worker stops: within twice the interval the pass is stale.
    worker.abort();
    tokio::time::sleep(StdDuration::from_millis(2_500)).await;
    let (_, body) = self_status(&internal, Some("internal-secret")).await;
    assert_eq!(body["state"], json!("stale"));
}
