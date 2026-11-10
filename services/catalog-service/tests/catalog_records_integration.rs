//! manage-service-catalog service records (phase 1 S3) in the standalone harness: real
//! catalog schema on PostgreSQL, provider-owned mocks for decide-access and lookup-principals.

mod common;

use common::Harness;
use observable_auth::authz::MockScenario;
use serde_json::{Value, json};
use uuid::Uuid;

fn ids(v: &Value) -> Vec<String> {
    v["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_owned())
        .collect()
}

async fn payments(h: &Harness) -> Uuid {
    h.service(json!({
        "business_name": "Card Payments",
        "operational_name": "payments-api",
        "domain": "Payments",
        "criticality": "critical",
        "support_route": "#payments-oncall, rota PAY-1",
        "aliases": ["cardpay", "pay-gateway"],
    }))
    .await
}

async fn team(h: &Harness, name: &str, members: &[Uuid]) -> Uuid {
    let (status, body, raw) = h
        .call(
            "POST",
            "/v1/catalog/teams",
            Some("admin"),
            Some(json!({"name": name, "members": members})),
        )
        .await;
    assert_eq!(status, 201, "{raw}");
    body["id"].as_str().unwrap().parse().unwrap()
}

async fn search(h: &Harness, token: &str, q: &str) -> Value {
    let (status, body, raw) = h
        .call(
            "GET",
            &format!("/v1/catalog/search?q={q}"),
            Some(token),
            None,
        )
        .await;
    assert_eq!(status, 200, "{raw}");
    body
}

// --- OBS-02-01 -----------------------------------------------------------------------------

#[tokio::test]
async fn search_by_each_approved_identifier_returns_the_service() {
    let h = Harness::start().await;
    let id = payments(&h).await;
    h.service(json!({"business_name": "Ledger", "operational_name": "ledger-svc", "domain": "Finance", "criticality": "high"}))
        .await;
    let owners = team(&h, "Payments Squad", &[h.member]).await;
    let (s, _, raw) = h
        .call(
            "PUT",
            &format!("/v1/catalog/services/{id}/owners/operational_owner"),
            Some("admin"),
            Some(json!({"principal_kind": "team", "principal_id": owners})),
        )
        .await;
    assert_eq!(s, 200, "{raw}");
    let (s, _, raw) = h
        .call(
            "PUT",
            &format!("/v1/catalog/services/{id}/owners/business_owner"),
            Some("admin"),
            Some(json!({"principal_kind": "user", "principal_id": h.admin})),
        )
        .await;
    assert_eq!(s, 200, "{raw}");

    for (q, field) in [
        ("card%20pay", "business_name"),
        ("Card%20Payments", "business_name"),
        ("payments-api", "operational_name"),
        ("cardpay", "alias"),
        ("payments", "domain"),
        ("payments%20squad", "owner"),
        (&h.admin.to_string()[..], "owner"),
    ] {
        let body = search(&h, "viewer", q).await;
        assert_eq!(ids(&body), vec![id.to_string()], "query {q}");
        assert!(
            body["items"][0]["matched_on"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f == field),
            "query {q}: {body}"
        );
        assert!(body.get("message").is_none());
    }
}

#[tokio::test]
async fn search_for_retired_alias_or_unknown_identifier_finds_nothing() {
    let h = Harness::start().await;
    let id = payments(&h).await;
    let (s, _, raw) = h
        .call(
            "PATCH",
            &format!("/v1/catalog/services/{id}"),
            Some("admin"),
            Some(json!({"retire_aliases": ["pay-gateway"]})),
        )
        .await;
    assert_eq!(s, 200, "{raw}");
    for q in ["pay-gateway", "pay-gate", "unknown-thing", "zz"] {
        let body = search(&h, "viewer", q).await;
        assert_eq!(body["items"], json!([]), "query {q}");
        assert_eq!(body["total"], 0);
        assert_eq!(body["message"], "No service matches this identifier.");
    }
    // The active alias still resolves.
    assert_eq!(
        ids(&search(&h, "viewer", "cardpay").await),
        vec![id.to_string()]
    );
}

// --- OBS-02-02 -----------------------------------------------------------------------------

#[tokio::test]
async fn critical_service_shows_current_owners_and_support_route() {
    let h = Harness::start().await;
    let id = payments(&h).await;
    let squad = team(&h, "Payments Squad", &[h.member, h.viewer]).await;
    for (role, body) in [
        (
            "business_owner",
            json!({"principal_kind": "user", "principal_id": h.admin}),
        ),
        (
            "operational_owner",
            json!({"principal_kind": "team", "principal_id": squad}),
        ),
    ] {
        let (s, _, raw) = h
            .call(
                "PUT",
                &format!("/v1/catalog/services/{id}/owners/{role}"),
                Some("admin"),
                Some(body),
            )
            .await;
        assert_eq!(s, 200, "{raw}");
    }
    let (s, detail, raw) = h
        .call(
            "GET",
            &format!("/v1/catalog/services/{id}"),
            Some("viewer"),
            None,
        )
        .await;
    assert_eq!(s, 200, "{raw}");
    assert_eq!(detail["support_route"], "#payments-oncall, rota PAY-1");
    let owners = detail["owners"].as_array().unwrap();
    let by_role = |r: &str| owners.iter().find(|o| o["role"] == r).cloned().unwrap();
    assert_eq!(
        by_role("business_owner")["principal_id"],
        h.admin.to_string()
    );
    assert_eq!(by_role("business_owner")["state"], "active");
    assert_eq!(by_role("operational_owner")["team_name"], "Payments Squad");
    assert_eq!(by_role("operational_owner")["state"], "active");
    assert!(
        !detail["exceptions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e.as_str().unwrap().contains("owner")),
        "{detail}"
    );
}

#[tokio::test]
async fn departed_owner_flags_service_as_lacking_accountable_ownership() {
    let h = Harness::start().await;
    let id = payments(&h).await;
    let squad = team(&h, "Payments Squad", &[h.member]).await;
    for (role, body) in [
        (
            "business_owner",
            json!({"principal_kind": "user", "principal_id": h.viewer}),
        ),
        (
            "operational_owner",
            json!({"principal_kind": "team", "principal_id": squad}),
        ),
    ] {
        let (s, _, raw) = h
            .call(
                "PUT",
                &format!("/v1/catalog/services/{id}/owners/{role}"),
                Some("admin"),
                Some(body),
            )
            .await;
        assert_eq!(s, 200, "{raw}");
    }
    // The business owner and the only team member leave the tenant.
    h.directory.remove_member(h.tenant, h.viewer);
    h.directory.remove_member(h.tenant, h.member);

    let (_, detail, _) = h
        .call(
            "GET",
            &format!("/v1/catalog/services/{id}"),
            Some("admin"),
            None,
        )
        .await;
    let states: Vec<_> = detail["owners"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["state"].clone())
        .collect();
    assert_eq!(states, vec![json!("departed"), json!("departed")]);
    let exceptions = detail["exceptions"].as_array().unwrap();
    assert!(
        exceptions.contains(&json!("business_owner_departed")),
        "{detail}"
    );
    assert!(
        exceptions.contains(&json!("operational_owner_departed")),
        "{detail}"
    );

    let (s, list, _) = h
        .call("GET", "/v1/catalog/exceptions", Some("admin"), None)
        .await;
    assert_eq!(s, 200);
    let item = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == id.to_string())
        .cloned()
        .unwrap();
    assert!(
        item["reasons"]
            .as_array()
            .unwrap()
            .contains(&json!("business_owner_departed"))
    );

    // A departed user cannot be assigned as owner.
    let (s, _, _) = h
        .call(
            "PUT",
            &format!("/v1/catalog/services/{id}/owners/business_owner"),
            Some("admin"),
            Some(json!({"principal_kind": "user", "principal_id": h.viewer})),
        )
        .await;
    assert_eq!(s, 400);
}

#[tokio::test]
async fn unresolvable_owner_is_flagged_not_assumed_accountable() {
    let h = Harness::start().await;
    let id = payments(&h).await;
    let (s, _, _) = h
        .call(
            "PUT",
            &format!("/v1/catalog/services/{id}/owners/business_owner"),
            Some("admin"),
            Some(json!({"principal_kind": "user", "principal_id": h.admin})),
        )
        .await;
    assert_eq!(s, 200);
    h.directory
        .set_scenario(observable_auth::principals::MockScenario::Unavailable);
    let (s, detail, _) = h
        .call(
            "GET",
            &format!("/v1/catalog/services/{id}"),
            Some("admin"),
            None,
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(detail["owners"][0]["state"], "unknown");
    assert!(
        detail["exceptions"]
            .as_array()
            .unwrap()
            .contains(&json!("business_owner_unresolved"))
    );
}

// --- OBS-02-04 -----------------------------------------------------------------------------

#[tokio::test]
async fn retired_services_are_differentiated_in_search_and_listing() {
    let h = Harness::start().await;
    let live = payments(&h).await;
    let retired = h
        .service(json!({"business_name": "Card Payments Legacy", "operational_name": "payments-v1", "criticality": "critical", "lifecycle": "retired"}))
        .await;

    let body = search(&h, "viewer", "card").await;
    let items = body["items"].as_array().unwrap();
    let find = |id: Uuid| {
        items
            .iter()
            .find(|i| i["id"] == id.to_string())
            .cloned()
            .unwrap()
    };
    assert_eq!(find(live)["lifecycle"], "live");
    assert_eq!(find(live)["live"], true);
    assert_eq!(find(retired)["lifecycle"], "retired");
    assert_eq!(find(retired)["live"], false);

    let (_, live_only, _) = h
        .call(
            "GET",
            "/v1/catalog/services?lifecycle=live",
            Some("viewer"),
            None,
        )
        .await;
    assert_eq!(ids(&live_only), vec![live.to_string()]);
    let (_, retired_only, _) = h
        .call(
            "GET",
            "/v1/catalog/services?lifecycle=retired",
            Some("viewer"),
            None,
        )
        .await;
    assert_eq!(ids(&retired_only), vec![retired.to_string()]);
}

#[tokio::test]
async fn retired_service_never_appears_live_in_search_or_reporting() {
    let h = Harness::start().await;
    let id = payments(&h).await;
    let (s, _, raw) = h
        .call(
            "PATCH",
            &format!("/v1/catalog/services/{id}"),
            Some("admin"),
            Some(json!({"lifecycle": "retired"})),
        )
        .await;
    assert_eq!(s, 200, "{raw}");
    let body = search(&h, "viewer", "payments-api").await;
    assert_eq!(body["items"][0]["live"], false);
    let (_, list, _) = h
        .call("GET", "/v1/catalog/services", Some("viewer"), None)
        .await;
    assert!(
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["live"] == false)
    );
    let (_, exceptions, _) = h
        .call("GET", "/v1/catalog/exceptions", Some("admin"), None)
        .await;
    assert_eq!(
        exceptions["items"],
        json!([]),
        "retired services are not reported as in-scope exceptions"
    );
}

// --- OBS-01-02 records ---------------------------------------------------------------------

#[tokio::test]
async fn critical_service_without_approved_outcome_is_an_exception() {
    let h = Harness::start().await;
    let id = payments(&h).await;
    let reasons = |v: &Value| {
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == id.to_string())
            .map(|i| i["reasons"].clone())
    };
    let (_, list, _) = h
        .call("GET", "/v1/catalog/exceptions", Some("admin"), None)
        .await;
    assert!(
        reasons(&list)
            .unwrap()
            .as_array()
            .unwrap()
            .contains(&json!("no_approved_outcome"))
    );

    let (s, created, raw) = h
        .call(
            "POST",
            &format!("/v1/catalog/services/{id}/outcomes"),
            Some("admin"),
            Some(json!({"statement": "Card authorisations complete within 2 s", "owner": {"principal_kind": "user", "principal_id": h.admin}})),
        )
        .await;
    assert_eq!(s, 201, "{raw}");
    let outcome = created["id"].as_str().unwrap();
    // An API key cannot approve: approval is an accountable human decision.
    let (s, _, _) = h
        .call(
            "POST",
            &format!("/v1/catalog/services/{id}/outcomes/{outcome}/approve"),
            Some("key-member"),
            None,
        )
        .await;
    assert_eq!(s, 403);
    let (s, _, _) = h
        .call(
            "POST",
            &format!("/v1/catalog/services/{id}/outcomes/{outcome}/approve"),
            Some("admin"),
            None,
        )
        .await;
    assert_eq!(s, 204);
    let (_, detail, _) = h
        .call(
            "GET",
            &format!("/v1/catalog/services/{id}"),
            Some("admin"),
            None,
        )
        .await;
    assert_eq!(detail["outcomes"][0]["approved"], true);
    assert_eq!(detail["outcomes"][0]["approved_by"], h.admin.to_string());
    let (_, list, _) = h
        .call("GET", "/v1/catalog/exceptions", Some("admin"), None)
        .await;
    assert!(
        !reasons(&list)
            .unwrap_or(json!([]))
            .as_array()
            .unwrap()
            .contains(&json!("no_approved_outcome"))
    );
}

// --- req-catalog-access-control ------------------------------------------------------------

async fn classified(h: &Harness) -> (Uuid, Uuid, Uuid) {
    let public = h
        .service(json!({"business_name": "Status Page", "operational_name": "status-page", "criticality": "standard", "classification": "public", "support_route": "#status"}))
        .await;
    let confidential = h
        .service(json!({"business_name": "Payroll", "operational_name": "payroll-run", "criticality": "critical", "classification": "confidential", "support_route": "#payroll"}))
        .await;
    let restricted = h
        .service(json!({"business_name": "Fraud Scoring", "operational_name": "fraud-score", "domain": "Risk", "criticality": "critical", "classification": "restricted", "support_route": "#fraud-secret", "aliases": ["frs"]}))
        .await;
    (public, confidential, restricted)
}

#[tokio::test]
async fn catalog_reads_show_only_permitted_services() {
    let h = Harness::start().await;
    let (public, confidential, restricted) = classified(&h).await;
    let list = |token: &'static str| {
        let h = &h;
        async move {
            ids(&h
                .call("GET", "/v1/catalog/services", Some(token), None)
                .await
                .1)
        }
    };
    assert_eq!(list("viewer").await, vec![public.to_string()]);
    let mut member = list("member").await;
    member.sort();
    let mut expect = vec![public.to_string(), confidential.to_string()];
    expect.sort();
    assert_eq!(member, expect);
    assert_eq!(list("admin").await.len(), 3);
    assert_eq!(
        list("other-admin").await,
        Vec::<String>::new(),
        "tenant isolation"
    );

    let (_, viewer_page, _) = h
        .call("GET", "/v1/catalog/services", Some("viewer"), None)
        .await;
    assert_eq!(
        viewer_page["total"], 1,
        "totals count only permitted records"
    );
    let (s, detail, _) = h
        .call(
            "GET",
            &format!("/v1/catalog/services/{restricted}"),
            Some("admin"),
            None,
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(detail["support_route"], "#fraud-secret");
}

#[tokio::test]
async fn restricted_lookup_and_search_match_nonexistent_responses() {
    let h = Harness::start().await;
    let (_, confidential, restricted) = classified(&h).await;
    let missing = Uuid::new_v4();

    let denied = h
        .call(
            "GET",
            &format!("/v1/catalog/services/{restricted}"),
            Some("member"),
            None,
        )
        .await;
    let absent = h
        .call(
            "GET",
            &format!("/v1/catalog/services/{missing}"),
            Some("member"),
            None,
        )
        .await;
    assert_eq!((denied.0, &denied.2), (absent.0, &absent.2));
    assert_eq!(denied.0, 404);
    let denied_viewer = h
        .call(
            "GET",
            &format!("/v1/catalog/services/{confidential}"),
            Some("viewer"),
            None,
        )
        .await;
    assert_eq!((denied_viewer.0, &denied_viewer.2), (absent.0, &absent.2));

    // Writes on a hidden record are indistinguishable from writes on a missing one.
    let w_denied = h
        .call(
            "PATCH",
            &format!("/v1/catalog/services/{restricted}"),
            Some("member"),
            Some(json!({"description": "x"})),
        )
        .await;
    let w_absent = h
        .call(
            "PATCH",
            &format!("/v1/catalog/services/{missing}"),
            Some("member"),
            Some(json!({"description": "x"})),
        )
        .await;
    assert_eq!((w_denied.0, &w_denied.2), (w_absent.0, &w_absent.2));

    let unknown = h
        .call(
            "GET",
            "/v1/catalog/search?q=no-such-service",
            Some("member"),
            None,
        )
        .await;
    for q in ["fraud", "fraud-score", "frs", "risk"] {
        let hidden = h
            .call(
                "GET",
                &format!("/v1/catalog/search?q={q}"),
                Some("member"),
                None,
            )
            .await;
        assert_eq!((hidden.0, &hidden.2), (unknown.0, &unknown.2), "query {q}");
    }
    let (_, exceptions, raw) = h
        .call("GET", "/v1/catalog/exceptions", Some("viewer"), None)
        .await;
    assert_eq!(exceptions["items"], json!([]), "{raw}");
    assert!(!raw.contains("fraud") && !raw.contains(&restricted.to_string()));
}

#[tokio::test]
async fn catalog_reads_fail_closed_when_decision_point_unavailable() {
    let h = Harness::start().await;
    let (public, _, _) = classified(&h).await;
    h.decider.set_scenario(MockScenario::Unavailable);
    for uri in [
        "/v1/catalog/services".to_owned(),
        format!("/v1/catalog/services/{public}"),
        "/v1/catalog/search?q=status".to_owned(),
        "/v1/catalog/exceptions".to_owned(),
    ] {
        let (status, _, raw) = h.call("GET", &uri, Some("admin"), None).await;
        assert_eq!(status, 503, "{uri}");
        assert_eq!(raw, r#"{"error":"access_decision_unavailable"}"#, "{uri}");
    }
    // A missing ID fails like an existing one, so an outage cannot reveal which IDs exist.
    let absent = Uuid::new_v4();
    for (method, uri) in [
        ("GET", format!("/v1/catalog/services/{public}")),
        ("GET", format!("/v1/catalog/services/{absent}")),
        ("GET", format!("/v1/catalog/services/{absent}/bindings")),
        ("GET", format!("/v1/catalog/journeys/{absent}")),
        ("GET", format!("/v1/catalog/providers/{absent}")),
        ("DELETE", "/v1/catalog/assets/runbook/absent".to_owned()),
    ] {
        let (status, _, raw) = h.call(method, &uri, Some("admin"), None).await;
        assert_eq!(status, 503, "{method} {uri}");
        assert_eq!(raw, r#"{"error":"access_decision_unavailable"}"#, "{uri}");
    }
    for (verb, body) in [
        (
            "remediate",
            json!({"owner": {"principal_kind": "team", "principal_id": absent}, "due_date": "2030-01-01"}),
        ),
        ("close", json!({})),
    ] {
        let uri = format!("/v1/catalog/gaps/{absent}/{verb}");
        let (status, _, raw) = h.call("POST", &uri, Some("admin"), Some(body)).await;
        assert_eq!(
            (status, raw.as_str()),
            (503, r#"{"error":"access_decision_unavailable"}"#),
            "{uri}"
        );
    }
}

// --- writes ---------------------------------------------------------------------------------

#[tokio::test]
async fn writes_are_authorized_and_validated() {
    let h = Harness::start().await;
    let body = json!({"business_name": "X", "operational_name": "x", "criticality": "standard"});
    assert_eq!(
        h.call(
            "POST",
            "/v1/catalog/services",
            Some("viewer"),
            Some(body.clone())
        )
        .await
        .0,
        403
    );
    let mut confidential = body.clone();
    confidential["classification"] = json!("confidential");
    assert_eq!(
        h.call(
            "POST",
            "/v1/catalog/services",
            Some("member"),
            Some(confidential)
        )
        .await
        .0,
        403
    );
    assert_eq!(
        h.call(
            "POST",
            "/v1/catalog/services",
            Some("member"),
            Some(body.clone())
        )
        .await
        .0,
        201
    );
    assert_eq!(
        h.call("POST", "/v1/catalog/services", Some("member"), Some(body))
            .await
            .0,
        409
    );
    let bad = json!({"business_name": "Y", "operational_name": "y", "criticality": "urgent"});
    assert_eq!(
        h.call("POST", "/v1/catalog/services", Some("admin"), Some(bad))
            .await
            .0,
        400
    );
    assert_eq!(
        h.call("GET", "/v1/catalog/services", None, None).await.0,
        401
    );
}
