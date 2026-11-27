// Shared operational view (W5b, OBS-16-06, docs/assurance-relocations.md 4.7) through the
// real response routes behind `require_tenant`, as the scoped `observable_response` login.
// Stand-ins: auth-service (sessions and `lookup-principals`) and observable-alerting, which
// answers `GET /v1/incidents/{id}` only for sessions that may read the incident.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use admin_service::{
    middleware::auth::require_tenant,
    response::{self, ResponseState, net::TargetPolicy},
};
use axum::{
    Extension, Router,
    body::Body,
    http::{Request, StatusCode},
    middleware,
};
use http_body_util::BodyExt;
use observable_auth::authz::HttpAccessDecider;
use observable_auth::principals::HttpPrincipalDirectory;
use serde_json::{Value, json};
use test_support::postgres::{isolated_pool, scoped_pool};
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, Respond, ResponseTemplate,
    matchers::{body_partial_json, method, path},
};

const INTERNAL_TOKEN: &str = "shared-views-internal-token";

/// `lookup-principals` stand-in: active members, departed users, or unavailable.
#[derive(Clone, Default)]
struct Directory {
    active: Arc<Mutex<HashSet<Uuid>>>,
    departed: Arc<Mutex<HashSet<Uuid>>>,
    down: Arc<Mutex<bool>>,
}

impl Respond for Directory {
    fn respond(&self, req: &wiremock::Request) -> ResponseTemplate {
        if *self.down.lock().unwrap() {
            return ResponseTemplate::new(503);
        }
        let body: Value = serde_json::from_slice(&req.body).unwrap_or_default();
        let principals: Vec<Value> = body["user_ids"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|id| {
                let u: Uuid = id.as_str().unwrap().parse().unwrap();
                let state = if self.active.lock().unwrap().contains(&u) {
                    "active"
                } else if self.departed.lock().unwrap().contains(&u) {
                    "departed"
                } else {
                    "unknown"
                };
                json!({"user_id": u, "state": state})
            })
            .collect();
        ResponseTemplate::new(200).set_body_json(json!({ "principals": principals }))
    }
}

/// Alerting stand-in: the incident is readable only by sessions in `readers`.
#[derive(Clone)]
struct Incidents {
    incident: Uuid,
    readers: Arc<Mutex<HashSet<String>>>,
}

impl Respond for Incidents {
    fn respond(&self, req: &wiremock::Request) -> ResponseTemplate {
        let session = req
            .headers
            .get("cookie")
            .and_then(|v| v.to_str().ok())
            .and_then(|c| c.strip_prefix("session="))
            .unwrap_or_default()
            .to_owned();
        if req.url.path() == format!("/v1/incidents/{}", self.incident)
            && self.readers.lock().unwrap().contains(&session)
        {
            ResponseTemplate::new(200).set_body_json(json!({
                "incident_id": self.incident,
                "title": "Payments degraded after database failover",
                "severity": "critical",
                "status": "investigating",
                "affected_services": ["payments-api", "ledger-db"],
            }))
        } else {
            ResponseTemplate::new(404)
        }
    }
}

struct Fixture {
    app: Router,
    directory: Directory,
    incidents: Incidents,
    users: std::collections::HashMap<&'static str, Uuid>,
    _db: test_support::postgres::TestDatabase,
    _auth: MockServer,
    _alerting: MockServer,
}

async fn fixture() -> Fixture {
    let (owner, db) = isolated_pool().await;
    let scoped = scoped_pool(&owner, "observable_response").await;
    let auth = MockServer::start().await;
    let alerting = MockServer::start().await;
    let (tenant, other_tenant) = (Uuid::new_v4(), Uuid::new_v4());
    let mut users = std::collections::HashMap::new();
    for (token, t, role) in [
        ("payments", tenant, "member"),
        ("platform", tenant, "member"),
        ("bystander", tenant, "member"),
        ("other", other_tenant, "tenant_admin"),
    ] {
        let user = Uuid::new_v4();
        users.insert(token, user);
        Mock::given(method("POST"))
            .and(path("/internal/validate-session"))
            .and(body_partial_json(json!({ "session_token": token })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "user_id": user, "tenant_id": t, "role": role, "environment": "production"
            })))
            .mount(&auth)
            .await;
    }
    let directory = Directory::default();
    directory.active.lock().unwrap().extend([
        users["payments"],
        users["platform"],
        users["bystander"],
    ]);
    Mock::given(method("POST"))
        .and(path("/internal/principals/resolve"))
        .respond_with(directory.clone())
        .mount(&auth)
        .await;
    let incidents = Incidents {
        incident: Uuid::new_v4(),
        readers: Arc::new(Mutex::new(
            ["payments", "platform", "bystander"]
                .into_iter()
                .map(String::from)
                .collect(),
        )),
    };
    Mock::given(method("GET"))
        .respond_with(incidents.clone())
        .mount(&alerting)
        .await;
    let auth_url = auth.uri();
    let state = ResponseState {
        db: scoped,
        http: reqwest::Client::new(),
        alerting_url: alerting.uri(),
        decider: HttpAccessDecider::new(&auth_url, INTERNAL_TOKEN, reqwest::Client::new()),
        targets: TargetPolicy::default(),
        directory: HttpPrincipalDirectory::new(
            auth_url.as_str(),
            INTERNAL_TOKEN,
            reqwest::Client::new(),
        ),
    };
    let app = Router::new()
        .merge(response::router(state))
        .layer(middleware::from_fn(require_tenant))
        .layer(Extension(Arc::new(auth_url)))
        .layer(Extension(reqwest::Client::new()));
    Fixture {
        app,
        directory,
        incidents,
        users,
        _db: db,
        _auth: auth,
        _alerting: alerting,
    }
}

async fn call(
    f: &Fixture,
    method: &str,
    uri: &str,
    session: &str,
    body: Value,
) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("cookie", format!("session={session}"));
    let req = if body.is_null() {
        req.body(Body::empty()).unwrap()
    } else {
        req.header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let resp = f.app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn authorized_teams_coordinate_on_one_shared_timeline() {
    let f = fixture().await;
    let incident = f.incidents.incident;
    let (payments, platform) = (f.users["payments"], f.users["platform"]);

    // Participants must be active in the tenant; a departed principal is refused.
    let departed = Uuid::new_v4();
    f.directory.departed.lock().unwrap().insert(departed);
    let (status, body) = call(
        &f,
        "POST",
        "/v1/response/shared-views",
        "payments",
        json!({"incident_id": incident, "participants": [platform, departed]}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["principals"], json!([departed]));

    // The creator must read the incident with their own credential.
    let (status, _) = call(
        &f,
        "POST",
        "/v1/response/shared-views",
        "payments",
        json!({"incident_id": Uuid::new_v4(), "participants": [platform]}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    *f.directory.down.lock().unwrap() = true;
    let (status, _) = call(
        &f,
        "POST",
        "/v1/response/shared-views",
        "payments",
        json!({"incident_id": incident, "participants": [platform]}),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "fails closed");
    *f.directory.down.lock().unwrap() = false;

    let (status, body) = call(
        &f,
        "POST",
        "/v1/response/shared-views",
        "payments",
        json!({"incident_id": incident, "participants": [platform, platform]}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["participants"], json!([payments, platform]));
    let view = body["view_id"].as_str().unwrap().to_owned();
    let notes = format!("/v1/response/shared-views/{view}/notes");
    let read = format!("/v1/response/shared-views/{view}");

    // Two teams post coordination notes.
    for (who, note) in [
        (
            "payments",
            "Payments: card retries paused, error rate falling",
        ),
        (
            "platform",
            "Platform: ledger-db failover complete, replica lag 4s",
        ),
    ] {
        let (status, body) = call(&f, "POST", &notes, who, json!({"body": note})).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }
    let (status, _) = call(&f, "POST", &notes, "platform", json!({"body": "  "})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Every participant reads the same timeline.
    let (s1, a) = call(&f, "GET", &read, "payments", Value::Null).await;
    let (s2, b) = call(&f, "GET", &read, "platform", Value::Null).await;
    assert_eq!((s1, s2), (StatusCode::OK, StatusCode::OK), "{a} {b}");
    assert_eq!(a, b);
    let timeline: Vec<(Value, Value)> = a["timeline"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| (n["author_id"].clone(), n["body"].clone()))
        .collect();
    assert_eq!(
        timeline,
        vec![
            (
                json!(payments),
                json!("Payments: card retries paused, error rate falling")
            ),
            (
                json!(platform),
                json!("Platform: ledger-db failover complete, replica lag 4s")
            ),
        ]
    );
    assert_eq!(a["incident"]["incident_id"], json!(incident));
    assert_eq!(a["participants"], json!([payments, platform]));

    // Negative: a non-participant, even one who can read the incident, gets 404.
    for (m, u, b) in [
        ("GET", &read, Value::Null),
        ("POST", &notes, json!({"body": "hi"})),
    ] {
        let (status, _) = call(&f, m, u, "bystander", b).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{m} by a non-participant");
    }
    let (status, _) = call(&f, "GET", &read, "other", Value::Null).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "another tenant");

    // Negative: a participant who lost access to the incident gets 404 on every read.
    f.incidents.readers.lock().unwrap().remove("platform");
    let (status, _) = call(&f, "GET", &read, "platform", Value::Null).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &f,
        "POST",
        &notes,
        "platform",
        json!({"body": "still here"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, body) = call(&f, "GET", &read, "payments", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["timeline"].as_array().unwrap().len(), 2);
}
