//! Phase 1 P1 catalog governance integration tests (OBS-02-05..08).

mod common;

use axum::http::StatusCode;
use common::Harness;
use serde_json::json;

#[tokio::test]
async fn operating_hours_and_planned_maintenance_visible_in_service_context() {
    let h = Harness::start().await;
    let svc_id = h
        .service(json!({
            "business_name": "Checkout API",
            "operational_name": "checkout-api",
            "criticality": "critical",
            "operating_hours": "24x7 UTC",
        }))
        .await;

    let (status, _, raw) = h
        .call(
            "POST",
            &format!("/v1/catalog/services/{svc_id}/maintenance"),
            Some("admin"),
            Some(json!({
                "title": "Database minor upgrade",
                "starts_at": "2026-12-01T02:00:00Z",
                "ends_at": "2026-12-01T03:00:00Z",
            })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{raw}");

    let (status, detail, raw) = h
        .call(
            "GET",
            &format!("/v1/catalog/services/{svc_id}"),
            Some("member"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    assert_eq!(detail["operating_hours"], "24x7 UTC");
    let windows = detail["maintenance_windows"].as_array().expect("windows");
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0]["title"], "Database minor upgrade");
}

#[tokio::test]
async fn exception_report_lists_gap_and_remediation_owner() {
    let h = Harness::start().await;
    let svc_id = h
        .service(json!({
            "business_name": "Billing Core",
            "operational_name": "billing-core",
            "criticality": "critical",
            "support_route": "#billing-oncall",
        }))
        .await;

    let owner_id = h.member;
    let (status, _, raw) = h
        .call(
            "PUT",
            &format!("/v1/catalog/services/{svc_id}/owners/operational_owner"),
            Some("admin"),
            Some(json!({"principal_kind": "user", "principal_id": owner_id})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{raw}");

    let (status, ex, raw) = h
        .call("GET", "/v1/catalog/exceptions", Some("member"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    let items = ex["items"].as_array().expect("items");
    let item = items
        .iter()
        .find(|i| i["id"] == svc_id.to_string())
        .expect("billing-core should appear in exceptions");
    assert!(
        !item["reasons"].as_array().unwrap().is_empty(),
        "expected gap reasons"
    );
    assert_eq!(
        item["remediation_owner"]["principal_id"],
        owner_id.to_string()
    );
}

#[tokio::test]
async fn unregistered_observed_component_appears_in_exceptions_with_proposed_owner() {
    let h = Harness::start().await;
    let proposed_owner = h.member;

    let (status, _, raw) = h
        .call(
            "POST",
            "/v1/catalog/observed",
            Some("admin"),
            Some(json!({
                "operational_name": "shadow-worker",
                "component_kind": "component",
                "proposed_owner_kind": "user",
                "proposed_owner_id": proposed_owner,
            })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{raw}");

    let (status, ex, raw) = h
        .call("GET", "/v1/catalog/exceptions", Some("member"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    let obs = ex["unregistered_observed"]
        .as_array()
        .expect("unregistered_observed");
    assert_eq!(obs.len(), 1);
    assert_eq!(obs[0]["operational_name"], "shadow-worker");
    assert_eq!(obs[0]["proposed_owner_id"], proposed_owner.to_string());

    // Once registered in the catalog, it disappears from unregistered_observed.
    h.service(json!({
        "business_name": "Shadow Worker",
        "operational_name": "shadow-worker",
        "criticality": "standard",
    }))
    .await;

    let (status, ex2, raw) = h
        .call("GET", "/v1/catalog/exceptions", Some("member"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    assert!(
        ex2["unregistered_observed"].as_array().unwrap().is_empty(),
        "registered service should no longer appear in unregistered_observed"
    );
}

#[tokio::test]
async fn authoritative_owner_change_is_reflected_or_flagged_as_conflict() {
    let h = Harness::start().await;
    let svc_id = h
        .service(json!({
            "business_name": "Ledger API",
            "operational_name": "ledger-api",
            "criticality": "critical",
        }))
        .await;

    let owner_a = h.member;
    let owner_b = h.admin;

    let (status, _, raw) = h
        .call(
            "PUT",
            &format!("/v1/catalog/services/{svc_id}/owners/operational_owner"),
            Some("admin"),
            Some(json!({"principal_kind": "user", "principal_id": owner_a})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{raw}");

    // Sync without auto_apply flags a conflict when authoritative owner differs.
    let (status, sync_res, raw) = h
        .call(
            "POST",
            "/v1/catalog/services/sync-authoritative",
            Some("admin"),
            Some(json!({
                "records": [{
                    "operational_name": "ledger-api",
                    "role": "operational_owner",
                    "authoritative_principal_kind": "user",
                    "authoritative_principal_id": owner_b,
                    "auto_apply": false
                }]
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    assert_eq!(sync_res["conflicts"], 1);

    let (status, detail, raw) = h
        .call(
            "GET",
            &format!("/v1/catalog/services/{svc_id}"),
            Some("member"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    assert!(
        detail["exceptions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e == "authoritative_owner_conflict"),
        "expected authoritative_owner_conflict in {:?}",
        detail["exceptions"]
    );

    // Sync with auto_apply updates the catalog owner and clears the conflict.
    let (status, sync_res2, raw) = h
        .call(
            "POST",
            "/v1/catalog/services/sync-authoritative",
            Some("admin"),
            Some(json!({
                "records": [{
                    "operational_name": "ledger-api",
                    "role": "operational_owner",
                    "authoritative_principal_kind": "user",
                    "authoritative_principal_id": owner_b,
                    "auto_apply": true
                }]
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    assert_eq!(sync_res2["updated"], 1);

    let (status, detail2, raw) = h
        .call(
            "GET",
            &format!("/v1/catalog/services/{svc_id}"),
            Some("member"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    assert!(
        !detail2["exceptions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e == "authoritative_owner_conflict"),
        "conflict should be resolved after auto_apply"
    );
    assert_eq!(detail2["owners"][0]["principal_id"], owner_b.to_string());
}
