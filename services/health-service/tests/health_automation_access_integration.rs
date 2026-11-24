//! OBS-16-07: an approved automation (an API key assigned the viewer role) reads service
//! status through the same routes and the same decide-access policy as a user, and gets
//! nothing its role does not grant.

mod common;

use common::{Harness, inputs_for, ns_ago, view};
use health_service::ports::{EvidenceTimes, TelemetryIdentity};
use serde_json::json;
use uuid::Uuid;

const KEY: &str = "status-automation-key";

async fn evaluated_service(h: &Harness, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    h.services.put(h.tenant, view(id, "live", "internal"));
    h.evidence.record(
        h.tenant,
        &TelemetryIdentity {
            service_name: name.into(),
            environment: "prod".into(),
        },
        EvidenceTimes {
            traces: Some(ns_ago(10)),
            ..Default::default()
        },
    );
    h.inputs.set(h.tenant, vec![inputs_for(id, name, "live")]);
    assert_eq!(h.evaluate().await.failed, 0);
    id
}

#[tokio::test]
async fn approved_automation_reads_service_status_without_broader_access_than_its_role() {
    let h = Harness::start().await;
    let id = evaluated_service(&h, "checkout").await;

    // The automation gets the same status answer a viewer gets.
    let (status, body, raw) = h.health(id, "production", KEY).await;
    assert_eq!(status, 200, "{raw}");
    let (_, viewer_body, _) = h.health(id, "production", "viewer").await;
    assert_eq!(body["state"], viewer_body["state"]);
    assert!(body["state"].is_string());
    for read in ["history", "issues"] {
        let (status, _, raw) = h
            .get(
                &format!("/v1/health/services/{id}/{read}?environment_class=production"),
                Some(KEY),
            )
            .await;
        assert_eq!(status, 200, "{read}: {raw}");
    }

    // It cannot change rules or open maintenance: writes need a tenant admin session.
    let (status, _, raw) = h
        .send(
            "PUT",
            "/v1/health/rules",
            KEY,
            &json!({"freshness_seconds": 60}).to_string(),
        )
        .await;
    assert_eq!(status, 403, "{raw}");
    let (status, _, raw) = h
        .send(
            "POST",
            &format!("/v1/health/services/{id}/maintenance"),
            KEY,
            &json!({"environment_class": "production", "reason": "x",
                    "starts_at": "2030-01-01T00:00:00Z", "ends_at": "2030-01-01T01:00:00Z"})
            .to_string(),
        )
        .await;
    assert_eq!(status, 403, "{raw}");

    // It sees only its own tenant.
    let foreign = Uuid::new_v4();
    h.services
        .put(h.other_tenant, view(foreign, "live", "internal"));
    let (status, _, _) = h.health(foreign, "production", KEY).await;
    assert_eq!(status, 404);
}
