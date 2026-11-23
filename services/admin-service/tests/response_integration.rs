// observable-response through its real routes: `require_tenant` with a stand-in auth-service
// (sessions, API keys, decide-access), observable-alerting stand-in for `GET /v1/incidents/{id}`,
// the `response` schema in real Postgres as the scoped `observable_response` login, and
// stand-in receivers for registered exchange targets.
//
// Evidence for OBS-16-01, 16-02, 16-04, 16-10 and OBS-30-01..04.

use std::sync::{Arc, Mutex};

use admin_service::{
    middleware::auth::require_tenant,
    response::{self, PURGE_STEPS, ResponseState, net::TargetPolicy},
};
use axum::{
    Extension, Router,
    body::Body,
    http::{Request, StatusCode},
    middleware,
};
use http_body_util::BodyExt;
use observable_auth::authz::HttpAccessDecider;
use serde_json::{Value, json};
use sqlx::PgPool;
use test_support::postgres::{isolated_pool, scoped_pool, tenant_row_counts, tenant_tables};
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, Respond, ResponseTemplate,
    matchers::{body_partial_json, header, header_exists, method, path},
};

const INTERNAL_TOKEN: &str = "response-test-internal-token";

/// Read grants `(principal, resource id)` evaluated by the decide-access stand-in.
#[derive(Clone, Default)]
struct Policy {
    tenant: Uuid,
    grants: Arc<Mutex<Vec<(Uuid, String)>>>,
}

impl Policy {
    fn grant(&self, principal: Uuid, resource: &str) {
        self.grants
            .lock()
            .unwrap()
            .push((principal, resource.to_owned()));
    }

    fn allows(&self, body: &Value, resource: &Value) -> bool {
        let tenant_ok = body["tenant_id"].as_str() == Some(self.tenant.to_string().as_str());
        let principal = body["principal"]["id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok());
        let id = resource["id"].as_str().unwrap_or_default();
        tenant_ok
            && principal.is_some_and(|p| {
                self.grants
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|(gp, gr)| *gp == p && gr == id)
            })
    }
}

impl Respond for Policy {
    fn respond(&self, req: &wiremock::Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&req.body).unwrap_or_default();
        if req.url.path().ends_with("/filter") {
            let permitted: Vec<Value> = body["resources"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|r| self.allows(&body, r))
                .map(|r| json!({ "resource_type": r["resource_type"], "id": r["id"] }))
                .collect();
            ResponseTemplate::new(200).set_body_json(json!({ "permitted": permitted }))
        } else if self.allows(&body, &body["resource"]) {
            ResponseTemplate::new(200)
        } else {
            ResponseTemplate::new(403)
        }
    }
}

struct Fixture {
    owner: PgPool,
    _db: test_support::postgres::TestDatabase,
    auth: MockServer,
    alerting: MockServer,
    tenant: Uuid,
    other_tenant: Uuid,
    admin: Uuid,
    second_admin: Uuid,
    member: Uuid,
    other_admin: Uuid,
    incident: Uuid,
    policy: Policy,
    app: Router,
}

async fn session(auth: &MockServer, token: &str, user: Uuid, tenant: Uuid, role: &str) {
    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .and(body_partial_json(json!({ "session_token": token })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user_id": user, "tenant_id": tenant, "role": role, "environment": "production"
        })))
        .mount(auth)
        .await;
}

fn incident_json(id: Uuid) -> Value {
    json!({
        "incident_id": id,
        "title": "Checkout latency",
        "severity": "critical",
        "status": "acknowledged",
        "dedup_key": "k",
        "triggered_at": "2026-11-23T10:00:00Z",
        "acknowledged_at": null, "acknowledged_by": null,
        "resolved_at": null, "resolved_by": null,
        "triggered_by_rule_id": null, "related_deployment_id": null,
        "runbook_url": "https://runbooks.example/checkout",
        "rule_name": null, "timeline": [],
        "impacted_service": "checkout",
        "owner": "team-checkout",
        "impact_summary": "Card payments slow in EU",
        "affected_services": ["checkout", "payments-api"]
    })
}

fn build_app(db: PgPool, auth_url: String, alerting_url: String, targets: TargetPolicy) -> Router {
    let state = ResponseState {
        db,
        http: reqwest::Client::new(),
        alerting_url,
        decider: HttpAccessDecider::new(&auth_url, INTERNAL_TOKEN, reqwest::Client::new()),
        targets,
    };
    Router::new()
        .merge(response::router(state))
        .layer(middleware::from_fn(require_tenant))
        .layer(Extension(Arc::new(auth_url)))
        .layer(Extension(reqwest::Client::new()))
}

fn permissive() -> TargetPolicy {
    TargetPolicy {
        allow_http: true,
        allow_private: true,
        ..Default::default()
    }
}

async fn fixture() -> Fixture {
    let (owner, db) = isolated_pool().await;
    let scoped = scoped_pool(&owner, "observable_response").await;
    let auth = MockServer::start().await;
    let alerting = MockServer::start().await;
    let (tenant, other_tenant) = (Uuid::new_v4(), Uuid::new_v4());
    let (admin, second_admin, member, other_admin) = (
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    );
    session(&auth, "admin", admin, tenant, "tenant_admin").await;
    session(&auth, "admin2", second_admin, tenant, "tenant_admin").await;
    session(&auth, "member", member, tenant, "member").await;
    session(&auth, "other", other_admin, other_tenant, "tenant_admin").await;
    Mock::given(method("POST"))
        .and(path("/internal/validate"))
        .and(body_partial_json(json!({ "api_key": "obs_key" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tenant_id": tenant, "role": "member", "environment": "production"
        })))
        .mount(&auth)
        .await;
    let policy = Policy {
        tenant,
        ..Default::default()
    };
    Mock::given(method("POST"))
        .and(path("/internal/authz/filter"))
        .and(header("x-internal-token", INTERNAL_TOKEN))
        .respond_with(policy.clone())
        .mount(&auth)
        .await;
    Mock::given(method("POST"))
        .and(path("/internal/authz/decide"))
        .and(header("x-internal-token", INTERNAL_TOKEN))
        .respond_with(policy.clone())
        .mount(&auth)
        .await;
    let incident = Uuid::new_v4();
    // Alerting answers only with the caller's own credential; every other incident is not found.
    Mock::given(method("GET"))
        .and(path(format!("/v1/incidents/{incident}")))
        .and(header_exists("cookie"))
        .respond_with(ResponseTemplate::new(200).set_body_json(incident_json(incident)))
        .mount(&alerting)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&alerting)
        .await;
    let app = build_app(scoped, auth.uri(), alerting.uri(), permissive());
    Fixture {
        owner,
        _db: db,
        auth,
        alerting,
        tenant,
        other_tenant,
        admin,
        second_admin,
        member,
        other_admin,
        incident,
        policy,
        app,
    }
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    session: &str,
    body: Value,
) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    req = if session.starts_with("obs_") {
        req.header("authorization", format!("Bearer {session}"))
    } else {
        req.header("cookie", format!("session={session}"))
    };
    let req = if body.is_null() {
        req.body(Body::empty()).unwrap()
    } else {
        req.header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn register(
    f: &Fixture,
    who: &str,
    kind: &str,
    name: &str,
    url: &str,
    consumer: Option<Uuid>,
) -> Uuid {
    let (status, body) = call(
        &f.app,
        "POST",
        "/v1/response/targets",
        who,
        json!({ "kind": kind, "name": name, "url": url, "consumer_principal_id": consumer }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    Uuid::parse_str(body["target_id"].as_str().unwrap()).unwrap()
}

async fn receiver(status: u16) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(status))
        .mount(&server)
        .await;
    server
}

async fn bodies(server: &MockServer) -> Vec<Value> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

// ── OBS-16-01 ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn incident_handoff_preserves_service_severity_status_and_context() {
    let f = fixture().await;
    let target = receiver(200).await;
    let target_id = register(
        &f,
        "admin",
        "incident_handoff",
        "itsm",
        &format!("{}/hook", target.uri()),
        None,
    )
    .await;

    let (status, body) = call(
        &f.app,
        "POST",
        "/v1/response/handoffs",
        "member",
        json!({
            "incident_id": f.incident,
            "target_id": target_id,
            "context": { "notes": "rolled back v42" },
            // Ignored: the tenant always comes from the caller.
            "tenant_id": f.other_tenant,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "delivered");
    assert_eq!(body["attempts"], 1);

    let sent = bodies(&target).await;
    assert_eq!(sent.len(), 1);
    let p = &sent[0];
    assert_eq!(p["tenant_id"], f.tenant.to_string());
    assert_eq!(p["incident_id"], f.incident.to_string());
    assert_eq!(p["service_name"], "checkout");
    assert_eq!(p["affected_services"], json!(["checkout", "payments-api"]));
    assert_eq!(p["severity"], "critical");
    assert_eq!(p["status"], "acknowledged");
    assert_eq!(p["owner"], "team-checkout");
    assert_eq!(p["context"]["notes"], "rolled back v42");

    let (row_status, row_tenant): (String, Uuid) = sqlx::query_as(
        "SELECT status, tenant_id FROM response.handoff_outbox WHERE incident_id = $1",
    )
    .bind(f.incident)
    .fetch_one(&f.owner)
    .await
    .unwrap();
    assert_eq!((row_status.as_str(), row_tenant), ("delivered", f.tenant));
}

#[tokio::test]
async fn handoff_failure_is_detected_retried_and_reported_without_silent_loss() {
    let f = fixture().await;
    let target = receiver(503).await;
    let target_id = register(&f, "admin", "incident_handoff", "itsm", &target.uri(), None).await;

    let (status, body) = call(
        &f.app,
        "POST",
        "/v1/response/handoffs",
        "member",
        json!({ "incident_id": f.incident, "target_id": target_id, "max_retries": 2, "context": { "k": "v" } }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert_eq!(body["status"], "failed_pending_retry");
    assert_eq!(body["attempts"], 3);
    assert_eq!(body["last_error"], "target returned HTTP 503");
    assert_eq!(
        bodies(&target).await.len(),
        3,
        "each attempt reached the target"
    );

    let (row_status, attempts, payload): (String, i32, Value) = sqlx::query_as(
        "SELECT status, attempts, payload FROM response.handoff_outbox WHERE incident_id = $1",
    )
    .bind(f.incident)
    .fetch_one(&f.owner)
    .await
    .unwrap();
    assert_eq!(row_status, "failed_pending_retry");
    assert_eq!(attempts, 3);
    assert_eq!(payload["severity"], "critical");
    assert_eq!(
        payload["context"]["k"], "v",
        "the context is kept for the retry"
    );
}

#[tokio::test]
async fn handoff_goes_only_to_registered_active_targets_of_the_callers_tenant() {
    let f = fixture().await;
    let target = receiver(200).await;

    // Only a tenant admin approves a destination.
    let (status, _) = call(
        &f.app,
        "POST",
        "/v1/response/targets",
        "member",
        json!({ "kind": "incident_handoff", "name": "x", "url": target.uri() }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let foreign = register(
        &f,
        "other",
        "incident_handoff",
        "theirs",
        &target.uri(),
        None,
    )
    .await;
    let ours = register(&f, "admin", "incident_handoff", "ours", &target.uri(), None).await;
    let notify = register(&f, "admin", "notification", "chat", &target.uri(), None).await;

    for (target_id, why) in [(foreign, "another tenant's target"), (notify, "wrong kind")] {
        let (status, body) = call(
            &f.app,
            "POST",
            "/v1/response/handoffs",
            "admin",
            json!({ "incident_id": f.incident, "target_id": target_id }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{why}: {body}");
    }

    let (status, _) = call(
        &f.app,
        "POST",
        &format!("/v1/response/targets/{ours}/deactivate"),
        "admin",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(
        &f.app,
        "POST",
        "/v1/response/handoffs",
        "admin",
        json!({ "incident_id": f.incident, "target_id": ours }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a deactivated target is not used"
    );

    // An incident the caller cannot read is not handed off.
    let active = register(
        &f,
        "admin",
        "incident_handoff",
        "ours-2",
        &target.uri(),
        None,
    )
    .await;
    let (status, _) = call(
        &f.app,
        "POST",
        "/v1/response/handoffs",
        "admin",
        json!({ "incident_id": Uuid::new_v4(), "target_id": active }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        bodies(&target).await.is_empty(),
        "nothing left the platform"
    );

    let (_, listed) = call(&f.app, "GET", "/v1/response/targets", "member", Value::Null).await;
    let names: Vec<&str> = listed["targets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["chat", "ours", "ours-2"],
        "another tenant's targets are not listed"
    );
}

#[tokio::test]
async fn target_registration_refuses_plain_http_private_addresses_and_credentials() {
    let f = fixture().await;
    let scoped = scoped_pool(&f.owner, "observable_response").await;
    let strict = build_app(
        scoped,
        f.auth.uri(),
        f.alerting.uri(),
        TargetPolicy::default(),
    );
    for (url, code) in [
        ("http://hooks.example.com/x", "target_scheme_not_allowed"),
        ("https://127.0.0.1/x", "target_address_not_allowed"),
        (
            "https://169.254.169.254/latest/meta-data",
            "target_address_not_allowed",
        ),
        ("https://[::1]/x", "target_address_not_allowed"),
        (
            "https://user:secret@hooks.example.com/x",
            "target_url_has_credentials",
        ),
        ("gopher://hooks.example.com/x", "target_scheme_not_allowed"),
    ] {
        let (status, body) = call(
            &strict,
            "POST",
            "/v1/response/targets",
            "admin",
            json!({ "kind": "notification", "name": url, "url": url }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{url}");
        assert_eq!(body["error"], code, "{url}");
    }
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM response.exchange_targets")
        .fetch_one(&f.owner)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
async fn send_rechecks_a_registered_target_against_the_policy() {
    let f = fixture().await;
    let target = receiver(200).await;
    let target_id = register(&f, "admin", "incident_handoff", "itsm", &target.uri(), None).await;
    // The same database served by a deployment with the strict policy: the stored loopback
    // URL is refused at send time and the refusal is recorded.
    let scoped = scoped_pool(&f.owner, "observable_response").await;
    let strict = build_app(
        scoped,
        f.auth.uri(),
        f.alerting.uri(),
        TargetPolicy::default(),
    );
    let (status, body) = call(
        &strict,
        "POST",
        "/v1/response/handoffs",
        "admin",
        json!({ "incident_id": f.incident, "target_id": target_id, "max_retries": 0 }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert_eq!(body["last_error"], "target_scheme_not_allowed");
    assert!(bodies(&target).await.is_empty());
}

// ── OBS-16-02 ────────────────────────────────────────────────────────────────

fn records() -> Value {
    json!([
        { "record_id": "slo-1", "record_type": "slo_report", "classification": "internal", "payload": { "target": 99.9 } },
        { "record_id": "inc-1", "record_type": "incident_report", "classification": "confidential", "payload": { "mttr_min": 42 } },
        { "record_id": "pii-1", "record_type": "customer_log", "classification": "restricted", "payload": { "email": "x" } }
    ])
}

#[tokio::test]
async fn exit_exercise_exports_agreed_business_records_for_independent_review() {
    let f = fixture().await;
    for r in ["slo-1", "inc-1", "pii-1"] {
        f.policy.grant(f.admin, r);
    }
    let (status, body) = call(
        &f.app,
        "POST",
        "/v1/response/exports",
        "admin",
        json!({ "purpose": "exit exercise 2026", "records": records(), "tenant_id": f.other_tenant, "user_id": f.other_admin }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["tenant_id"], f.tenant.to_string());
    assert_eq!(body["exported_by"], f.admin.to_string());
    assert_eq!(body["record_count"], 3);
    assert_eq!(body["withheld_count"], 0);
    assert_eq!(body["records"][0]["payload"]["target"], 99.9);
}

#[tokio::test]
async fn restricted_scope_export_withholds_unauthorized_records() {
    let f = fixture().await;
    f.policy.grant(f.member, "slo-1");
    f.policy.grant(f.member, "inc-1");
    // A grant to another user does not widen the caller's export.
    f.policy.grant(f.admin, "pii-1");
    let (status, body) = call(
        &f.app,
        "POST",
        "/v1/response/exports",
        "member",
        json!({ "purpose": "exit exercise 2026", "records": records(), "user_id": f.admin }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["record_count"], 2);
    assert_eq!(body["withheld_count"], 1);
    let ids: Vec<&str> = body["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["record_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["slo-1", "inc-1"]);
    assert!(
        !body.to_string().contains("email"),
        "withheld content is not returned"
    );

    // An API key has no person to decide on.
    let (status, body) = call(
        &f.app,
        "POST",
        "/v1/response/exports",
        "obs_key",
        json!({ "purpose": "p", "records": records() }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "user_session_required");
}

// ── OBS-16-04 ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn notification_reaches_only_approved_enterprise_channels() {
    let f = fixture().await;
    let chat = receiver(200).await;
    let pager = receiver(200).await;
    let itsm = receiver(200).await;
    let chat_id = register(&f, "admin", "notification", "ops-chat", &chat.uri(), None).await;
    let pager_id = register(&f, "admin", "notification", "pager", &pager.uri(), None).await;
    let itsm_id = register(&f, "admin", "incident_handoff", "itsm", &itsm.uri(), None).await;
    let unknown = Uuid::new_v4();

    let (status, body) = call(
        &f.app,
        "POST",
        "/v1/response/notifications",
        "member",
        json!({
            "incident_id": f.incident,
            "target_ids": [chat_id, pager_id, itsm_id, unknown],
            "message": "Checkout degraded, mitigation in progress"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let delivered: Vec<&str> = body["deliveries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["status"].as_str().unwrap())
        .collect();
    assert_eq!(delivered, ["delivered", "delivered"]);
    let mut rejected: Vec<String> = body["rejected_target_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    rejected.sort();
    let mut expected = vec![itsm_id.to_string(), unknown.to_string()];
    expected.sort();
    assert_eq!(rejected, expected);

    for server in [&chat, &pager] {
        let sent = bodies(server).await;
        assert_eq!(sent.len(), 1);
        assert_eq!(
            sent[0]["message"],
            "Checkout degraded, mitigation in progress"
        );
        assert_eq!(sent[0]["incident"]["severity"], "critical");
    }
    assert!(
        bodies(&itsm).await.is_empty(),
        "a non-notification target gets nothing"
    );

    let recorded: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM response.notification_deliveries WHERE tenant_id = $1 AND status = 'delivered'",
    )
    .bind(f.tenant)
    .fetch_one(&f.owner)
    .await
    .unwrap();
    assert_eq!(recorded, 2);
}

// ── OBS-16-10 ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn investigation_handoff_carries_only_evidence_the_consumer_may_read() {
    let f = fixture().await;
    let consumer = Uuid::new_v4();
    let target = receiver(200).await;
    let target_id = register(
        &f,
        "admin",
        "external_consumer",
        "vendor-soc",
        &target.uri(),
        Some(consumer),
    )
    .await;
    for r in ["metric:p95", "log:pii", "deploy:rel-4"] {
        f.policy.grant(f.admin, r);
    }
    f.policy.grant(consumer, "metric:p95");
    f.policy.grant(consumer, "deploy:rel-4");
    // The consumer may read this, the caller may not: it is not handed over either.
    f.policy.grant(consumer, "trace:abc");

    let (status, body) = call(
        &f.app,
        "POST",
        "/v1/response/investigation-handoffs",
        "admin",
        json!({
            "incident_id": f.incident,
            "target_id": target_id,
            "scope": "EU checkout latency since 10:00 UTC",
            "purpose": "joint investigation",
            "evidence": [
                { "resource_type": "metric", "id": "metric:p95", "classification": "internal" },
                { "resource_type": "log", "id": "log:pii", "classification": "restricted" },
                { "resource_type": "deploy", "id": "deploy:rel-4", "classification": "internal" },
                { "resource_type": "trace", "id": "trace:abc", "classification": "internal" }
            ]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let sent = bodies(&target).await;
    assert_eq!(sent.len(), 1);
    let ids: Vec<&str> = sent[0]["evidence_refs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["metric:p95", "deploy:rel-4"]);
    assert_eq!(sent[0]["withheld_evidence_count"], 2);
    assert_eq!(sent[0]["scope"], "EU checkout latency since 10:00 UTC");
    assert_eq!(sent[0]["incident"]["service_name"], "checkout");
    assert_eq!(sent[0]["incident"]["tenant_id"], f.tenant.to_string());
}

// ── OBS-30-01..04 ────────────────────────────────────────────────────────────

fn comms(f: &Fixture) -> String {
    format!("/v1/response/incidents/{}/communications", f.incident)
}

async fn draft(f: &Fixture, audience: Uuid, evidence: Value) -> Uuid {
    let (status, body) = call(
        &f.app,
        "POST",
        &comms(f),
        "admin",
        json!({
            "audience_target_id": audience,
            "suspected_scope": ["eu-west edge"],
            "confirmed_services": ["checkout"],
            "summary": "Card payments in the EU are slow. We are mitigating.",
            "evidence_refs": evidence
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["state"], "draft");
    Uuid::parse_str(body["communication_id"].as_str().unwrap()).unwrap()
}

#[tokio::test]
async fn approved_update_reaches_the_defined_audience_with_labeled_scope_and_review_trail() {
    let f = fixture().await;
    let page = receiver(200).await;
    let audience = register(
        &f,
        "admin",
        "status_page",
        "customers-eu",
        &page.uri(),
        None,
    )
    .await;
    let id = draft(
        &f,
        audience,
        json!([{ "resource_type": "metric", "id": "m1", "classification": "public" }]),
    )
    .await;

    let (status, body) = call(
        &f.app,
        "POST",
        &format!("{}/{id}/approve", comms(&f)),
        "admin2",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["approved_by"], f.second_admin.to_string());

    let (status, body) = call(
        &f.app,
        "POST",
        &format!("{}/{id}/release", comms(&f)),
        "admin2",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["communication"]["state"], "released");

    let sent = bodies(&page).await;
    assert_eq!(sent.len(), 1, "the update reached the audience once");
    assert_eq!(sent[0]["suspected_scope"], json!(["eu-west edge"]));
    assert_eq!(sent[0]["confirmed_affected_services"], json!(["checkout"]));
    assert_eq!(
        sent[0]["summary"],
        "Card payments in the EU are slow. We are mitigating."
    );
    assert!(
        sent[0].get("evidence_refs").is_none(),
        "no evidence leaves with the update"
    );

    let (status, trail) = call(&f.app, "GET", &comms(&f), "member", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    let c = &trail["communications"][0];
    assert_eq!(c["drafted_by"], f.admin.to_string());
    assert_eq!(c["approved_by"], f.second_admin.to_string());
    assert_eq!(c["released_by"], f.second_admin.to_string());
    assert!(c["approved_at"].is_string() && c["released_at"].is_string());
    assert_eq!(c["delivery_status"], "delivered");
    assert_eq!(c["audience_target_id"], audience.to_string());

    // Released once; a second release is refused and nothing is resent.
    let (status, _) = call(
        &f.app,
        "POST",
        &format!("{}/{id}/release", comms(&f)),
        "admin2",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(bodies(&page).await.len(), 1);
}

#[tokio::test]
async fn unapproved_or_self_approved_updates_are_not_released() {
    let f = fixture().await;
    let page = receiver(200).await;
    let audience = register(&f, "admin", "status_page", "customers", &page.uri(), None).await;
    let id = draft(&f, audience, json!([])).await;

    let (status, _) = call(
        &f.app,
        "POST",
        &format!("{}/{id}/release", comms(&f)),
        "admin",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "a draft is never released");
    let (status, body) = call(
        &f.app,
        "POST",
        &format!("{}/{id}/approve", comms(&f)),
        "admin",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "approver_must_differ_from_drafter");
    let (status, _) = call(
        &f.app,
        "POST",
        &format!("{}/{id}/approve", comms(&f)),
        "member",
        Value::Null,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "approval needs a tenant admin"
    );
    let (status, _) = call(
        &f.app,
        "POST",
        &format!("{}/{id}/approve", comms(&f)),
        "other",
        Value::Null,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "another tenant cannot see the draft"
    );
    assert!(bodies(&page).await.is_empty());

    // An audience must be a registered status page of the tenant.
    let chat = register(&f, "admin", "notification", "chat", &page.uri(), None).await;
    let (status, _) = call(
        &f.app,
        "POST",
        &comms(&f),
        "admin",
        json!({ "audience_target_id": chat, "summary": "s" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn confirmed_services_must_be_named_by_the_incident() {
    let f = fixture().await;
    let page = receiver(200).await;
    let audience = register(&f, "admin", "status_page", "customers", &page.uri(), None).await;
    let (status, body) = call(
        &f.app,
        "POST",
        &comms(&f),
        "admin",
        json!({
            "audience_target_id": audience,
            "suspected_scope": ["search"],
            "confirmed_services": ["search"],
            "summary": "Search is down"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "service_not_confirmed_by_incident");
}

#[tokio::test]
async fn customer_facing_update_never_carries_internal_or_restricted_evidence() {
    let f = fixture().await;
    let page = receiver(200).await;
    let audience = register(&f, "admin", "status_page", "customers", &page.uri(), None).await;
    for evidence in [
        json!([{ "resource_type": "log", "id": "l1", "classification": "restricted" }]),
        json!([{ "resource_type": "trace", "id": "t1", "classification": "internal" }]),
        json!([{ "resource_type": "trace", "id": "t2" }]),
    ] {
        let id = draft(&f, audience, evidence.clone()).await;
        let (status, body) = call(
            &f.app,
            "POST",
            &format!("{}/{id}/approve", comms(&f)),
            "admin2",
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{evidence}");
        assert_eq!(body["error"], "non_public_evidence");
    }
    assert!(bodies(&page).await.is_empty());
}

// ── Tenant purge ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn every_response_table_is_purged_and_only_for_the_tenant() {
    let (owner, _db) = isolated_pool().await;
    let mut covered: Vec<String> = PURGE_STEPS.iter().map(|(t, _)| t.to_string()).collect();
    covered.sort();
    assert_eq!(tenant_tables(&owner, "response").await, covered);

    let (gone, kept) = (Uuid::new_v4(), Uuid::new_v4());
    for t in [gone, kept] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "WITH tg AS (INSERT INTO response.exchange_targets (tenant_id, kind, name, url, registered_by) \
               VALUES ('{t}', 'status_page', 'p', 'https://status.example', gen_random_uuid()) RETURNING target_id), \
             h AS (INSERT INTO response.handoff_outbox (tenant_id, kind, incident_id, target_id, status, attempts, payload) \
               SELECT '{t}', 'incident', gen_random_uuid(), target_id, 'delivered', 1, '{{}}' FROM tg), \
             n AS (INSERT INTO response.notification_deliveries (tenant_id, target_id, status) \
               SELECT '{t}', target_id, 'delivered' FROM tg) \
             INSERT INTO response.incident_communications (tenant_id, incident_id, audience_target_id, summary, drafted_by) \
               SELECT '{t}', gen_random_uuid(), target_id, 's', gen_random_uuid() FROM tg"
        )))
        .execute(&owner)
        .await
        .unwrap();
    }
    let kept_before = tenant_row_counts(&owner, "response", kept).await;
    let scoped = scoped_pool(&owner, "observable_response").await;
    let deleted = response::purge_tenant(&scoped, gone).await.unwrap();
    assert!(deleted.iter().all(|(_, n)| *n == 1), "{deleted:?}");
    assert!(
        tenant_row_counts(&owner, "response", gone)
            .await
            .iter()
            .all(|(_, n)| *n == 0)
    );
    assert_eq!(
        tenant_row_counts(&owner, "response", kept).await,
        kept_before
    );
    let again = response::purge_tenant(&scoped, gone).await.unwrap();
    assert!(again.iter().all(|(_, n)| *n == 0));
}

#[tokio::test]
async fn response_login_reaches_only_the_response_schema() {
    let (owner, _db) = isolated_pool().await;
    let scoped = scoped_pool(&owner, "observable_response").await;
    for sql in [
        "SELECT count(*) FROM control.tenants",
        "SELECT count(*) FROM alerting.service_objectives",
        "SELECT count(*) FROM auth.access_decision_log",
    ] {
        let err = sqlx::query(sql).execute(&scoped).await.unwrap_err();
        assert!(
            err.to_string().contains("permission denied"),
            "observable_response must not read {sql}: {err}"
        );
    }
}
