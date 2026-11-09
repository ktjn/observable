//! Health worker inputs (phase 1 C2.2) in the standalone harness: the tenant-bound worker
//! credential, the `health_worker` grant and the binding and class scope it reads.

mod common;

use axum::{body::Body, http::Request};
use catalog_service::ports::EnvironmentClass;
use common::{Harness, INTERNAL_TOKEN};
use http_body_util::BodyExt as _;
use observable_auth::authz::{Classification, MockScenario, PrincipalKind};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

const WORKER: &str = "ohw_worker-of-tenant";
const OTHER_WORKER: &str = "ohw_worker-of-other-tenant";

async fn inputs_raw(
    h: &Harness,
    query: &str,
    internal: Option<&str>,
    worker: Option<&str>,
    bearer: Option<&str>,
) -> (u16, Value) {
    let mut req = Request::get(format!("/internal/catalog/health-inputs{query}"));
    if let Some(t) = internal {
        req = req.header("X-Internal-Token", t);
    }
    if let Some(t) = worker {
        req = req.header("X-Health-Worker-Token", t);
    }
    if let Some(t) = bearer {
        req = req.header("Authorization", format!("Bearer {t}"));
    }
    let resp = h
        .app
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn inputs(h: &Harness, tenant: Uuid, worker: &str) -> (u16, Value) {
    inputs_raw(
        h,
        &format!("?tenant_id={tenant}"),
        Some(INTERNAL_TOKEN),
        Some(worker),
        None,
    )
    .await
}

/// Service ids in the response, in order.
fn ids(body: &Value) -> Vec<String> {
    body["services"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_owned())
        .collect()
}

fn bindings_of(body: &Value, id: Uuid) -> Value {
    body["services"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == json!(id))
        .map(|s| s["bindings"].clone())
        .unwrap_or(Value::Null)
}

async fn service_as(h: &Harness, token: &str, name: &str, extra: Value) -> Uuid {
    let mut body = json!({
        "business_name": name,
        "operational_name": name.to_lowercase().replace(' ', "-"),
        "criticality": "critical",
    });
    for (k, v) in extra.as_object().unwrap() {
        body[k] = v.clone();
    }
    let (status, json, raw) = h
        .call("POST", "/v1/catalog/services", Some(token), Some(body))
        .await;
    assert_eq!(status, 201, "{raw}");
    json["id"].as_str().unwrap().parse().unwrap()
}

async fn bind_as(h: &Harness, token: &str, id: Uuid, bindings: Value) {
    let (status, _, raw) = h
        .call(
            "PUT",
            &format!("/v1/catalog/services/{id}/bindings"),
            Some(token),
            Some(json!({ "bindings": bindings })),
        )
        .await;
    assert_eq!(status, 200, "{raw}");
}

fn classes(pairs: &[(&str, &str)]) -> Vec<EnvironmentClass> {
    pairs
        .iter()
        .map(|(e, c)| EnvironmentClass {
            environment: (*e).into(),
            class: (*c).into(),
        })
        .collect()
}

/// (case, internal token, worker token, bearer).
type Attempt<'a> = (&'a str, Option<&'a str>, Option<&'a str>, Option<&'a str>);

async fn harness() -> Harness {
    let h = Harness::start().await;
    h.workers.issue(WORKER, h.tenant);
    h.workers.issue(OTHER_WORKER, h.other_tenant);
    h
}

#[tokio::test]
async fn worker_grant_matrix() {
    let h = harness().await;
    service_as(&h, "admin", "Card Payments", json!({})).await;
    let query = format!("?tenant_id={}", h.tenant);
    let refused: [Attempt; 9] = [
        ("no internal token", None, Some(WORKER), None),
        ("wrong internal token", Some("wrong"), Some(WORKER), None),
        ("cluster token alone", Some(INTERNAL_TOKEN), None, None),
        ("empty worker token", Some(INTERNAL_TOKEN), Some(""), None),
        (
            "tenant session instead of worker",
            Some(INTERNAL_TOKEN),
            None,
            Some("admin"),
        ),
        (
            "session as worker token",
            Some(INTERNAL_TOKEN),
            Some("admin"),
            None,
        ),
        (
            "worker of another tenant",
            Some(INTERNAL_TOKEN),
            Some(OTHER_WORKER),
            None,
        ),
        (
            "unknown worker token",
            Some(INTERNAL_TOKEN),
            Some("ohw_unknown"),
            None,
        ),
        (
            "worker without internal token",
            None,
            Some(WORKER),
            Some("admin"),
        ),
    ];
    for (case, internal, worker, bearer) in refused {
        let (status, body) = inputs_raw(&h, &query, internal, worker, bearer).await;
        assert_eq!(
            (status, &body),
            (401, &json!({"error": "unauthorized"})),
            "{case}"
        );
    }
    for q in ["", "?tenant_id=not-a-uuid"] {
        let (status, _) = inputs_raw(&h, q, Some(INTERNAL_TOKEN), Some(WORKER), None).await;
        assert_eq!(status, 401, "{q}");
    }

    let (status, body) = inputs(&h, h.tenant, WORKER).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["tenant_id"], json!(h.tenant));
    assert_eq!(body["services"].as_array().unwrap().len(), 1);

    // A revoked credential stops working at once.
    h.workers.revoke(WORKER);
    assert_eq!(inputs(&h, h.tenant, WORKER).await.0, 401);
    h.workers.issue(WORKER, h.tenant);

    // Policy outage and credential store outage fail closed with no service data.
    h.decider.set_scenario(MockScenario::Unavailable);
    assert_eq!(
        inputs(&h, h.tenant, WORKER).await,
        (503, json!({"error": "access_decision_unavailable"}))
    );
    h.decider.set_scenario(MockScenario::Policy);
    h.workers.set_unavailable();
    assert_eq!(
        inputs(&h, h.tenant, WORKER).await,
        (503, json!({"error": "credential_validation_unavailable"}))
    );
}

#[tokio::test]
async fn reads_are_authorized_through_decide_access_as_the_health_worker() {
    let h = harness().await;
    let open = service_as(&h, "admin", "Ledger", json!({"classification": "internal"})).await;
    let secret = service_as(
        &h,
        "admin",
        "Vault",
        json!({"classification": "restricted"}),
    )
    .await;
    let retired = service_as(&h, "admin", "Legacy", json!({})).await;
    let (status, _, raw) = h
        .call(
            "PATCH",
            &format!("/v1/catalog/services/{retired}"),
            Some("admin"),
            Some(json!({"lifecycle": "retired"})),
        )
        .await;
    assert_eq!(status, 200, "{raw}");

    // The worker grant reaches restricted records; retired services carry their lifecycle so
    // the evaluator can exclude them.
    let (_, body) = inputs(&h, h.tenant, WORKER).await;
    let mut seen = ids(&body);
    seen.sort();
    let mut want = vec![open.to_string(), secret.to_string(), retired.to_string()];
    want.sort();
    assert_eq!(seen, want);
    let legacy = body["services"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == json!(retired))
        .unwrap();
    assert_eq!(legacy["lifecycle"], json!("retired"));

    // A policy that withholds restricted records from the worker filters them out.
    h.decider
        .require_purpose("catalog.service", "read", Classification::Restricted);
    let (_, body) = inputs(&h, h.tenant, WORKER).await;
    assert!(!ids(&body).contains(&secret.to_string()));
    assert!(ids(&body).contains(&open.to_string()));
    assert!(
        h.decider
            .decision_log()
            .iter()
            .any(|r| r.principal_kind == Some(PrincipalKind::Service)
                && r.tenant_id == Some(h.tenant))
    );
}

#[tokio::test]
async fn two_tenants_with_identical_binding_names_stay_apart() {
    let h = harness().await;
    let ours = service_as(&h, "admin", "Payments", json!({})).await;
    let theirs = service_as(&h, "other-admin", "Payments", json!({})).await;
    let identity = json!([{"service_name": "payments", "environment": "prod"}]);
    bind_as(&h, "admin", ours, identity.clone()).await;
    bind_as(&h, "other-admin", theirs, identity).await;
    h.environments
        .set(h.tenant, classes(&[("prod", "production")]));
    h.environments
        .set(h.other_tenant, classes(&[("prod", "non_production")]));

    let (_, body) = inputs(&h, h.tenant, WORKER).await;
    assert_eq!(ids(&body), vec![ours.to_string()]);
    assert_eq!(
        bindings_of(&body, ours),
        json!([{"service_name": "payments", "environment": "prod", "environment_class": "production"}])
    );
    let (_, body) = inputs(&h, h.other_tenant, OTHER_WORKER).await;
    assert_eq!(ids(&body), vec![theirs.to_string()]);
    assert_eq!(
        bindings_of(&body, theirs)[0]["environment_class"],
        json!("non_production")
    );
}

#[tokio::test]
async fn same_name_in_production_and_non_production_keeps_its_class() {
    let h = harness().await;
    let live = service_as(&h, "admin", "Payments", json!({})).await;
    let test = service_as(&h, "admin", "Payments Staging", json!({})).await;
    bind_as(
        &h,
        "admin",
        live,
        json!([{"service_name": "payments", "environment": "prod"}]),
    )
    .await;
    bind_as(
        &h,
        "admin",
        test,
        json!([{"service_name": "payments", "environment": "staging"}]),
    )
    .await;
    h.environments.set(
        h.tenant,
        classes(&[("prod", "production"), ("staging", "non_production")]),
    );
    let (_, body) = inputs(&h, h.tenant, WORKER).await;
    assert_eq!(
        bindings_of(&body, live)[0]["environment_class"],
        json!("production")
    );
    assert_eq!(
        bindings_of(&body, test)[0]["environment_class"],
        json!("non_production")
    );
}

#[tokio::test]
async fn empty_or_unclassified_bindings_have_no_class() {
    let h = harness().await;
    let id = service_as(&h, "admin", "Payments", json!({})).await;
    bind_as(
        &h,
        "admin",
        id,
        json!([
            {"service_name": "payments", "environment": ""},
            {"service_name": "payments", "environment": "lab"},
            {"service_name": "payments", "environment": "unreported"},
        ]),
    )
    .await;
    h.environments
        .set(h.tenant, classes(&[("lab", "unclassified")]));
    let (_, body) = inputs(&h, h.tenant, WORKER).await;
    assert_eq!(
        bindings_of(&body, id),
        json!([
            {"service_name": "payments", "environment": "", "environment_class": null},
            {"service_name": "payments", "environment": "lab", "environment_class": null},
            {"service_name": "payments", "environment": "unreported", "environment_class": null},
        ])
    );
    // A service with no binding is listed with none.
    let bare = service_as(&h, "admin", "Bare", json!({})).await;
    let (_, body) = inputs(&h, h.tenant, WORKER).await;
    assert_eq!(bindings_of(&body, bare), json!([]));
}

#[tokio::test]
async fn changed_binding_or_class_shows_on_the_next_read() {
    let h = harness().await;
    let id = service_as(&h, "admin", "Payments", json!({})).await;
    bind_as(
        &h,
        "admin",
        id,
        json!([{"service_name": "payments", "environment": "prod"}]),
    )
    .await;
    h.environments
        .set(h.tenant, classes(&[("prod", "production")]));
    let (_, body) = inputs(&h, h.tenant, WORKER).await;
    assert_eq!(
        bindings_of(&body, id)[0]["environment_class"],
        json!("production")
    );

    h.environments
        .set(h.tenant, classes(&[("prod", "non_production")]));
    let (_, body) = inputs(&h, h.tenant, WORKER).await;
    assert_eq!(
        bindings_of(&body, id)[0]["environment_class"],
        json!("non_production")
    );

    bind_as(
        &h,
        "admin",
        id,
        json!([{"service_name": "payments-v2", "environment": "prod"}]),
    )
    .await;
    let (_, body) = inputs(&h, h.tenant, WORKER).await;
    assert_eq!(
        bindings_of(&body, id),
        json!([{"service_name": "payments-v2", "environment": "prod", "environment_class": "non_production"}])
    );
}

#[tokio::test]
async fn control_outage_returns_no_inputs() {
    let h = harness().await;
    let id = service_as(&h, "admin", "Payments", json!({})).await;
    bind_as(
        &h,
        "admin",
        id,
        json!([{"service_name": "payments", "environment": "prod"}]),
    )
    .await;
    h.environments.set_unavailable();
    assert_eq!(
        inputs(&h, h.tenant, WORKER).await,
        (503, json!({"error": "control_plane_unavailable"}))
    );
}

// --- The HTTP adapter of the worker credential port -------------------------------------------

#[tokio::test]
async fn http_worker_port_maps_auth_responses_and_fails_closed() {
    use axum::{Json, Router, http::HeaderMap, http::StatusCode, routing::post};
    use catalog_service::ports::{WorkerCredentials, WorkerRejected};
    use std::sync::Arc;

    let tenant = Uuid::new_v4();
    let app = Router::new().route(
        "/internal/health-worker/validate",
        post(
            move |headers: HeaderMap, Json(body): Json<Value>| async move {
                if headers
                    .get("x-internal-token")
                    .and_then(|v| v.to_str().ok())
                    != Some("svc")
                {
                    return StatusCode::UNAUTHORIZED;
                }
                match (body["tenant_id"].as_str(), body["token"].as_str()) {
                    (Some(t), Some("ohw_ok")) if t == tenant.to_string() => StatusCode::NO_CONTENT,
                    (_, Some("ohw_store_down")) => StatusCode::SERVICE_UNAVAILABLE,
                    _ => StatusCode::UNAUTHORIZED,
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let port = |token: &str, url: String| WorkerCredentials::Http {
        auth_url: Arc::new(url),
        internal_token: Arc::new(token.into()),
        client: reqwest::Client::new(),
    };
    let real = port("svc", format!("http://{addr}/"));
    assert_eq!(real.validate(tenant, "ohw_ok").await, Ok(()));
    assert_eq!(
        real.validate(Uuid::new_v4(), "ohw_ok").await,
        Err(WorkerRejected::Unauthorized)
    );
    assert_eq!(
        real.validate(tenant, "ohw_bad").await,
        Err(WorkerRejected::Unauthorized)
    );
    assert_eq!(
        real.validate(tenant, "ohw_store_down").await,
        Err(WorkerRejected::Unavailable)
    );
    // A misconfigured internal token never validates anything.
    assert_eq!(
        port("wrong", format!("http://{addr}"))
            .validate(tenant, "ohw_ok")
            .await,
        Err(WorkerRejected::Unauthorized)
    );
    assert_eq!(
        port("svc", "http://127.0.0.1:1".into())
            .validate(tenant, "ohw_ok")
            .await,
        Err(WorkerRejected::Unavailable)
    );
}
