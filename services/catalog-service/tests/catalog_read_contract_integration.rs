//! Runs the versioned read scenarios used by the catalog's programmable mock.

mod common;

use common::Harness;
use observable_auth::authz::MockScenario;
use serde_json::{Value, json};
use uuid::Uuid;

#[tokio::test]
async fn manage_service_catalog_read_contract() {
    let h = Harness::start().await;
    let visible = h
        .service(json!({
            "business_name": "Checkout", "operational_name": "checkout-api",
            "domain": "Commerce", "criticality": "critical", "lifecycle": "retired"
        }))
        .await;
    let restricted = h
        .service(json!({
            "business_name": "Restricted Payments", "operational_name": "checkout-restricted",
            "criticality": "critical", "classification": "restricted"
        }))
        .await;
    let missing = Uuid::new_v4();
    let fixture: Value =
        serde_json::from_str(include_str!("../mocks/manage-service-catalog.v0.1.json"))
            .expect("catalog read contract");
    let mut unordered = fixture["bindings"].as_array().expect("bindings").clone();
    unordered.reverse();
    let binding_path = format!("/v1/catalog/services/{visible}/bindings");
    let (status, body, raw) = h.call("GET", &binding_path, Some("viewer"), None).await;
    assert_eq!(status, 200, "{raw}");
    assert_eq!(body, json!({"bindings": []}));
    for (id, bindings) in [
        (visible, json!(unordered)),
        (
            restricted,
            json!([{"service_name": "restricted-api", "environment": "prod"}]),
        ),
    ] {
        let (status, _, raw) = h
            .call(
                "PUT",
                &format!("/v1/catalog/services/{id}/bindings"),
                Some("admin"),
                Some(json!({"bindings": bindings})),
            )
            .await;
        assert_eq!(status, 200, "{raw}");
    }
    let (status, body, raw) = h
        .call("GET", &binding_path, Some("other-admin"), None)
        .await;
    assert_eq!(status, 404, "{raw}");
    assert_eq!(body, json!({"error": "not_found"}));
    for expected in fixture["scenarios"].as_array().expect("scenarios") {
        if expected["unavailable"] == true {
            h.decider.set_scenario(MockScenario::Unavailable);
        }
        let path = expected["path"]
            .as_str()
            .expect("path")
            .replace("${visible}", &visible.to_string())
            .replace("${restricted}", &restricted.to_string())
            .replace("${missing}", &missing.to_string());
        let (status, body, raw) = h
            .call("GET", &format!("/v1/catalog/{path}"), Some("viewer"), None)
            .await;
        assert_eq!(json!(status), expected["status"], "{path}: {raw}");
        for (actual, want) in [
            ("error", "error"),
            ("business_name", "name"),
            ("total", "total"),
            ("message", "message"),
        ] {
            assert_eq!(body[actual], expected[want], "{path}: {raw}");
        }
        let names = body["items"].as_array().map(|items| {
            items
                .iter()
                .map(|item| &item["business_name"])
                .collect::<Vec<_>>()
        });
        assert_eq!(json!(names), expected["names"], "{path}: {raw}");
        if path.ends_with("/bindings") {
            let expected_body = if expected["binding_set"].is_string() {
                json!({"bindings": fixture["bindings"]})
            } else {
                json!({"error": expected["error"]})
            };
            assert_eq!(body, expected_body, "{path}: {raw}");
        }
    }
}
