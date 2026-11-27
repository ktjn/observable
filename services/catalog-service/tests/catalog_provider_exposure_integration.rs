//! Third-party disruption exposure (W5b, OBS-15-06, docs/assurance-relocations.md 4.6) through
//! the routes as the scoped `observable_catalog` login: a tenant admin marks regulated
//! functions, a provider incident is recorded, and the exposure lists each affected regulated
//! function with its service or journey and dependency record.

mod common;

use common::Harness;
use serde_json::{Value, json};
use uuid::Uuid;

async fn ok(h: &Harness, method: &str, uri: &str, token: &str, body: Option<Value>) -> Value {
    let (status, json, raw) = h.call(method, uri, Some(token), body).await;
    assert!(
        (200..300).contains(&status),
        "{method} {uri} -> {status} {raw}"
    );
    json
}

fn id(v: &Value) -> Uuid {
    v["id"].as_str().unwrap().parse().unwrap()
}

async fn svc(h: &Harness, name: &str, classification: &str) -> Uuid {
    h.service(json!({
        "business_name": name,
        "operational_name": name.to_lowercase().replace(' ', "-"),
        "criticality": "critical",
        "classification": classification,
    }))
    .await
}

async fn depend(h: &Harness, body: Value) -> Uuid {
    id(&ok(
        h,
        "POST",
        "/v1/catalog/providers/dependencies",
        "admin",
        Some(body),
    )
    .await)
}

async fn functions(
    h: &Harness,
    token: &str,
    provider: Uuid,
    incident: Uuid,
) -> (Vec<String>, Value) {
    let body = ok(
        h,
        "GET",
        &format!("/v1/catalog/providers/{provider}/incidents/{incident}/exposure"),
        token,
        None,
    )
    .await;
    let mut f: Vec<String> = body["exposed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["function"].as_str().unwrap().to_owned())
        .collect();
    f.sort();
    (f, body)
}

#[tokio::test]
async fn material_provider_incident_lists_affected_regulated_functions() {
    let (owner, guard) = test_support::postgres::isolated_pool().await;
    let scoped = test_support::postgres::scoped_pool(&owner, "observable_catalog").await;
    let h = Harness::with_db(scoped, Some(guard));

    let payments = svc(&h, "Card Payments", "internal").await;
    let fraud = svc(&h, "Fraud Engine", "restricted").await;
    let journey = id(&ok(
        &h,
        "POST",
        "/v1/catalog/journeys",
        "admin",
        Some(json!({"name": "Checkout", "criticality": "critical", "service_ids": [payments]})),
    )
    .await);
    let provider = id(&ok(
        &h,
        "POST",
        "/v1/catalog/providers",
        "admin",
        Some(json!({"name": "EU Clearing", "kind": "payment"})),
    )
    .await);
    let settlement = depend(
        &h,
        json!({"provider_id": provider, "service_id": payments, "function": "card settlement"}),
    )
    .await;
    let step = depend(
        &h,
        json!({"provider_id": provider, "journey_id": journey, "function": "payment step"}),
    )
    .await;
    let _receipts = depend(
        &h,
        json!({"provider_id": provider, "service_id": payments, "function": "receipt email"}),
    )
    .await;
    let scoring = depend(
        &h,
        json!({"provider_id": provider, "service_id": fraud, "function": "fraud scoring"}),
    )
    .await;

    // Only a signed-in tenant admin marks a function as regulated.
    let mark = |d: Uuid| format!("/v1/catalog/providers/dependencies/{d}/regulated");
    for who in ["member", "viewer", "key-member"] {
        let (status, _, raw) = h
            .call(
                "PUT",
                &mark(settlement),
                Some(who),
                Some(json!({"regulated": true})),
            )
            .await;
        assert_eq!(status, 403, "{who}: {raw}");
    }
    let (status, _, _) = h
        .call(
            "PUT",
            &mark(Uuid::new_v4()),
            Some("admin"),
            Some(json!({"regulated": true})),
        )
        .await;
    assert_eq!(status, 404);
    for d in [settlement, step, scoring] {
        let body = ok(
            &h,
            "PUT",
            &mark(d),
            "admin",
            Some(json!({"regulated": true})),
        )
        .await;
        assert_eq!(body["regulated_by"], json!(h.admin));
    }

    let incidents = format!("/v1/catalog/providers/{provider}/incidents");
    let outage = json!({"title": "Clearing outage", "started_at": "2026-11-20T08:00:00Z",
                        "material": true});
    let (status, _, _) = h
        .call("POST", &incidents, Some("viewer"), Some(outage.clone()))
        .await;
    assert_eq!(status, 403, "a viewer cannot record a provider incident");
    let (status, _, _) = h
        .call(
            "POST",
            &incidents,
            Some("admin"),
            Some(json!({"title": " ", "started_at": "2026-11-20T08:00:00Z", "material": true})),
        )
        .await;
    assert_eq!(status, 400);
    let material = ok(&h, "POST", &incidents, "admin", Some(outage)).await;
    assert_eq!(material["recorded_by"], json!(format!("user:{}", h.admin)));
    let material = id(&material);

    // The admin sees every affected regulated function; unregulated ones are never listed.
    let (f, body) = functions(&h, "admin", provider, material).await;
    assert_eq!(f, ["card settlement", "fraud scoring", "payment step"]);
    assert_eq!(body["exposed_withheld"], false);
    assert_eq!(body["incident"]["material"], true);
    for e in body["exposed"].as_array().unwrap() {
        assert_eq!(e["dependency"]["provider_name"], "EU Clearing");
        assert_eq!(e["dependency"]["regulated_by"], json!(h.admin));
        match e["function"].as_str().unwrap() {
            "card settlement" => {
                assert_eq!(e["dependency"]["id"], json!(settlement));
                assert_eq!(e["service"]["business_name"], "Card Payments");
            }
            "payment step" => {
                assert_eq!(e["dependency"]["id"], json!(step));
                assert_eq!(e["journey"]["name"], "Checkout");
            }
            _ => assert_eq!(e["service"]["business_name"], "Fraud Engine"),
        }
    }

    // Negative: hidden services are omitted, never identified or counted.
    let (f, body) = functions(&h, "viewer", provider, material).await;
    assert_eq!(f, ["card settlement", "payment step"]);
    assert_eq!(body["exposed_withheld"], true);
    assert!(!body.to_string().contains("Fraud Engine"));

    // Negative: a non-material incident yields no exposure entries.
    let minor = id(&ok(
        &h,
        "POST",
        &incidents,
        "admin",
        Some(
            json!({"title": "Slow portal", "started_at": "2026-11-21T08:00:00Z",
                    "material": false}),
        ),
    )
    .await);
    let (f, body) = functions(&h, "admin", provider, minor).await;
    assert!(f.is_empty());
    assert_eq!(body["exposed_withheld"], false);

    // Unregistering a function's regulation removes it from the exposure.
    ok(
        &h,
        "PUT",
        &mark(step),
        "admin",
        Some(json!({"regulated": false})),
    )
    .await;
    let (f, _) = functions(&h, "admin", provider, material).await;
    assert_eq!(f, ["card settlement", "fraud scoring"]);

    let exposure = format!("/v1/catalog/providers/{provider}/incidents/{material}/exposure");
    let (status, _, _) = h.call("GET", &exposure, Some("other-admin"), None).await;
    assert_eq!(status, 404, "another tenant never sees the incident");
    let (status, _, _) = h
        .call(
            "GET",
            &format!(
                "/v1/catalog/providers/{provider}/incidents/{}/exposure",
                Uuid::new_v4()
            ),
            Some("admin"),
            None,
        )
        .await;
    assert_eq!(status, 404);
}
