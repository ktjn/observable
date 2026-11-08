//! Providers and dependencies (phase 1 S7, OBS-36-01) in the standalone harness: real catalog
//! schema on PostgreSQL and the provider-owned mocks for decide-access and lookup-principals.

mod common;

use common::Harness;
use observable_auth::authz::MockScenario;
use serde_json::{Value, json};
use uuid::Uuid;

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

async fn provider(h: &Harness, body: Value) -> Uuid {
    let json = ok(h, "POST", "/v1/catalog/providers", "admin", Some(body)).await;
    json["id"].as_str().unwrap().parse().unwrap()
}

async fn depend(h: &Harness, token: &str, body: Value) -> (u16, Value, String) {
    h.call(
        "POST",
        "/v1/catalog/providers/dependencies",
        Some(token),
        Some(body),
    )
    .await
}

fn reasons(gaps: &Value) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = gaps["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            let who = g["provider"]["name"]
                .as_str()
                .or_else(|| g["provider_name"].as_str())
                .unwrap()
                .to_owned();
            (g["reason"].as_str().unwrap().to_owned(), who)
        })
        .collect();
    out.sort();
    out
}

#[tokio::test]
async fn provider_record_lists_dependent_critical_functions_and_contract_owner() {
    let h = Harness::start().await;
    let payments = svc(&h, "Card Payments", "critical", json!({})).await;
    let journey = ok(
        &h,
        "POST",
        "/v1/catalog/journeys",
        "admin",
        Some(json!({"name": "Checkout", "criticality": "critical", "service_ids": [payments]})),
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let stripe = provider(
        &h,
        json!({
            "name": "Stripe", "kind": "payment", "contract_ref": "MSA-2026-014",
            "contract_owner": {"principal_kind": "user", "principal_id": h.admin},
        }),
    )
    .await;
    for body in [
        json!({"provider_id": stripe, "service_id": payments, "function": "card authorisation"}),
        json!({"provider_id": stripe, "journey_id": journey, "function": "payment step"}),
    ] {
        let (status, json, raw) = depend(&h, "member", body).await;
        assert_eq!(status, 201, "{raw}");
        assert_eq!(json["registered"], true);
        assert_eq!(json["provider"]["name"], "Stripe");
    }

    let detail = ok(
        &h,
        "GET",
        &format!("/v1/catalog/providers/{stripe}"),
        "viewer",
        None,
    )
    .await;
    assert_eq!(detail["contract_ref"], "MSA-2026-014");
    assert_eq!(detail["contract_owner"]["principal_id"], json!(h.admin));
    assert_eq!(detail["contract_owner"]["state"], "active");
    let mut functions: Vec<(String, String)> = detail["dependents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            let target = d["service"]["business_name"]
                .as_str()
                .or_else(|| d["journey"]["name"].as_str())
                .unwrap()
                .to_owned();
            (d["function"].as_str().unwrap().to_owned(), target)
        })
        .collect();
    functions.sort();
    assert_eq!(
        functions,
        vec![
            ("card authorisation".into(), "Card Payments".into()),
            ("payment step".into(), "Checkout".into()),
        ]
    );
    assert_eq!(detail["dependents_withheld"], false);
    assert_eq!(detail["exceptions"], json!([]));

    let deps = ok(
        &h,
        "GET",
        &format!("/v1/catalog/services/{payments}/dependencies"),
        "viewer",
        None,
    )
    .await;
    assert_eq!(deps["total"], 1);
    assert_eq!(deps["items"][0]["provider"]["id"], json!(stripe));
}

#[tokio::test]
async fn critical_function_on_an_unregistered_provider_is_a_gap() {
    let h = Harness::start().await;
    let payments = svc(&h, "Card Payments", "critical", json!({})).await;
    let reports = svc(&h, "Reporting", "standard", json!({})).await;
    for (service, function) in [(payments, "fraud scoring"), (reports, "exports")] {
        let (status, json, raw) = depend(
            &h,
            "member",
            json!({"provider_name": "Sift", "service_id": service, "function": function}),
        )
        .await;
        assert_eq!(status, 201, "{raw}");
        assert_eq!(
            (json["registered"].clone(), json["provider_name"].clone()),
            (json!(false), json!("Sift"))
        );
    }

    let gaps = ok(&h, "GET", "/v1/catalog/providers/gaps", "viewer", None).await;
    // Only the critical function is a gap.
    assert_eq!(gaps["total"], 1);
    assert_eq!(gaps["items"][0]["reason"], "unregistered_provider");
    assert_eq!(gaps["items"][0]["provider_name"], "Sift");
    assert_eq!(gaps["items"][0]["dependent"]["function"], "fraud scoring");
    assert_eq!(
        gaps["items"][0]["dependent"]["service"]["id"],
        json!(payments)
    );
    let exceptions = ok(&h, "GET", "/v1/catalog/exceptions", "admin", None).await;
    let item = exceptions["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == json!(payments))
        .unwrap()
        .clone();
    assert!(
        item["reasons"]
            .as_array()
            .unwrap()
            .contains(&json!("unregistered_provider"))
    );

    // Registering the provider (any letter case) links the dependency and closes the gap.
    let sift = provider(
        &h,
        json!({"name": "sift", "kind": "saas",
               "contract_owner": {"principal_kind": "user", "principal_id": h.admin}}),
    )
    .await;
    let gaps = ok(&h, "GET", "/v1/catalog/providers/gaps", "viewer", None).await;
    assert_eq!(gaps["total"], 0, "{gaps}");
    let detail = ok(
        &h,
        "GET",
        &format!("/v1/catalog/providers/{sift}"),
        "admin",
        None,
    )
    .await;
    assert_eq!(detail["dependents"].as_array().unwrap().len(), 2);

    // Deleting the provider returns its dependencies to unregistered.
    ok(
        &h,
        "DELETE",
        &format!("/v1/catalog/providers/{sift}"),
        "admin",
        None,
    )
    .await;
    let gaps = ok(&h, "GET", "/v1/catalog/providers/gaps", "viewer", None).await;
    assert_eq!(
        reasons(&gaps),
        vec![("unregistered_provider".into(), "sift".into())]
    );
}

#[tokio::test]
async fn material_providers_need_an_active_contract_owner_and_a_dependent_function() {
    let h = Harness::start().await;
    let leaver = Uuid::new_v4();
    h.directory.add_member(h.tenant, leaver);
    provider(&h, json!({"name": "AWS", "kind": "cloud"})).await;
    provider(
        &h,
        json!({"name": "Coffee Club", "kind": "other", "material": false}),
    )
    .await;
    let cdn = provider(
        &h,
        json!({"name": "Fastly", "kind": "network",
               "contract_owner": {"principal_kind": "user", "principal_id": leaver}}),
    )
    .await;
    let edge = svc(&h, "Edge", "high", json!({})).await;
    let (status, _, raw) = depend(
        &h,
        "admin",
        json!({"provider_id": cdn, "service_id": edge, "function": "content delivery"}),
    )
    .await;
    assert_eq!(status, 201, "{raw}");
    h.directory.remove_member(h.tenant, leaver);

    let gaps = ok(&h, "GET", "/v1/catalog/providers/gaps", "viewer", None).await;
    assert_eq!(
        reasons(&gaps),
        vec![
            ("contract_owner_departed".into(), "Fastly".into()),
            ("missing_contract_owner".into(), "AWS".into()),
            ("unmapped_material_provider".into(), "AWS".into()),
        ]
    );
    let detail = ok(
        &h,
        "GET",
        &format!("/v1/catalog/providers/{cdn}"),
        "viewer",
        None,
    )
    .await;
    assert_eq!(detail["exceptions"], json!(["contract_owner_departed"]));

    // Input is validated.
    for body in [
        json!({"name": "X", "kind": "telepathy"}),
        json!({"name": " ", "kind": "saas"}),
        json!({"name": "Y", "kind": "saas", "contract_owner": {"principal_kind": "user", "principal_id": Uuid::new_v4()}}),
    ] {
        let (status, _, _) = h
            .call(
                "POST",
                "/v1/catalog/providers",
                Some("admin"),
                Some(body.clone()),
            )
            .await;
        assert_eq!(status, 400, "{body}");
    }
    let (status, _, _) = h
        .call(
            "POST",
            "/v1/catalog/providers",
            Some("admin"),
            Some(json!({"name": "aws", "kind": "cloud"})),
        )
        .await;
    assert_eq!(status, 409);
    let (status, _, _) = depend(
        &h,
        "admin",
        json!({"provider_name": "A", "provider_id": cdn, "service_id": edge, "function": "f"}),
    )
    .await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn provider_access_is_decided_and_hidden_records_are_withheld() {
    let h = Harness::start().await;
    let secret_svc = svc(
        &h,
        "Fraud Engine",
        "critical",
        json!({"classification": "restricted"}),
    )
    .await;
    let public_svc = svc(&h, "Card Payments", "critical", json!({})).await;
    let secret = provider(
        &h,
        json!({"name": "Shadow Bank", "kind": "payment", "classification": "restricted"}),
    )
    .await;
    let open = provider(&h, json!({"name": "Stripe", "kind": "payment"})).await;
    for (p, s, f) in [
        (secret, public_svc, "settlement"),
        (open, public_svc, "authorisation"),
        (open, secret_svc, "scoring"),
    ] {
        let (status, _, raw) = depend(
            &h,
            "admin",
            json!({"provider_id": p, "service_id": s, "function": f}),
        )
        .await;
        assert_eq!(status, 201, "{raw}");
    }
    // An unregistered dependency of a hidden critical service is not a viewer's gap.
    depend(
        &h,
        "admin",
        json!({"provider_name": "Ghost", "service_id": secret_svc, "function": "x"}),
    )
    .await;

    let listed = ok(&h, "GET", "/v1/catalog/providers", "viewer", None).await;
    assert_eq!(listed["total"], 1);
    assert_eq!(listed["items"][0]["name"], "Stripe");

    let (s1, _, hidden_raw) = h
        .call(
            "GET",
            &format!("/v1/catalog/providers/{secret}"),
            Some("viewer"),
            None,
        )
        .await;
    let (s2, _, missing_raw) = h
        .call(
            "GET",
            &format!("/v1/catalog/providers/{}", Uuid::new_v4()),
            Some("viewer"),
            None,
        )
        .await;
    assert_eq!((s1, s2), (404, 404));
    assert_eq!(hidden_raw, missing_raw);

    let detail = ok(
        &h,
        "GET",
        &format!("/v1/catalog/providers/{open}"),
        "viewer",
        None,
    )
    .await;
    assert_eq!(detail["dependents"].as_array().unwrap().len(), 1);
    assert_eq!(detail["dependents_withheld"], true);
    assert!(!detail.to_string().contains(&secret_svc.to_string()));

    // A registered provider the caller may not read is withheld, never named.
    let deps = ok(
        &h,
        "GET",
        &format!("/v1/catalog/services/{public_svc}/dependencies"),
        "viewer",
        None,
    )
    .await;
    let raw = deps.to_string();
    assert!(!raw.contains("Shadow Bank") && !raw.contains(&secret.to_string()));
    assert!(
        deps["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["withheld"] == true && d["registered"] == true)
    );
    let gaps = ok(&h, "GET", "/v1/catalog/providers/gaps", "viewer", None).await;
    assert!(!gaps.to_string().contains("Ghost"));

    // Writes: viewers cannot create providers; a dependency on a hidden service is refused
    // exactly like one on a missing service; a hidden provider is refused like a missing one.
    let (status, _, _) = h
        .call(
            "POST",
            "/v1/catalog/providers",
            Some("viewer"),
            Some(json!({"name": "V", "kind": "saas"})),
        )
        .await;
    assert_eq!(status, 403);
    let (s1, b1, _) = depend(
        &h,
        "member",
        json!({"provider_id": open, "service_id": secret_svc, "function": "probe"}),
    )
    .await;
    let (s2, b2, _) = depend(
        &h,
        "member",
        json!({"provider_id": open, "service_id": Uuid::new_v4(), "function": "probe"}),
    )
    .await;
    assert_eq!((s1, s2), (400, 400));
    assert_eq!(b1, b2);
    let (s1, b1, _) = depend(
        &h,
        "viewer",
        json!({"provider_id": open, "service_id": public_svc, "function": "probe"}),
    )
    .await;
    assert_eq!((s1, b1), (403, json!({"error": "forbidden"})));
    let (s1, b1, _) = depend(
        &h,
        "member",
        json!({"provider_id": secret, "service_id": public_svc, "function": "probe"}),
    )
    .await;
    let (s2, b2, _) = depend(
        &h,
        "member",
        json!({"provider_id": Uuid::new_v4(), "service_id": public_svc, "function": "probe"}),
    )
    .await;
    assert_eq!((s1, s2), (400, 400));
    assert_eq!(b1, b2);

    // Unavailable decision point: reads fail closed.
    h.decider.set_scenario(MockScenario::Unavailable);
    for uri in [
        "/v1/catalog/providers".to_owned(),
        format!("/v1/catalog/providers/{open}"),
        "/v1/catalog/providers/gaps".to_owned(),
        format!("/v1/catalog/services/{public_svc}/dependencies"),
    ] {
        let (status, body, _) = h.call("GET", &uri, Some("admin"), None).await;
        assert_eq!(status, 503, "{uri}");
        assert_eq!(body, json!({"error": "access_decision_unavailable"}));
    }
}
