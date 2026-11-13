//! Runs the versioned assess-service-health scenarios used by the provider mock through the
//! actual router, with real PostgreSQL snapshots produced by the evaluator.

mod common;

use std::collections::HashMap;

use common::{Harness, inputs_for, ns_ago, view};
use health_service::ports::{EvidenceTimes, TelemetryIdentity};
use observable_auth::authz::MockScenario;
use serde_json::{Value, json};
use uuid::Uuid;

#[tokio::test]
async fn assess_service_health_read_contract() {
    let h = Harness::start().await;
    let fixture: Value =
        serde_json::from_str(include_str!("../mocks/assess-service-health.v0.1.json"))
            .expect("health read contract");
    let mut ids: HashMap<String, Uuid> = HashMap::new();
    let mut evaluated = vec![];
    let mut later = vec![];
    for f in fixture["fixtures"].as_array().unwrap() {
        let name = f["name"].as_str().unwrap();
        let id = Uuid::new_v4();
        ids.insert(name.into(), id);
        let class = f["classification"].as_str().unwrap();
        // Every fixture is live when evaluated; the catalog may say otherwise at read time.
        h.services
            .put(h.tenant, view(id, f["lifecycle"].as_str().unwrap(), class));
        if f["hidden"] == true {
            h.services.hide(id);
        }
        for (env, key) in [("prod", "production"), ("staging", "non_production")] {
            let at = match f[key].as_str() {
                Some("fresh") => Some(ns_ago(30)),
                Some("stale") => Some(ns_ago(3600)),
                _ => None,
            };
            h.evidence.record(
                h.tenant,
                &TelemetryIdentity {
                    service_name: name.into(),
                    environment: env.into(),
                },
                EvidenceTimes {
                    traces: at,
                    logs: None,
                    metrics: None,
                },
            );
        }
        let record = inputs_for(id, name, "live");
        if f.get("production").is_some() {
            evaluated.push(record);
        } else {
            later.push(record);
        }
    }
    h.inputs.set(h.tenant, evaluated.clone());
    let summary = h.evaluate().await;
    assert_eq!(summary.failed, 0, "{summary:?}");
    // Afterwards the catalog reports current lifecycles and the unevaluated service.
    let mut current: Vec<_> = evaluated
        .into_iter()
        .map(|mut s| {
            if s.id == ids["retired_after_evaluation"] {
                s.lifecycle = "retired".into();
            }
            s
        })
        .collect();
    current.extend(later);
    h.inputs.set(h.tenant, current);

    let mut outage_bodies = vec![];
    for s in fixture["scenarios"].as_array().unwrap() {
        let name = s["name"].as_str().unwrap();
        let id = ids
            .get(s["service"].as_str().unwrap())
            .copied()
            .unwrap_or_else(Uuid::new_v4);
        let unavailable = s["unavailable"] == true;
        h.decider.set_scenario(if unavailable {
            MockScenario::Unavailable
        } else {
            MockScenario::Policy
        });
        let caller = s["caller"].as_str().unwrap_or("member");
        let (status, body, raw) = h.health(id, s["class"].as_str().unwrap(), caller).await;
        assert_eq!(json!(status), s["status"], "{name}: {raw}");
        if status != 200 {
            assert_eq!(body["error"], s["error"], "{name}: {raw}");
            assert!(body.get("state").is_none(), "{name}: {raw}");
            if unavailable {
                outage_bodies.push(raw);
            }
            continue;
        }
        assert_eq!(body["service_id"], json!(id), "{name}");
        assert_eq!(body["environment_class"], s["class"], "{name}");
        assert_eq!(body["state"], s["state"], "{name}: {raw}");
        assert_eq!(
            body["evidence_status"], s["evidence_status"],
            "{name}: {raw}"
        );
        let has_times = !body["last_evidence_at"].is_null();
        assert_eq!(json!(has_times), s["times"], "{name}: {raw}");
        if s["evidence_status"] == "restricted" {
            assert_eq!(body["conditions"], json!([]), "{name}");
            assert!(body["evaluated_at"].is_null(), "{name}");
        }
    }
    // Existing, restricted and missing IDs are indistinguishable during a policy outage.
    assert_eq!(outage_bodies.len(), 3);
    assert!(outage_bodies.windows(2).all(|w| w[0] == w[1]));
}
