//! Service review composition (Phase 1 C4) against substitute catalog, health and alerting
//! providers served over HTTP. The providers' own behavior is covered by their contract suites;
//! these tests pin how query combines and isolates their answers.

use axum::{
    Extension, Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use http_body_util::BodyExt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use observable_storage_contracts::watermark::IngestWatermark;
use query_api::confidence::{PlatformSignals, WatermarkSource, get_platform_confidence};
use query_api::middleware::auth::TenantContext;
use query_api::review::{
    ReviewProviders, get_service_review, list_service_overview, list_service_reviews,
};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::matchers::{header, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// An objective's class and, when approved, its compliance status and evidence status.
type Entry<'a> = (&'a str, Option<(&'a str, &'a str)>);

const COOKIE: &str = "session=caller-session";
const INTERNAL: &str = "internal-secret";

/// The tenant watermark the ingest function reads; `Err` is an unreadable store.
struct Watermark(Mutex<Result<IngestWatermark, ()>>);

#[async_trait::async_trait]
impl WatermarkSource for Watermark {
    async fn read(&self, _: Uuid, _: u64, _: u64) -> Result<IngestWatermark, ()> {
        *self.0.lock().unwrap()
    }
}

fn now_ns() -> u64 {
    chrono::Utc::now().timestamp_nanos_opt().unwrap() as u64
}

fn seen(seconds_ago: u64) -> Result<IngestWatermark, ()> {
    Ok(IngestWatermark {
        traces: Some(now_ns() - seconds_ago * 1_000_000_000),
        logs: None,
        metrics: None,
    })
}

struct World {
    catalog: MockServer,
    health: MockServer,
    alerting: MockServer,
    /// Serves both providers' internal self-status routes.
    internal: MockServer,
    watermark: Arc<Watermark>,
    tenant: Uuid,
}

impl World {
    async fn new() -> Self {
        let w = Self {
            catalog: MockServer::start().await,
            health: MockServer::start().await,
            alerting: MockServer::start().await,
            internal: MockServer::start().await,
            watermark: Arc::new(Watermark(Mutex::new(seen(5)))),
            tenant: Uuid::new_v4(),
        };
        w.self_statuses("current", "current").await;
        w
    }

    fn router(&self) -> Router {
        let providers = ReviewProviders::http(
            self.catalog.uri(),
            self.health.uri(),
            self.alerting.uri(),
            reqwest::Client::new(),
        );
        let signals = PlatformSignals::new(
            Some(self.internal.uri()),
            self.internal.uri(),
            INTERNAL,
            reqwest::Client::new(),
            self.watermark.clone(),
            Duration::from_secs(300),
        );
        Router::new()
            .route("/v1/review/services", get(list_service_reviews))
            .route("/v1/review/services/{id}", get(get_service_review))
            .route("/v1/overview/services", get(list_service_overview))
            .route("/v1/platform/confidence", get(get_platform_confidence))
            .layer(Extension(providers))
            .layer(Extension(signals))
            .layer(Extension(TenantContext {
                tenant_id: self.tenant,
                user_id: None,
                role: "member".into(),
            }))
    }

    /// Both providers' self-status, served only with the internal token. `None` is an
    /// outage of that provider.
    async fn self_statuses(
        &self,
        health: impl Into<Option<&str>>,
        alerting: impl Into<Option<&str>>,
    ) {
        self.internal.reset().await;
        for (provider, state) in [("health", health.into()), ("alerting", alerting.into())] {
            let Some(state) = state else { continue };
            Mock::given(method("GET"))
                .and(path(format!("/internal/{provider}/self-status")))
                .and(header("x-internal-token", INTERNAL))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "state": state,
                    "last_pass": { "completed_at": "2026-11-10T12:00:00Z" },
                })))
                .mount(&self.internal)
                .await;
        }
    }

    async fn issues(&self, id: Uuid, class: &str, open: u64) {
        self.caller(
            Mock::given(method("GET"))
                .and(path(format!("/v1/health/services/{id}/issues")))
                .and(query_param("environment_class", class)),
        )
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "service_id": id,
            "environment_class": class,
            "evidence_status": "current",
            "open": open,
            "issues": [],
        })))
        .mount(&self.health)
        .await;
    }

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let resp = self
            .router()
            .oneshot(
                Request::get(uri)
                    .header("cookie", COOKIE)
                    .header("x-tenant-id", self.tenant.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// Every provider read must carry the caller's own credential and tenant.
    fn caller(&self, m: wiremock::MockBuilder) -> wiremock::MockBuilder {
        m.and(header("cookie", COOKIE))
            .and(header("x-tenant-id", self.tenant.to_string().as_str()))
    }

    async fn service(&self, id: Uuid, lifecycle: &str) {
        self.caller(Mock::given(method("GET")).and(path(format!("/v1/catalog/services/{id}"))))
            .respond_with(ResponseTemplate::new(200).set_body_json(record(id, lifecycle)))
            .mount(&self.catalog)
            .await;
    }

    async fn health_state(&self, id: Uuid, class: &str, state: &str, evidence: &str) {
        self.caller(
            Mock::given(method("GET"))
                .and(path(format!("/v1/health/services/{id}")))
                .and(query_param("environment_class", class)),
        )
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "service_id": id,
            "environment_class": class,
            "state": state,
            "evaluated_at": "2026-11-09T12:00:00Z",
            "last_evidence_at": "2026-11-09T11:59:30Z",
            "evidence_status": evidence,
            "freshness_seconds": 300,
            "conditions": [],
        })))
        .mount(&self.health)
        .await;
    }

    /// One approved objective per (class, compliance status) pair, plus its compliance read.
    async fn objectives(&self, id: Uuid, entries: &[Entry<'_>]) {
        let mut items = Vec::new();
        for (class, compliance) in entries {
            let oid = Uuid::new_v4();
            items.push(json!({
                "objective_id": oid,
                "service_id": id,
                "environment_class": class,
                "sli_type": "availability",
                "target": 0.99,
                "window_days": 30,
                "description": "checkout availability",
                "approved_at": compliance.map(|_| "2026-11-09T10:00:00Z"),
            }));
            if let Some((status, evidence)) = compliance {
                self.caller(
                    Mock::given(method("GET"))
                        .and(path(format!("/v1/objectives/{oid}/compliance"))),
                )
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "objective_id": oid,
                    "status": status,
                    "evidence_status": evidence,
                    "attainment": if *evidence == "current" { json!(0.995) } else { Value::Null },
                    "error_budget_consumed": if *evidence == "current" { json!(0.5) } else { Value::Null },
                    "last_evidence_at": "2026-11-09T11:59:00Z",
                })))
                .mount(&self.alerting)
                .await;
            }
        }
        self.caller(
            Mock::given(method("GET"))
                .and(path("/v1/objectives"))
                .and(query_param("service_id", id.to_string().as_str())),
        )
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": items })))
        .mount(&self.alerting)
        .await;
    }
}

fn record(id: Uuid, lifecycle: &str) -> Value {
    json!({
        "id": id,
        "business_name": format!("Checkout {lifecycle}"),
        "operational_name": "checkout",
        "domain": "payments",
        "criticality": "critical",
        "lifecycle": lifecycle,
    })
}

#[tokio::test]
async fn review_non_production_failure_leaves_production_review_unchanged() {
    let w = World::new().await;
    let id = Uuid::new_v4();
    w.service(id, "live").await;
    w.health_state(id, "production", "normal", "current").await;
    w.health_state(id, "non_production", "normal", "current")
        .await;
    // The failure is injected only into non-production evidence.
    w.objectives(
        id,
        &[
            ("production", Some(("compliant", "current"))),
            ("non_production", Some(("breached", "current"))),
        ],
    )
    .await;

    let (s, prod) = w
        .get(&format!(
            "/v1/review/services/{id}?environment_class=production"
        ))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(prod["live"], true);
    assert_eq!(prod["health"]["state"], "normal");
    assert_eq!(prod["expectation"]["status"], "compliant");
    assert_eq!(
        prod["expectation"]["objectives"].as_array().unwrap().len(),
        1
    );

    let (_, non_prod) = w
        .get(&format!(
            "/v1/review/services/{id}?environment_class=non_production"
        ))
        .await;
    assert_eq!(non_prod["expectation"]["status"], "breached");
    assert_eq!(
        non_prod["expectation"]["objectives"][0]["status"],
        "breached"
    );

    // The production report is unchanged by the non-production failure.
    Mock::given(method("GET"))
        .and(path("/v1/catalog/services"))
        .and(query_param("context", "business_critical"))
        .and(query_param("offset", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [record(id, "live")], "total": 1,
        })))
        .mount(&w.catalog)
        .await;
    let (s, report) = w
        .get("/v1/review/services?environment_class=production")
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(report["context"], "business_critical");
    assert_eq!(report["items"][0]["expectation"]["status"], "compliant");
    let (_, report) = w
        .get("/v1/review/services?environment_class=non_production")
        .await;
    assert_eq!(report["items"][0]["expectation"]["status"], "breached");
}

#[tokio::test]
async fn review_retired_service_is_not_live_in_review_or_report() {
    let w = World::new().await;
    let id = Uuid::new_v4();
    w.service(id, "retired").await;
    // Neither provider may be asked about a retired service.
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&w.health)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&w.alerting)
        .await;

    let (s, body) = w
        .get(&format!(
            "/v1/review/services/{id}?environment_class=production"
        ))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["live"], false);
    assert_eq!(body["service"]["lifecycle"], "retired");
    assert!(body["health"].is_null());
    assert!(body["expectation"].is_null());

    // The full report lists the retired record as not live; business-critical reporting
    // delegates the live filter to the catalog.
    Mock::given(method("GET"))
        .and(path("/v1/catalog/services"))
        .and(query_param_is_missing("context"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [record(id, "retired")], "total": 1,
        })))
        .mount(&w.catalog)
        .await;
    let (_, report) = w
        .get("/v1/review/services?environment_class=production&context=all")
        .await;
    assert_eq!(report["items"][0]["live"], false);
    assert!(report["items"][0]["health"].is_null());
}

#[tokio::test]
async fn review_exposes_all_four_expectation_states_and_never_compliant_without_evidence() {
    let cases: [(&[Entry], &str, &str); 6] = [
        (
            &[("production", Some(("compliant", "current")))],
            "compliant",
            "current",
        ),
        (
            &[("production", Some(("at_risk", "current")))],
            "at_risk",
            "current",
        ),
        (
            &[("production", Some(("breached", "current")))],
            "breached",
            "current",
        ),
        (
            &[
                ("production", Some(("compliant", "current"))),
                ("production", Some(("unknown", "stale"))),
            ],
            "unknown",
            "stale",
        ),
        (
            &[("production", Some(("unknown", "missing")))],
            "unknown",
            "missing",
        ),
        // Pending approval: no compliance result exists, so nothing is agreed.
        (&[("production", None)], "unknown", "no_approved_objective"),
    ];
    for (entries, status, reason) in cases {
        let w = World::new().await;
        let id = Uuid::new_v4();
        w.service(id, "live").await;
        w.health_state(id, "production", "unknown", "missing").await;
        w.objectives(id, entries).await;
        let (s, body) = w
            .get(&format!(
                "/v1/review/services/{id}?environment_class=production"
            ))
            .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(body["expectation"]["status"], status, "{entries:?}");
        assert_eq!(body["expectation"]["reason"], reason, "{entries:?}");
        assert_eq!(body["health"]["state"], "unknown");
    }
}

#[tokio::test]
async fn review_hidden_service_is_not_found_without_provider_reads() {
    let w = World::new().await;
    let id = Uuid::new_v4();
    Mock::given(method("GET"))
        .and(path(format!("/v1/catalog/services/{id}")))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"error": "not_found"})))
        .mount(&w.catalog)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&w.health)
        .await;
    let (s, body) = w
        .get(&format!(
            "/v1/review/services/{id}?environment_class=production"
        ))
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({"error": "not_found"}));
    let (s, _) = w
        .get("/v1/review/services/not-a-uuid?environment_class=production")
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn review_provider_outages_yield_unknown_and_catalog_outage_fails_closed() {
    let w = World::new().await;
    let id = Uuid::new_v4();
    w.service(id, "live").await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&w.health)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&w.alerting)
        .await;
    let (s, body) = w
        .get(&format!(
            "/v1/review/services/{id}?environment_class=production"
        ))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["health"]["state"], "unknown");
    assert_eq!(body["health"]["evidence_status"], "evaluation_unavailable");
    assert_eq!(body["expectation"]["status"], "unknown");
    assert_eq!(body["expectation"]["reason"], "evaluation_unavailable");

    let w = World::new().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&w.catalog)
        .await;
    let (s, body) = w
        .get(&format!(
            "/v1/review/services/{id}?environment_class=production"
        ))
        .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "catalog_unavailable");
    let (s, _) = w
        .get("/v1/review/services?environment_class=production")
        .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn review_withheld_health_and_objectives_are_restricted_unknown() {
    let w = World::new().await;
    let id = Uuid::new_v4();
    w.service(id, "live").await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&w.health)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&w.alerting)
        .await;
    let (_, body) = w
        .get(&format!(
            "/v1/review/services/{id}?environment_class=production"
        ))
        .await;
    assert_eq!(body["health"]["evidence_status"], "restricted");
    assert_eq!(body["expectation"]["status"], "unknown");
    assert_eq!(body["expectation"]["reason"], "restricted");
}

#[tokio::test]
async fn review_rejects_missing_class_and_propagates_unauthorized() {
    let w = World::new().await;
    let id = Uuid::new_v4();
    for uri in [
        format!("/v1/review/services/{id}"),
        format!("/v1/review/services/{id}?environment_class=staging"),
        "/v1/review/services?environment_class=production&context=production".to_string(),
    ] {
        let (s, body) = w.get(&uri).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(body["error"], "invalid_request");
    }
    w.service(id, "live").await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&w.health)
        .await;
    w.objectives(id, &[]).await;
    let (s, _) = w
        .get(&format!(
            "/v1/review/services/{id}?environment_class=production"
        ))
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn review_is_unavailable_without_providers() {
    let router = Router::new()
        .route("/v1/review/services/{id}", get(get_service_review))
        .layer(Extension(ReviewProviders::disabled()));
    let resp = router
        .oneshot(
            Request::get(format!(
                "/v1/review/services/{}?environment_class=production",
                Uuid::new_v4()
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn review_discloses_scope_period_freshness_issues_and_full_confidence() {
    let w = World::new().await;
    let id = Uuid::new_v4();
    w.service(id, "live").await;
    w.health_state(id, "production", "normal", "current").await;
    w.issues(id, "production", 2).await;
    w.objectives(id, &[("production", Some(("compliant", "current")))])
        .await;
    let (s, body) = w
        .get(&format!(
            "/v1/review/services/{id}?environment_class=production"
        ))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        body["scope"],
        json!({"environment_class": "production", "service_id": id})
    );
    assert_eq!(body["period"]["freshness_seconds"], 300);
    assert!(body["period"]["as_of"].is_string());
    assert_eq!(body["freshness"]["status"], "current");
    assert_eq!(body["freshness"]["bound_seconds"], 300);
    assert!(body["freshness"]["age_seconds"].as_i64().unwrap() > 0);
    assert_eq!(body["issues"], json!({"status": "current", "open": 2}));
    assert_eq!(body["confidence"]["confidence"], "full");
    assert_eq!(body["confidence"]["affected"], json!([]));
    assert_eq!(body["fit_for_decision"], true);
    assert_eq!(body["unfit_reasons"], json!([]));
}

#[tokio::test]
async fn each_provider_outage_degrades_confidence_naming_the_function() {
    // (health self-status, alerting self-status, watermark) -> (confidence, affected)
    type Case = (
        Option<&'static str>,
        Option<&'static str>,
        Result<IngestWatermark, ()>,
    );
    let cases: [(Case, &str, Value); 7] = [
        (
            (None, Some("current"), seen(5)),
            "degraded",
            json!(["assess"]),
        ),
        (
            (Some("stale"), Some("current"), seen(5)),
            "degraded",
            json!(["assess"]),
        ),
        (
            (Some("current"), None, seen(5)),
            "degraded",
            json!(["detect"]),
        ),
        (
            (Some("current"), Some("stale"), seen(5)),
            "degraded",
            json!(["detect"]),
        ),
        (
            (Some("current"), Some("current"), Err(())),
            "degraded",
            json!(["ingest"]),
        ),
        (
            (Some("current"), Some("current"), seen(900)),
            "degraded",
            json!(["ingest"]),
        ),
        (
            (
                Some("not_started"),
                Some("current"),
                Ok(IngestWatermark::default()),
            ),
            "unknown",
            json!(["assess", "ingest"]),
        ),
    ];
    for ((health, alerting, watermark), confidence, affected) in cases {
        let w = World::new().await;
        w.self_statuses(health, alerting).await;
        *w.watermark.0.lock().unwrap() = watermark;
        let (s, body) = w.get("/v1/platform/confidence").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(body["confidence"], confidence, "{body}");
        assert_eq!(body["affected"], affected, "{body}");

        // A review that depends on it is not fit, though its own evidence is current.
        let id = Uuid::new_v4();
        w.service(id, "live").await;
        w.health_state(id, "production", "normal", "current").await;
        w.issues(id, "production", 0).await;
        w.objectives(id, &[]).await;
        let (_, review) = w
            .get(&format!(
                "/v1/review/services/{id}?environment_class=production"
            ))
            .await;
        assert_eq!(review["health"]["state"], "normal");
        assert_eq!(review["fit_for_decision"], false);
        assert_eq!(
            review["unfit_reasons"],
            json!([format!("confidence_{confidence}")])
        );
    }
}

#[tokio::test]
async fn confidence_is_unknown_without_signals_and_never_full() {
    let router = Router::new().route("/v1/platform/confidence", get(get_platform_confidence));
    let resp = router
        .oneshot(
            Request::get("/v1/platform/confidence")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["confidence"], "unknown");
    assert_eq!(body["affected"], json!(["assess", "detect", "ingest"]));
}

#[tokio::test]
async fn overview_lists_critical_services_and_never_shows_unread_issues_as_zero() {
    let w = World::new().await;
    let (fresh, stale) = (Uuid::new_v4(), Uuid::new_v4());
    w.caller(
        Mock::given(method("GET"))
            .and(path("/v1/catalog/services"))
            .and(query_param("context", "business_critical")),
    )
    .respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "items": [record(fresh, "live"), record(stale, "live")], "total": 2,
    })))
    .mount(&w.catalog)
    .await;
    w.health_state(fresh, "production", "degraded", "current")
        .await;
    w.health_state(stale, "production", "unknown", "stale")
        .await;
    w.issues(fresh, "production", 1).await;
    // The stale service's issue list cannot be read.
    Mock::given(method("GET"))
        .and(path(format!("/v1/health/services/{stale}/issues")))
        .respond_with(ResponseTemplate::new(503))
        .mount(&w.health)
        .await;
    w.objectives(fresh, &[]).await;
    w.objectives(stale, &[]).await;

    let (s, body) = w
        .get("/v1/overview/services?environment_class=production")
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(
        body["scope"],
        json!({"environment_class": "production", "context": "business_critical"})
    );
    assert_eq!(body["confidence"]["confidence"], "full");
    assert_eq!(body["total"], 2);
    let items = body["items"].as_array().unwrap();
    assert_eq!(items[0]["service"]["id"], json!(fresh));
    assert_eq!(items[0]["state"], "degraded");
    assert_eq!(items[0]["issues"], json!({"status": "current", "open": 1}));
    assert_eq!(items[0]["fit_for_decision"], true);
    assert_eq!(items[1]["state"], "unknown");
    assert_eq!(items[1]["freshness"]["status"], "stale");
    assert_eq!(
        items[1]["issues"],
        json!({"status": "evaluation_unavailable", "open": null})
    );
    assert_eq!(items[1]["fit_for_decision"], false);
    assert_eq!(items[1]["unfit_reasons"], json!(["evidence_stale"]));

    let (s, _) = w.get("/v1/overview/services").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn review_reproduces_past_period_using_historical_transitions_and_period_disclosure() {
    let w = World::new().await;
    let id = Uuid::new_v4();
    w.caller(
        Mock::given(method("GET"))
            .and(path("/v1/catalog/services"))
            .and(query_param("context", "business_critical")),
    )
    .respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "items": [{
            "id": id,
            "business_name": "Checkout",
            "domain": "commerce",
            "criticality": "critical",
            "lifecycle": "live",
            "accountable": true,
        }],
        "total": 1,
    })))
    .mount(&w.catalog)
    .await;
    // Current health is normal, but the past monthly period had a degradation transition.
    w.health_state(id, "production", "normal", "current").await;
    w.caller(
        Mock::given(method("GET"))
            .and(path(format!("/v1/health/services/{id}/history")))
            .and(query_param("environment_class", "production")),
    )
    .respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "service_id": id,
        "environment_class": "production",
        "since": "2026-10-01T00:00:00Z",
        "until": "2026-10-31T23:59:59Z",
        "evidence_status": "current",
        "transitions": [{
            "at": "2026-10-15T12:00:00Z",
            "from_state": "normal",
            "to_state": "degraded",
            "evidence_status": "current",
            "rule_id": "default",
            "rule_revision": 2,
        }],
        "interruptions": [{
            "started_at": "2026-10-15T12:00:00Z",
            "ended_at": null,
            "worst_state": "degraded",
        }],
    })))
    .mount(&w.health)
    .await;
    w.issues(id, "production", 0).await;
    w.objectives(id, &[]).await;

    let (s, body) = w
        .get("/v1/review/services?environment_class=production&from=2026-10-01T00:00:00Z&to=2026-10-31T23:59:59Z")
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["period"]["historical"], true);
    assert_eq!(body["period"]["from"], "2026-10-01T00:00:00Z");
    assert_eq!(body["period"]["to"], "2026-10-31T23:59:59Z");
    assert_eq!(body["period"]["definitions_as_of"], "2026-10-31T23:59:59Z");
    let item = &body["items"][0];
    assert_eq!(item["service"]["accountable"], true);
    assert_eq!(item["service"]["domain"], "commerce");
    assert_eq!(item["health"]["state"], "degraded");
    assert_eq!(item["history"]["transitions"][0]["rule_revision"], 2);
    assert_eq!(
        item["history"]["interruptions"][0]["worst_state"],
        "degraded"
    );

    let (s, _) = w
        .get("/v1/review/services?environment_class=production&from=2026-10-31T00:00:00Z&to=2026-10-01T00:00:00Z")
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = w
        .get("/v1/review/services?environment_class=production&from=2026-10-01T00:00:00Z")
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn review_and_overview_propagate_accountable_filter_to_catalog() {
    let w = World::new().await;
    let mine = Uuid::new_v4();
    w.caller(
        Mock::given(method("GET"))
            .and(path("/v1/catalog/services"))
            .and(query_param("context", "business_critical"))
            .and(query_param("accountable", "true")),
    )
    .respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "items": [{
            "id": mine,
            "business_name": "Payments",
            "domain": "billing",
            "criticality": "critical",
            "lifecycle": "live",
            "accountable": true,
        }],
        "total": 1,
    })))
    .mount(&w.catalog)
    .await;
    w.health_state(mine, "production", "normal", "current")
        .await;
    w.issues(mine, "production", 0).await;
    w.objectives(mine, &[]).await;

    let (s, body) = w
        .get("/v1/overview/services?environment_class=production&accountable=true")
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["service"]["accountable"], true);
    assert_eq!(body["items"][0]["service"]["domain"], "billing");
}
