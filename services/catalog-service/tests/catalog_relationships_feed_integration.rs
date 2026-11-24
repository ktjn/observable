//! The relationships feed alerting's guidance ledger reads (W4a, OBS-39): internal token only,
//! one tenant, current owners with their assignment time, journey members, bindings,
//! provider dependencies and open owner conflicts.

mod common;

use common::{Harness, INTERNAL_TOKEN};
use serde_json::{Value, json};

async fn ok(h: &Harness, method: &str, uri: &str, body: Value) -> Value {
    let (status, json, raw) = h.call(method, uri, Some("admin"), Some(body)).await;
    assert!(
        (200..300).contains(&status),
        "{method} {uri} -> {status} {raw}"
    );
    json
}

#[tokio::test]
async fn feed_lists_the_tenants_current_relationships_for_internal_callers_only() {
    let h = Harness::start().await;
    let checkout = h
        .service(
            json!({"business_name": "Checkout", "operational_name": "checkout",
                        "criticality": "critical"}),
        )
        .await;
    ok(
        &h,
        "PUT",
        &format!("/v1/catalog/services/{checkout}/bindings"),
        json!({"bindings": [{"service_name": "checkout", "environment": "prod"}]}),
    )
    .await;
    ok(
        &h,
        "PUT",
        &format!("/v1/catalog/services/{checkout}/owners/operational_owner"),
        json!({"principal_kind": "user", "principal_id": h.member}),
    )
    .await;
    let journey = ok(
        &h,
        "POST",
        "/v1/catalog/journeys",
        json!({"name": "Buy", "criticality": "critical", "service_ids": [checkout]}),
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let provider = ok(
        &h,
        "POST",
        "/v1/catalog/providers",
        json!({"name": "CardCo", "kind": "payment"}),
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &h,
        "POST",
        "/v1/catalog/providers/dependencies",
        json!({"provider_id": provider, "service_id": checkout, "function": "card auth"}),
    )
    .await;
    // Another tenant's service never appears.
    let (status, _, raw) = h
        .call(
            "POST",
            "/v1/catalog/services",
            Some("other-admin"),
            Some(
                json!({"business_name": "Elsewhere", "operational_name": "elsewhere",
                        "criticality": "low"}),
            ),
        )
        .await;
    assert_eq!(status, 201, "{raw}");

    let uri = format!("/internal/catalog/relationships?tenant_id={}", h.tenant);
    for (user, internal) in [(Some("admin"), None), (None, Some("wrong-token"))] {
        let (status, _, _) = h.call_with("GET", &uri, user, internal, None).await;
        assert_eq!(status, 401);
    }
    let (status, feed, raw) = h
        .call_with("GET", &uri, None, Some(INTERNAL_TOKEN), None)
        .await;
    assert_eq!(status, 200, "{raw}");
    let names: Vec<&str> = feed["services"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Checkout"]);
    assert_eq!(
        feed["bindings"],
        json!([{"service_id": checkout, "service_name": "checkout", "environment": "prod"}])
    );
    assert_eq!(feed["journeys"][0]["id"], json!(journey));
    assert_eq!(
        feed["journey_services"],
        json!([{"journey_id": journey, "service_id": checkout}])
    );
    let owner = &feed["owners"][0];
    assert_eq!(owner["service_id"], json!(checkout));
    assert_eq!(owner["role"], "operational_owner");
    assert_eq!(owner["principal_id"], json!(h.member));
    assert!(owner["assigned_at"].is_string());
    assert_eq!(feed["providers"][0]["name"], "CardCo");
    let dep = &feed["provider_dependencies"][0];
    assert_eq!(
        (&dep["provider_id"], &dep["service_id"], &dep["function"]),
        (&json!(provider), &json!(checkout), &json!("card auth"))
    );
    assert_eq!(feed["owner_conflicts"], json!([]));
}
