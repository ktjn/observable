//! Critical journeys, observability scope and operating contexts (phase 1 S4) in the standalone
//! harness: real catalog schema on PostgreSQL, provider-owned mocks for decide-access and
//! lookup-principals, and the environment-class substitute for manage-control-plane.

mod common;

use catalog_service::ports::EnvironmentClass;
use common::Harness;
use observable_auth::authz::MockScenario;
use serde_json::{Value, json};
use uuid::Uuid;

fn names(v: &Value) -> Vec<String> {
    let mut n: Vec<String> = v
        .as_array()
        .expect("array")
        .iter()
        .map(|i| {
            i["name"]
                .as_str()
                .or_else(|| i["business_name"].as_str())
                .unwrap()
                .to_owned()
        })
        .collect();
    n.sort();
    n
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

async fn journey(h: &Harness, token: &str, body: Value) -> (u16, Value, String) {
    h.call("POST", "/v1/catalog/journeys", Some(token), Some(body))
        .await
}

async fn created(h: &Harness, body: Value) -> Uuid {
    let (status, json, raw) = journey(h, "admin", body).await;
    assert_eq!(status, 201, "{raw}");
    json["id"].as_str().unwrap().parse().unwrap()
}

async fn get(h: &Harness, token: &str, uri: &str) -> Value {
    let (status, body, raw) = h.call("GET", uri, Some(token), None).await;
    assert_eq!(status, 200, "{raw}");
    body
}

fn journey_reasons(exceptions: &Value, name: &str) -> Vec<String> {
    exceptions["journeys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["name"] == name)
        .map(|j| {
            j["reasons"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r.as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

// --- OBS-01-01 -----------------------------------------------------------------------------

#[tokio::test]
async fn scope_lists_in_scope_services_and_journeys_with_criticality() {
    let h = Harness::start().await;
    let pay = svc(&h, "Card Payments", "critical", json!({})).await;
    let cart = svc(&h, "Basket", "high", json!({})).await;
    svc(&h, "Legacy Fax", "low", json!({"lifecycle": "retired"})).await;
    created(
        &h,
        json!({
            "name": "Checkout",
            "criticality": "critical",
            "owner": {"principal_kind": "user", "principal_id": h.admin},
            "service_ids": [cart, pay],
        }),
    )
    .await;
    created(
        &h,
        json!({
            "name": "Gift cards",
            "criticality": "low",
            "in_scope": false,
            "scope_rationale": "Run by the partner; covered by their monitoring",
        }),
    )
    .await;

    let scope = get(&h, "viewer", "/v1/catalog/scope").await;
    assert_eq!(names(&scope["services"]), vec!["Basket", "Card Payments"]);
    assert_eq!(names(&scope["journeys"]), vec!["Checkout"]);
    assert_eq!(scope["journeys"][0]["criticality"], "critical");
    assert_eq!(names(&scope["excluded_journeys"]), vec!["Gift cards"]);
    assert_eq!(
        scope["excluded_journeys"][0]["scope_rationale"],
        "Run by the partner; covered by their monitoring"
    );
    for s in scope["services"].as_array().unwrap() {
        assert!(s["criticality"].is_string());
    }

    let (status, detail, raw) = h
        .call(
            "GET",
            &format!(
                "/v1/catalog/journeys/{}",
                scope["journeys"][0]["id"].as_str().unwrap()
            ),
            Some("viewer"),
            None,
        )
        .await;
    assert_eq!(status, 200, "{raw}");
    let ordered: Vec<&str> = detail["services"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["business_name"].as_str().unwrap())
        .collect();
    assert_eq!(ordered, vec!["Basket", "Card Payments"]);
    assert_eq!(detail["owner"]["state"], "active");
    assert_eq!(detail["exceptions"], json!([]));
}

#[tokio::test]
async fn critical_journey_outside_scope_is_reported_as_a_gap() {
    let h = Harness::start().await;
    let pay = svc(&h, "Card Payments", "critical", json!({})).await;
    created(
        &h,
        json!({
            "name": "Refunds",
            "criticality": "critical",
            "in_scope": false,
            "scope_rationale": "Not instrumented yet",
            "service_ids": [pay],
        }),
    )
    .await;

    // Never silently in scope.
    let scope = get(&h, "admin", "/v1/catalog/scope").await;
    assert!(names(&scope["journeys"]).is_empty());
    assert_eq!(names(&scope["excluded_journeys"]), vec!["Refunds"]);

    let exceptions = get(&h, "admin", "/v1/catalog/exceptions").await;
    assert_eq!(
        journey_reasons(&exceptions, "Refunds"),
        vec!["critical_journey_not_in_scope"]
    );

    // Excluding a journey needs a stated rationale.
    let (status, _, _) = journey(
        &h,
        "admin",
        json!({"name": "Returns", "criticality": "high", "in_scope": false}),
    )
    .await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn incomplete_in_scope_journeys_are_exceptions() {
    let h = Harness::start().await;
    let old = svc(&h, "Old Ledger", "high", json!({"lifecycle": "retired"})).await;
    let leaver = Uuid::new_v4();
    h.directory.add_member(h.tenant, leaver);
    created(&h, json!({"name": "Sign-up", "criticality": "critical"})).await;
    created(
        &h,
        json!({
            "name": "Statements",
            "criticality": "critical",
            "owner": {"principal_kind": "user", "principal_id": leaver},
            "service_ids": [old],
        }),
    )
    .await;
    h.directory.remove_member(h.tenant, leaver);

    let exceptions = get(&h, "admin", "/v1/catalog/exceptions").await;
    assert_eq!(
        journey_reasons(&exceptions, "Sign-up"),
        vec!["no_services", "missing_owner"]
    );
    assert_eq!(
        journey_reasons(&exceptions, "Statements"),
        vec!["retired_service", "owner_departed"]
    );
}

// --- OBS-01-03 -----------------------------------------------------------------------------

#[tokio::test]
async fn contexts_partition_environments_and_business_critical_services() {
    let h = Harness::start().await;
    let class = |e: &str, c: &str| EnvironmentClass {
        environment: e.into(),
        class: c.into(),
    };
    h.environments.set(
        h.tenant,
        vec![
            class("prod-eu", "production"),
            class("prod-us", "production"),
            class("staging", "non_production"),
            class("sandbox", "unclassified"),
        ],
    );
    h.environments
        .set(h.other_tenant, vec![class("other-prod", "production")]);
    svc(&h, "Card Payments", "critical", json!({})).await;
    svc(&h, "Basket", "high", json!({})).await;
    svc(&h, "Old Core", "critical", json!({"lifecycle": "retired"})).await;

    let body = get(&h, "viewer", "/v1/catalog/contexts").await;
    let ctx = |name: &str| {
        body["contexts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["context"] == name)
            .unwrap()
            .clone()
    };
    assert_eq!(
        ctx("production")["environments"],
        json!(["prod-eu", "prod-us"])
    );
    assert_eq!(ctx("non_production")["environments"], json!(["staging"]));
    assert_eq!(
        names(&ctx("business_critical")["services"]),
        vec!["Card Payments"]
    );
    assert_eq!(body["unclassified_environments"], json!(["sandbox"]));

    // Restricting catalog views to business-critical is unambiguous...
    let listed = get(
        &h,
        "viewer",
        "/v1/catalog/services?context=business_critical",
    )
    .await;
    assert_eq!(names(&listed["items"]), vec!["Card Payments"]);
    let scope = get(&h, "viewer", "/v1/catalog/scope?context=business_critical").await;
    assert_eq!(names(&scope["services"]), vec!["Card Payments"]);

    // ...and environment contexts or unknown contexts are refused rather than guessed.
    for c in ["production", "prod", ""] {
        let (status, _, _) = h
            .call(
                "GET",
                &format!("/v1/catalog/services?context={c}"),
                Some("viewer"),
                None,
            )
            .await;
        assert_eq!(status, 400, "context={c}");
    }
}

#[tokio::test]
async fn contexts_fail_closed_when_control_plane_is_unavailable() {
    let h = Harness::start().await;
    h.environments.set_unavailable();
    let (status, body, _) = h
        .call("GET", "/v1/catalog/contexts", Some("viewer"), None)
        .await;
    assert_eq!(status, 503);
    assert_eq!(body, json!({"error": "control_plane_unavailable"}));
}

// --- Access (req-catalog-access-control) ---------------------------------------------------

#[tokio::test]
async fn journey_access_is_decided_and_hidden_journeys_match_missing_ones() {
    let h = Harness::start().await;
    let secret = svc(
        &h,
        "Fraud Engine",
        "critical",
        json!({"classification": "restricted"}),
    )
    .await;
    let public = svc(&h, "Card Payments", "critical", json!({})).await;
    let hidden = created(
        &h,
        json!({"name": "Chargebacks", "criticality": "high", "classification": "restricted"}),
    )
    .await;
    let mixed = created(
        &h,
        json!({"name": "Checkout", "criticality": "critical", "service_ids": [public, secret]}),
    )
    .await;

    // Filtered lists never show or count a hidden journey.
    let listed = get(&h, "viewer", "/v1/catalog/journeys").await;
    assert_eq!(names(&listed["items"]), vec!["Checkout"]);
    assert_eq!(listed["total"], 1);

    // Hidden and missing journeys return identical responses.
    let (s1, _, hidden_raw) = h
        .call(
            "GET",
            &format!("/v1/catalog/journeys/{hidden}"),
            Some("viewer"),
            None,
        )
        .await;
    let (s2, _, missing_raw) = h
        .call(
            "GET",
            &format!("/v1/catalog/journeys/{}", Uuid::new_v4()),
            Some("viewer"),
            None,
        )
        .await;
    assert_eq!((s1, s2), (404, 404));
    assert_eq!(hidden_raw, missing_raw);

    // A visible journey never identifies or counts a restricted service.
    let detail = get(&h, "viewer", &format!("/v1/catalog/journeys/{mixed}")).await;
    assert_eq!(names(&detail["services"]), vec!["Card Payments"]);
    assert_eq!(detail["services_withheld"], true);
    assert!(!detail.to_string().contains(&secret.to_string()));

    // Linking a hidden service is refused exactly like linking a missing one.
    let (s1, b1, _) = journey(
        &h,
        "member",
        json!({"name": "Probe A", "criticality": "low", "service_ids": [secret]}),
    )
    .await;
    let (s2, b2, _) = journey(
        &h,
        "member",
        json!({"name": "Probe B", "criticality": "low", "service_ids": [Uuid::new_v4()]}),
    )
    .await;
    assert_eq!((s1, s2), (400, 400));
    assert_eq!(b1, b2);

    // Viewers cannot write; members cannot create above their ceiling.
    let (status, _, _) = journey(&h, "viewer", json!({"name": "V", "criticality": "low"})).await;
    assert_eq!(status, 403);
    let (status, _, _) = journey(
        &h,
        "member",
        json!({"name": "M", "criticality": "low", "classification": "confidential"}),
    )
    .await;
    assert_eq!(status, 403);

    // Unavailable decision point: reads fail closed.
    h.decider.set_scenario(MockScenario::Unavailable);
    for uri in [
        "/v1/catalog/journeys".to_owned(),
        format!("/v1/catalog/journeys/{mixed}"),
        "/v1/catalog/scope".to_owned(),
        "/v1/catalog/contexts".to_owned(),
    ] {
        let (status, body, _) = h.call("GET", &uri, Some("admin"), None).await;
        assert_eq!(status, 503, "{uri}");
        assert_eq!(
            body,
            json!({"error": "access_decision_unavailable"}),
            "{uri}"
        );
    }
}

#[tokio::test]
async fn journeys_are_updated_with_validated_fields() {
    let h = Harness::start().await;
    let a = svc(&h, "Search", "high", json!({})).await;
    let b = svc(&h, "Catalogue", "standard", json!({})).await;
    let id = created(
        &h,
        json!({"name": "Browse", "criticality": "high", "service_ids": [a]}),
    )
    .await;

    let (status, body, raw) = h
        .call(
            "PATCH",
            &format!("/v1/catalog/journeys/{id}"),
            Some("member"),
            Some(json!({"criticality": "critical", "service_ids": [b, a]})),
        )
        .await;
    assert_eq!(status, 200, "{raw}");
    assert_eq!(body["criticality"], "critical");
    let ordered: Vec<&str> = body["services"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["business_name"].as_str().unwrap())
        .collect();
    assert_eq!(ordered, vec!["Catalogue", "Search"]);

    for bad in [
        json!({"criticality": "urgent"}),
        json!({"in_scope": false}),
        json!({"service_ids": [a, a]}),
        json!({"owner": {"principal_kind": "user", "principal_id": Uuid::new_v4()}}),
    ] {
        let (status, _, raw) = h
            .call(
                "PATCH",
                &format!("/v1/catalog/journeys/{id}"),
                Some("member"),
                Some(bad.clone()),
            )
            .await;
        assert_eq!(status, 400, "{bad} -> {raw}");
    }

    let (status, _, _) =
        journey(&h, "admin", json!({"name": "browse", "criticality": "low"})).await;
    assert_eq!(status, 409);

    // Other tenants never see the journey.
    let (status, _, _) = h
        .call(
            "GET",
            &format!("/v1/catalog/journeys/{id}"),
            Some("other-admin"),
            None,
        )
        .await;
    assert_eq!(status, 404);
}
