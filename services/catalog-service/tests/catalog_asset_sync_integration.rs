//! Producer asset sync (phase 1 S6) in the standalone harness: alerting and control report the
//! full set of their assets of one kind per tenant, and the catalog registers what resolves and
//! retires the rest.

mod common;

use common::{Harness, INTERNAL_TOKEN};
use serde_json::{Value, json};
use uuid::Uuid;

const SYNC: &str = "/internal/catalog/assets/sync";

async fn svc(h: &Harness, name: &str) -> Uuid {
    h.service(json!({
        "business_name": name,
        "operational_name": name.to_lowercase().replace(' ', "-"),
        "criticality": "critical",
    }))
    .await
}

async fn bind(h: &Harness, id: Uuid, bindings: Value) {
    let (status, _, raw) = h
        .call(
            "PUT",
            &format!("/v1/catalog/services/{id}/bindings"),
            Some("admin"),
            Some(json!({ "bindings": bindings })),
        )
        .await;
    assert!((200..300).contains(&status), "{raw}");
}

async fn sync(h: &Harness, tenant: Uuid, component: &str, kind: &str, assets: Value) -> Value {
    let (status, json, raw) = h
        .call_with(
            "POST",
            SYNC,
            None,
            Some(INTERNAL_TOKEN),
            Some(json!({
                "tenant_id": tenant,
                "owning_component": component,
                "kind": kind,
                "assets": assets,
            })),
        )
        .await;
    assert_eq!(status, 200, "{raw}");
    json
}

/// (kind, external_id, owning_component, service_id) of live assets, sorted.
async fn assets(h: &Harness) -> Vec<(String, String, String, String)> {
    let (status, json, raw) = h
        .call("GET", "/v1/catalog/assets", Some("admin"), None)
        .await;
    assert_eq!(status, 200, "{raw}");
    let mut out: Vec<_> = json["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            (
                a["kind"].as_str().unwrap().to_owned(),
                a["external_id"].as_str().unwrap().to_owned(),
                a["owning_component"].as_str().unwrap().to_owned(),
                a["service_id"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    out.sort();
    out
}

#[tokio::test]
async fn sync_requires_the_internal_token_and_a_producer_component() {
    let h = Harness::start().await;
    let body = json!({
        "tenant_id": h.tenant, "owning_component": "observable-alerting",
        "kind": "alert_rule", "assets": [],
    });
    for (token, internal) in [(None, None), (None, Some("wrong")), (Some("admin"), None)] {
        let (status, _, _) = h
            .call_with("POST", SYNC, token, internal, Some(body.clone()))
            .await;
        assert_eq!(status, 401);
    }
    // The catalog's own component and unknown kinds are refused.
    for (component, kind) in [
        ("catalog", "alert_rule"),
        ("", "alert_rule"),
        ("x", "widget"),
    ] {
        let (status, _, _) = h
            .call_with(
                "POST",
                SYNC,
                None,
                Some(INTERNAL_TOKEN),
                Some(json!({
                    "tenant_id": h.tenant, "owning_component": component,
                    "kind": kind, "assets": [],
                })),
            )
            .await;
        assert_eq!(status, 400, "{component}/{kind}");
    }
}

#[tokio::test]
async fn sync_registers_resolved_assets_and_retires_what_is_no_longer_listed() {
    let h = Harness::start().await;
    let payments = svc(&h, "Card Payments").await;
    let ledger = svc(&h, "Ledger").await;
    bind(
        &h,
        payments,
        json!([{"service_name": "payments-api", "environment": "prod"}]),
    )
    .await;
    bind(
        &h,
        ledger,
        json!([{"service_name": "ledger", "environment": ""}]),
    )
    .await;

    let summary = sync(
        &h,
        h.tenant,
        "observable-alerting",
        "alert_rule",
        json!([
            {"external_id": "r1", "service_name": "payments-api", "environment": "prod", "name": "Errors"},
            {"external_id": "r2", "service_name": "ledger", "environment": "", "name": "Lag"},
            {"external_id": "r3", "service_name": "unbound", "environment": "", "name": "Orphan"},
            {"external_id": "r4", "service_name": "payments-api", "environment": "staging", "name": "Wrong env"},
        ]),
    )
    .await;
    assert_eq!(
        summary,
        json!({"registered": 2, "skipped": 2, "retired": 0})
    );
    assert_eq!(
        assets(&h).await,
        vec![
            (
                "alert_rule".into(),
                "r1".into(),
                "observable-alerting".into(),
                payments.to_string()
            ),
            (
                "alert_rule".into(),
                "r2".into(),
                "observable-alerting".into(),
                ledger.to_string()
            ),
        ]
    );

    // r1 deleted at the source: the next sync retires it; other kinds are untouched.
    sync(
        &h,
        h.tenant,
        "observable-alerting",
        "objective",
        json!([{"external_id": "s1", "service_name": "ledger", "environment": "prod", "name": "Ledger SLO"}]),
    )
    .await;
    let summary = sync(
        &h,
        h.tenant,
        "observable-alerting",
        "alert_rule",
        json!([{"external_id": "r2", "service_name": "ledger", "environment": "", "name": "Lag"}]),
    )
    .await;
    assert_eq!(
        summary,
        json!({"registered": 1, "skipped": 0, "retired": 1})
    );
    let live: Vec<String> = assets(&h).await.into_iter().map(|a| a.1).collect();
    assert_eq!(live, vec!["r2".to_owned(), "s1".to_owned()]);

    // A binding that goes away makes the asset unresolvable, and the sync retires it.
    bind(&h, ledger, json!([])).await;
    let summary = sync(
        &h,
        h.tenant,
        "observable-alerting",
        "alert_rule",
        json!([{"external_id": "r2", "service_name": "ledger", "environment": "", "name": "Lag"}]),
    )
    .await;
    assert_eq!(
        summary,
        json!({"registered": 0, "skipped": 1, "retired": 1})
    );
    // An asset listed again comes back.
    bind(
        &h,
        ledger,
        json!([{"service_name": "ledger", "environment": ""}]),
    )
    .await;
    sync(
        &h,
        h.tenant,
        "observable-alerting",
        "alert_rule",
        json!([{"external_id": "r2", "service_name": "ledger", "environment": "", "name": "Lag"}]),
    )
    .await;
    let live: Vec<String> = assets(&h).await.into_iter().map(|a| a.1).collect();
    assert_eq!(live, vec!["r2".to_owned(), "s1".to_owned()]);
}

#[tokio::test]
async fn an_identity_without_environment_resolves_only_to_a_single_service() {
    let h = Harness::start().await;
    let eu = svc(&h, "Payments EU").await;
    let us = svc(&h, "Payments US").await;
    bind(
        &h,
        eu,
        json!([{"service_name": "payments", "environment": "eu"}]),
    )
    .await;
    bind(
        &h,
        us,
        json!([{"service_name": "payments", "environment": "us"}]),
    )
    .await;
    let summary = sync(
        &h,
        h.tenant,
        "observable-control",
        "dashboard",
        json!([
            {"external_id": "d-any", "service_name": "payments", "environment": "", "name": "Ambiguous"},
            {"external_id": "d-eu", "service_name": "payments", "environment": "eu", "name": "EU"},
        ]),
    )
    .await;
    assert_eq!(
        summary,
        json!({"registered": 1, "skipped": 1, "retired": 0})
    );
    assert_eq!(assets(&h).await[0].3, eu.to_string());
}

#[tokio::test]
async fn sync_stays_within_its_tenant_component_and_kind() {
    let h = Harness::start().await;
    let payments = svc(&h, "Card Payments").await;
    bind(
        &h,
        payments,
        json!([{"service_name": "payments-api", "environment": ""}]),
    )
    .await;
    // A runbook registered by hand under the same key, and a control dashboard.
    let (status, _, raw) = h
        .call(
            "PUT",
            "/v1/catalog/assets/alert_rule/shared",
            Some("admin"),
            Some(json!({"service_id": payments, "name": "Hand-made"})),
        )
        .await;
    assert_eq!(status, 204, "{raw}");
    sync(
        &h,
        h.tenant,
        "observable-control",
        "dashboard",
        json!([{"external_id": "d1", "service_name": "payments-api", "environment": "", "name": "Board"}]),
    )
    .await;

    // Alerting cannot take over the hand-registered key, and its empty sync of another kind
    // retires nothing of control's.
    let summary = sync(
        &h,
        h.tenant,
        "observable-alerting",
        "alert_rule",
        json!([{"external_id": "shared", "service_name": "payments-api", "environment": "", "name": "Takeover"}]),
    )
    .await;
    assert_eq!(
        summary,
        json!({"registered": 0, "skipped": 1, "retired": 0})
    );
    sync(&h, h.tenant, "observable-alerting", "dashboard", json!([])).await;
    let owners: Vec<(String, String)> = assets(&h).await.into_iter().map(|a| (a.1, a.2)).collect();
    assert_eq!(
        owners,
        vec![
            ("shared".to_owned(), "catalog".to_owned()),
            ("d1".to_owned(), "observable-control".to_owned()),
        ]
    );

    // Another tenant's sync neither resolves this tenant's bindings nor retires its assets.
    let summary = sync(
        &h,
        h.other_tenant,
        "observable-control",
        "dashboard",
        json!([{"external_id": "d2", "service_name": "payments-api", "environment": "", "name": "Foreign"}]),
    )
    .await;
    assert_eq!(
        summary,
        json!({"registered": 0, "skipped": 1, "retired": 0})
    );
    assert_eq!(assets(&h).await.len(), 2);
}

// --- Control dashboards, pulled through manage-control-plane ---------------------------------

fn dashboard(id: &str, service: &str, owner: Option<Uuid>) -> Value {
    let mut d = json!({"kind": "dashboard", "external_id": id, "service_name": service,
                       "environment": "", "name": id});
    if let Some(o) = owner {
        d["owner"] = json!({"principal_kind": "user", "principal_id": o});
    }
    d
}

#[tokio::test]
async fn control_dashboards_are_pulled_and_retired_when_gone() {
    use catalog_service::coverage::pull_control_assets;
    use catalog_service::ports::ControlAssets;

    let h = Harness::start().await;
    let payments = svc(&h, "Card Payments").await;
    bind(
        &h,
        payments,
        json!([{"service_name": "payments-api", "environment": ""}]),
    )
    .await;
    let control = ControlAssets::substitute();
    control.set(
        h.tenant,
        vec![
            dashboard("d1", "payments-api", Some(h.member)),
            dashboard("d2", "unbound", None),
            json!({"kind": "alert_rule", "external_id": "x", "service_name": "payments-api",
                   "environment": "", "name": "not control's"}),
        ],
    );
    assert_eq!(
        pull_control_assets(&h.state, &control).await.unwrap(),
        (1, 0)
    );
    let (_, list, _) = h
        .call("GET", "/v1/catalog/assets", Some("admin"), None)
        .await;
    let items = list["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(
        (
            &items[0]["external_id"],
            &items[0]["owning_component"],
            &items[0]["owner_state"]
        ),
        (&json!("d1"), &json!("observable-control"), &json!("active"))
    );

    // Control unavailable: the registry is left as it is.
    let down = ControlAssets::substitute();
    down.set_unavailable();
    assert_eq!(pull_control_assets(&h.state, &down).await.unwrap(), (0, 1));
    assert_eq!(assets(&h).await.len(), 1);

    // The dashboard is deleted in control: the next pull retires it.
    control.set(h.tenant, vec![]);
    pull_control_assets(&h.state, &control).await.unwrap();
    assert!(assets(&h).await.is_empty());
}

#[tokio::test]
async fn objectives_bound_by_service_id_register_only_within_the_tenant() {
    let h = Harness::start().await;
    let payments = svc(&h, "Card Payments").await;
    let summary = sync(
        &h,
        h.tenant,
        "observable-alerting",
        "objective",
        json!([
            {"external_id": "o1", "service_id": payments, "environment": "", "name": "availability 0.995 production"},
            {"external_id": "o2", "service_id": Uuid::new_v4(), "environment": "", "name": "Unknown service"},
        ]),
    )
    .await;
    assert_eq!(
        summary,
        json!({"registered": 1, "skipped": 1, "retired": 0})
    );
    assert_eq!(
        assets(&h).await,
        [(
            "objective".to_owned(),
            "o1".to_owned(),
            "observable-alerting".to_owned(),
            payments.to_string()
        )]
    );
    // Another tenant naming this tenant's service id registers nothing.
    let other = sync(
        &h,
        Uuid::new_v4(),
        "observable-alerting",
        "objective",
        json!([{"external_id": "o3", "service_id": payments, "environment": "", "name": "x"}]),
    )
    .await;
    assert_eq!(other, json!({"registered": 0, "skipped": 1, "retired": 0}));
}
