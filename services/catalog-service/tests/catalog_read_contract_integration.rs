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
            "domain": "Commerce", "criticality": "critical"
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
    }
}
