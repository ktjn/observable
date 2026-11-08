//! Monitoring coverage (phase 1 S5) in the standalone harness: real catalog schema on
//! PostgreSQL, provider-owned mocks for decide-access and lookup-principals, and the
//! store-telemetry substitute.

mod common;

use catalog_service::ports::SignalsSeen;
use chrono::{Duration, Utc};
use common::{Harness, INTERNAL_TOKEN};
use observable_auth::authz::MockScenario;
use serde_json::{Value, json};
use uuid::Uuid;

fn now_ns() -> u64 {
    Utc::now().timestamp_nanos_opt().unwrap() as u64
}

fn ago_ns(minutes: i64) -> u64 {
    (Utc::now() - Duration::minutes(minutes))
        .timestamp_nanos_opt()
        .unwrap() as u64
}

async fn svc(h: &Harness, name: &str, criticality: &str, extra: Value) -> Uuid {
    let mut body = json!({
        "business_name": name,
        "operational_name": name.to_lowercase().replace(' ', "-"),
        "criticality": criticality,
    });
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    h.service(body).await
}

async fn ok(h: &Harness, method: &str, uri: &str, token: &str, body: Option<Value>) -> Value {
    let (status, json, raw) = h.call(method, uri, Some(token), body).await;
    assert!(
        (200..300).contains(&status),
        "{method} {uri} -> {status} {raw}"
    );
    json
}

async fn bind(h: &Harness, id: Uuid, name: &str, env: &str) {
    ok(
        h,
        "PUT",
        &format!("/v1/catalog/services/{id}/bindings"),
        "admin",
        Some(json!({"bindings": [{"service_name": name, "environment": env}]})),
    )
    .await;
}

async fn policy(h: &Harness, criticality: &str, items: &[&str], approve: bool) {
    ok(
        h,
        "PUT",
        &format!("/v1/catalog/coverage/policies/{criticality}"),
        "admin",
        Some(json!({"items": items, "window_minutes": 60})),
    )
    .await;
    if approve {
        ok(
            h,
            "POST",
            &format!("/v1/catalog/coverage/policies/{criticality}/approve"),
            "admin",
            None,
        )
        .await;
    }
}

async fn runbook(h: &Harness, service: Uuid, id: &str, owner: Uuid) {
    ok(
        h,
        "PUT",
        &format!("/v1/catalog/assets/runbook/{id}"),
        "admin",
        Some(json!({
            "service_id": service,
            "name": "Runbook",
            "owner": {"principal_kind": "user", "principal_id": owner},
        })),
    )
    .await;
}

async fn coverage(h: &Harness, token: &str, id: Uuid) -> Value {
    ok(
        h,
        "GET",
        &format!("/v1/catalog/services/{id}/coverage"),
        token,
        None,
    )
    .await
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

fn item<'a>(report: &'a Value, name: &str) -> &'a Value {
    report["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["item"] == name)
        .unwrap()
}

fn reasons_of(exceptions: &Value, id: Uuid) -> Vec<String> {
    exceptions["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == id.to_string())
        .map(|i| strings(&i["reasons"]))
        .unwrap_or_default()
}

// --- OBS-02-03 -----------------------------------------------------------------------------

#[tokio::test]
async fn service_without_monitoring_is_uncovered_not_healthy() {
    let h = Harness::start().await;
    let id = svc(&h, "Card Payments", "critical", json!({})).await;

    // Never assessed: unknown, not covered and not healthy.
    let listed = ok(&h, "GET", "/v1/catalog/coverage", "viewer", None).await;
    assert_eq!(listed["items"][0]["coverage_status"], "unknown");

    let report = coverage(&h, "viewer", id).await;
    assert_eq!(report["status"], "uncovered");
    assert_eq!(strings(&report["unmet"]), vec!["traces", "logs", "metrics"]);
    assert_eq!(item(&report, "traces")["detail"], "no_telemetry_binding");
    assert_eq!(report["policy"]["source"], "default");
    assert!(!report.to_string().to_lowercase().contains("healthy"));

    let listed = ok(&h, "GET", "/v1/catalog/coverage", "viewer", None).await;
    assert_eq!(listed["items"][0]["coverage_status"], "uncovered");
    let exceptions = ok(&h, "GET", "/v1/catalog/exceptions", "viewer", None).await;
    assert!(reasons_of(&exceptions, id).contains(&"coverage_uncovered".to_owned()));
}

#[tokio::test]
async fn missing_or_incomplete_coverage_is_distinguishable() {
    let h = Harness::start().await;
    policy(&h, "high", &["traces", "metrics", "runbook"], true).await;
    let full = svc(&h, "Search", "high", json!({})).await;
    let half = svc(&h, "Basket", "high", json!({})).await;
    let none = svc(&h, "Wishlist", "high", json!({})).await;
    bind(&h, full, "search-api", "prod").await;
    bind(&h, half, "basket-api", "prod").await;
    let all = SignalsSeen {
        traces: Some(now_ns()),
        logs: None,
        metrics: Some(now_ns()),
    };
    h.telemetry.record(h.tenant, "search-api", "prod", all);
    h.telemetry.record(
        h.tenant,
        "basket-api",
        "prod",
        SignalsSeen {
            traces: Some(now_ns()),
            ..Default::default()
        },
    );
    runbook(&h, full, "rb-search", h.member).await;

    ok(&h, "POST", "/v1/catalog/coverage/assess", "member", None).await;
    let listed = ok(&h, "GET", "/v1/catalog/coverage", "viewer", None).await;
    let status = |id: Uuid| {
        listed["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == id.to_string())
            .unwrap()["coverage_status"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(status(full), "covered");
    assert_eq!(status(half), "partial");
    assert_eq!(status(none), "uncovered");
    let report = coverage(&h, "viewer", full).await;
    assert_eq!(
        item(&report, "traces")["evidence_ref"],
        "traces:search-api@prod"
    );
    assert_eq!(
        item(&report, "runbook")["evidence_ref"],
        "asset:runbook/rb-search"
    );

    // Telemetry that cannot be read is unknown, never covered.
    h.telemetry.set_unavailable();
    let report = coverage(&h, "viewer", full).await;
    assert_eq!(report["status"], "unknown");
    assert_eq!(report["error"], "telemetry_unavailable");
}

// --- OBS-27-02 -----------------------------------------------------------------------------

#[tokio::test]
async fn coverage_report_identifies_each_unmet_requirement() {
    let h = Harness::start().await;
    policy(
        &h,
        "critical",
        &["traces", "logs", "metrics", "alert_rule", "runbook"],
        true,
    )
    .await;
    let id = svc(&h, "Card Payments", "critical", json!({})).await;
    bind(&h, id, "payments-api", "").await;
    h.telemetry.record(
        h.tenant,
        "payments-api",
        "prod-eu",
        SignalsSeen {
            traces: Some(now_ns()),
            ..Default::default()
        },
    );
    // A producer registers an alert rule by telemetry identity.
    let (status, _, raw) = h
        .call_with(
            "PUT",
            "/internal/catalog/assets/alert_rule/rule-17",
            None,
            Some(INTERNAL_TOKEN),
            Some(json!({
                "tenant_id": h.tenant,
                "owning_component": "observable-alerting",
                "service_name": "payments-api",
                "environment": "prod-eu",
                "name": "High error rate",
                "owner": {"principal_kind": "user", "principal_id": h.member},
            })),
        )
        .await;
    assert_eq!(status, 204, "{raw}");

    let report = coverage(&h, "member", id).await;
    assert_eq!(report["status"], "partial");
    assert_eq!(
        strings(&report["unmet"]),
        vec!["logs", "metrics", "runbook"]
    );
    assert_eq!(
        item(&report, "alert_rule")["evidence_ref"],
        "asset:alert_rule/rule-17"
    );
    assert_eq!(item(&report, "logs")["detail"], "no_data_within_60m");
    assert_eq!(item(&report, "runbook")["detail"], "no_registered_asset");
}

#[tokio::test]
async fn claimed_requirement_without_evidence_is_unmet() {
    let h = Harness::start().await;
    policy(&h, "critical", &["traces", "runbook", "dashboard"], true).await;
    let id = svc(&h, "Card Payments", "critical", json!({})).await;
    bind(&h, id, "payments-api", "prod").await;
    // Data older than the policy window is not evidence.
    h.telemetry.record(
        h.tenant,
        "payments-api",
        "prod",
        SignalsSeen {
            traces: Some(ago_ns(180)),
            ..Default::default()
        },
    );
    // A dashboard whose owner left is not evidence.
    let leaver = Uuid::new_v4();
    h.directory.add_member(h.tenant, leaver);
    ok(
        &h,
        "PUT",
        "/v1/catalog/assets/dashboard/dash-9",
        "admin",
        Some(
            json!({"service_id": id, "owner": {"principal_kind": "user", "principal_id": leaver}}),
        ),
    )
    .await;
    h.directory.remove_member(h.tenant, leaver);
    for claimed in ["traces", "runbook", "dashboard"] {
        let (status, _, _) = h
            .call(
                "PUT",
                &format!("/v1/catalog/services/{id}/coverage/claims/{claimed}"),
                Some("member"),
                Some(json!({"note": "we have this"})),
            )
            .await;
        assert_eq!(status, 204);
    }

    let report = coverage(&h, "member", id).await;
    assert_eq!(report["status"], "uncovered");
    assert_eq!(
        strings(&report["unmet"]),
        vec!["traces", "runbook", "dashboard"]
    );
    for name in ["traces", "runbook", "dashboard"] {
        let i = item(&report, name);
        assert_eq!(i["claimed"], true);
        assert_eq!(i["demonstrated"], false);
        assert_eq!(i["evidence_ref"], Value::Null);
        assert!(
            i["detail"]
                .as_str()
                .unwrap()
                .starts_with("claimed_without_evidence"),
            "{i}"
        );
    }
    assert!(
        item(&report, "dashboard")["detail"]
            .as_str()
            .unwrap()
            .ends_with("asset_without_active_owner")
    );
    let ownerless = ok(
        &h,
        "GET",
        "/v1/catalog/assets?ownerless=true",
        "viewer",
        None,
    )
    .await;
    assert_eq!(ownerless["items"][0]["external_id"], "dash-9");
    assert_eq!(ownerless["items"][0]["owner_state"], "departed");
}

// --- OBS-27-01 (records) -------------------------------------------------------------------

#[tokio::test]
async fn critical_service_without_agreed_coverage_is_an_exception() {
    let h = Harness::start().await;
    let id = svc(&h, "Card Payments", "critical", json!({})).await;
    let exceptions = ok(&h, "GET", "/v1/catalog/exceptions", "viewer", None).await;
    assert!(reasons_of(&exceptions, id).contains(&"no_approved_coverage_policy".to_owned()));

    // Only tenant admins set policies, and only a person approves them.
    let (status, _, _) = h
        .call(
            "PUT",
            "/v1/catalog/coverage/policies/critical",
            Some("member"),
            Some(json!({"items": ["traces"]})),
        )
        .await;
    assert_eq!(status, 403);
    policy(&h, "critical", &["traces", "alert_rule"], false).await;
    let listed = ok(&h, "GET", "/v1/catalog/coverage/policies", "viewer", None).await;
    let critical = &listed["items"][0];
    assert_eq!(critical["source"], "unapproved");
    let exceptions = ok(&h, "GET", "/v1/catalog/exceptions", "viewer", None).await;
    assert!(reasons_of(&exceptions, id).contains(&"no_approved_coverage_policy".to_owned()));

    ok(
        &h,
        "POST",
        "/v1/catalog/coverage/policies/critical/approve",
        "admin",
        None,
    )
    .await;
    let exceptions = ok(&h, "GET", "/v1/catalog/exceptions", "viewer", None).await;
    assert!(!reasons_of(&exceptions, id).contains(&"no_approved_coverage_policy".to_owned()));

    // Changing an approved policy withdraws the approval.
    policy(&h, "critical", &["traces"], false).await;
    let exceptions = ok(&h, "GET", "/v1/catalog/exceptions", "viewer", None).await;
    assert!(reasons_of(&exceptions, id).contains(&"no_approved_coverage_policy".to_owned()));

    for bad in [
        json!({"items": []}),
        json!({"items": ["vibes"]}),
        json!({"items": ["traces"], "window_minutes": 1}),
    ] {
        let (status, _, _) = h
            .call(
                "PUT",
                "/v1/catalog/coverage/policies/critical",
                Some("admin"),
                Some(bad),
            )
            .await;
        assert_eq!(status, 400);
    }
}

// --- OBS-27-04 (records) -------------------------------------------------------------------

fn gap_for<'a>(gaps: &'a Value, item: &str) -> &'a Value {
    gaps["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["item"] == item)
        .unwrap()
}

#[tokio::test]
async fn critical_gap_closes_only_when_remediated_or_risk_accepted() {
    let h = Harness::start().await;
    policy(&h, "critical", &["traces", "runbook"], true).await;
    let id = svc(&h, "Card Payments", "critical", json!({})).await;
    bind(&h, id, "payments-api", "prod").await;
    coverage(&h, "member", id).await;

    let gaps = ok(&h, "GET", "/v1/catalog/gaps?state=open", "viewer", None).await;
    assert_eq!(gaps["total"], 2);
    let traces = gap_for(&gaps, "traces")["id"].as_str().unwrap().to_owned();
    let runbook_gap = gap_for(&gaps, "runbook")["id"].as_str().unwrap().to_owned();

    // Neither open nor merely assigned gaps close.
    let close = |gap: &str| format!("/v1/catalog/gaps/{gap}/close");
    let (status, body, _) = h.call("POST", &close(&traces), Some("member"), None).await;
    assert_eq!(
        (status, body["error"].as_str()),
        (409, Some("gap_not_closable"))
    );
    ok(
        &h,
        "POST",
        &format!("/v1/catalog/gaps/{traces}/remediate"),
        "member",
        Some(json!({
            "owner": {"principal_kind": "user", "principal_id": h.member},
            "due_date": (Utc::now() + Duration::days(14)).date_naive(),
        })),
    )
    .await;
    let (status, _, _) = h.call("POST", &close(&traces), Some("member"), None).await;
    assert_eq!(status, 409);

    // Remediated with evidence: closes.
    h.telemetry.record(
        h.tenant,
        "payments-api",
        "prod",
        SignalsSeen {
            traces: Some(now_ns()),
            ..Default::default()
        },
    );
    coverage(&h, "member", id).await;
    let closed = ok(&h, "POST", &close(&traces), "member", None).await;
    assert_eq!(closed["state"], "closed");
    assert_eq!(closed["closed_reason"], "remediated");

    // Deferral needs a person's documented acceptance with a future review date.
    let accept = format!("/v1/catalog/gaps/{runbook_gap}/accept");
    let review = (Utc::now() + Duration::days(30)).date_naive();
    let (status, _, _) = h
        .call(
            "POST",
            &accept,
            Some("key-member"),
            Some(json!({"rationale": "Runbook after migration", "review_date": review})),
        )
        .await;
    assert_eq!(status, 403);
    for bad in [
        json!({"rationale": " ", "review_date": review}),
        json!({"rationale": "x", "review_date": Utc::now().date_naive()}),
    ] {
        let (status, _, _) = h.call("POST", &accept, Some("admin"), Some(bad)).await;
        assert_eq!(status, 400);
    }
    ok(
        &h,
        "POST",
        &accept,
        "admin",
        Some(json!({"rationale": "Runbook after migration", "review_date": review})),
    )
    .await;
    let closed = ok(&h, "POST", &close(&runbook_gap), "member", None).await;
    assert_eq!(closed["closed_reason"], "risk_accepted");
    assert_eq!(closed["acceptance_approver"], h.admin.to_string());
}

#[tokio::test]
async fn missing_journey_coverage_cannot_be_closed_without_remediation() {
    let h = Harness::start().await;
    let pay = svc(&h, "Card Payments", "critical", json!({})).await;
    let (status, _, raw) = h
        .call(
            "POST",
            "/v1/catalog/journeys",
            Some("admin"),
            Some(json!({"name": "Checkout", "criticality": "critical", "service_ids": [pay]})),
        )
        .await;
    assert_eq!(status, 201, "{raw}");
    ok(&h, "POST", "/v1/catalog/coverage/assess", "member", None).await;
    let gaps = ok(&h, "GET", "/v1/catalog/gaps", "viewer", None).await;
    let journey_gap = gap_for(&gaps, "journey_coverage");
    assert!(journey_gap["journey_id"].is_string());
    let (status, body, _) = h
        .call(
            "POST",
            &format!(
                "/v1/catalog/gaps/{}/close",
                journey_gap["id"].as_str().unwrap()
            ),
            Some("admin"),
            None,
        )
        .await;
    assert_eq!(
        (status, body["error"].as_str()),
        (409, Some("gap_not_closable"))
    );
}

// --- Registry and access -------------------------------------------------------------------

#[tokio::test]
async fn producer_registration_requires_the_internal_token_and_a_known_service() {
    let h = Harness::start().await;
    let id = svc(&h, "Card Payments", "critical", json!({})).await;
    let body =
        json!({"tenant_id": h.tenant, "owning_component": "observable-alerting", "service_id": id});
    let uri = "/internal/catalog/assets/alert_rule/r1";
    let (status, _, _) = h
        .call_with("PUT", uri, None, None, Some(body.clone()))
        .await;
    assert_eq!(status, 401);
    let (status, _, _) = h
        .call_with("PUT", uri, None, Some("wrong"), Some(body.clone()))
        .await;
    assert_eq!(status, 401);
    // A caller credential is not enough either.
    let (status, _, _) = h
        .call_with("PUT", uri, Some("admin"), None, Some(body.clone()))
        .await;
    assert_eq!(status, 401);

    let (status, json, _) = h
        .call_with(
            "PUT",
            uri,
            None,
            Some(INTERNAL_TOKEN),
            Some(json!({"tenant_id": h.tenant, "owning_component": "observable-alerting", "service_name": "nobody"})),
        )
        .await;
    assert_eq!((status, json), (422, json!({"error": "unknown_service"})));
    // Another tenant's service id does not resolve.
    let (status, _, _) = h
        .call_with(
            "PUT",
            uri,
            None,
            Some(INTERNAL_TOKEN),
            Some(json!({"tenant_id": h.other_tenant, "owning_component": "x", "service_id": id})),
        )
        .await;
    assert_eq!(status, 422);

    let (status, _, _) = h
        .call_with("PUT", uri, None, Some(INTERNAL_TOKEN), Some(body))
        .await;
    assert_eq!(status, 204);
    let ownerless = ok(
        &h,
        "GET",
        "/v1/catalog/assets?ownerless=true",
        "viewer",
        None,
    )
    .await;
    assert_eq!(ownerless["items"][0]["owner_state"], "none");
    let (status, _, _) = h
        .call_with(
            "DELETE",
            &format!("{uri}?tenant_id={}", h.tenant),
            None,
            Some(INTERNAL_TOKEN),
            None,
        )
        .await;
    assert_eq!(status, 204);
    let listed = ok(&h, "GET", "/v1/catalog/assets", "viewer", None).await;
    assert_eq!(listed["total"], 0);
}

#[tokio::test]
async fn coverage_reads_respect_service_access() {
    let h = Harness::start().await;
    let secret = svc(
        &h,
        "Fraud Engine",
        "critical",
        json!({"classification": "restricted"}),
    )
    .await;
    svc(&h, "Card Payments", "critical", json!({})).await;
    ok(&h, "POST", "/v1/catalog/coverage/assess", "admin", None).await;

    let (s1, _, hidden) = h
        .call(
            "GET",
            &format!("/v1/catalog/services/{secret}/coverage"),
            Some("viewer"),
            None,
        )
        .await;
    let (s2, _, missing) = h
        .call(
            "GET",
            &format!("/v1/catalog/services/{}/coverage", Uuid::new_v4()),
            Some("viewer"),
            None,
        )
        .await;
    assert_eq!((s1, s2), (404, 404));
    assert_eq!(hidden, missing);

    let listed = ok(&h, "GET", "/v1/catalog/coverage", "viewer", None).await;
    assert_eq!(listed["total"], 1);
    let gaps = ok(&h, "GET", "/v1/catalog/gaps", "viewer", None).await;
    assert!(!gaps.to_string().contains(&secret.to_string()));
    let (status, _, _) = h
        .call(
            "PUT",
            &format!("/v1/catalog/services/{secret}/bindings"),
            Some("member"),
            Some(json!({"bindings": []})),
        )
        .await;
    assert_eq!(status, 404);

    h.decider.set_scenario(MockScenario::Unavailable);
    for uri in [
        "/v1/catalog/coverage",
        "/v1/catalog/gaps",
        "/v1/catalog/assets",
        "/v1/catalog/coverage/policies",
    ] {
        let (status, body, _) = h.call("GET", uri, Some("admin"), None).await;
        assert_eq!(status, 503, "{uri}");
        assert_eq!(body, json!({"error": "access_decision_unavailable"}));
    }
}
