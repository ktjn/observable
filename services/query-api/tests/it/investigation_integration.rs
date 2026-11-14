//! HTTP integration tests for `support-investigation` (Phase 6 S2:
//! OBS-09-01, OBS-09-02, OBS-09-03, OBS-09-08) exercising the full handler
//! path via `tower::ServiceExt::oneshot` against wiremock upstream providers.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Extension, Router,
    body::Body,
    http::{Request, StatusCode},
    routing::{get, post},
};
use chrono::{DateTime, Duration, Utc};
use http_body_util::BodyExt;
use query_api::investigation::{
    BaselineMetricsSource, InvestigationProviders, PeriodMetrics, evaluate_causes,
    get_investigation_context,
};
use query_api::middleware::auth::TenantContext;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct MockBaselineSource {
    baseline: PeriodMetrics,
    degraded: PeriodMetrics,
    baseline_end: DateTime<Utc>,
}

#[async_trait::async_trait]
impl BaselineMetricsSource for MockBaselineSource {
    async fn read_period(
        &self,
        _tenant_id: Uuid,
        _service_name: &str,
        _from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<PeriodMetrics, ()> {
        if to <= self.baseline_end {
            Ok(self.baseline.clone())
        } else {
            Ok(self.degraded.clone())
        }
    }
}

fn build_app(
    providers: InvestigationProviders,
    baseline: Arc<dyn BaselineMetricsSource>,
    tenant_id: Uuid,
) -> Router {
    Router::new()
        .route("/v1/investigations/context", get(get_investigation_context))
        .route("/v1/investigations/causes/validate", post(evaluate_causes))
        .layer(Extension(providers))
        .layer(Extension(baseline))
        .layer(Extension(TenantContext {
            tenant_id,
            user_id: None,
            role: "responder".into(),
        }))
}

async fn json_body(body: Body) -> Value {
    let bytes = body.collect().await.expect("body collected").to_bytes();
    serde_json::from_slice(&bytes).expect("valid JSON")
}

fn urlencoding_utc(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[tokio::test]
async fn obs_09_01_begin_from_service_status_and_withhold_restricted_service() {
    let catalog = MockServer::start().await;
    let health = MockServer::start().await;
    let alerting = MockServer::start().await;
    let control = MockServer::start().await;

    let tenant_id = Uuid::new_v4();
    let allowed_id = Uuid::new_v4();
    let restricted_id = Uuid::new_v4();

    Mock::given(method("GET"))
        .and(path(format!("/v1/catalog/services/{allowed_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": allowed_id,
            "operational_name": "checkout",
            "dependencies": [{"service": "payments", "kind": "sync"}]
        })))
        .mount(&catalog)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v1/catalog/services/{restricted_id}")))
        .respond_with(ResponseTemplate::new(403))
        .mount(&catalog)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v1/health/services/{allowed_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "degraded",
            "evidence_status": "current"
        })))
        .mount(&health)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/deployments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
        .mount(&control)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/events/changes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
        .mount(&control)
        .await;

    let now = Utc::now();
    let from = now - Duration::minutes(15);
    let providers = InvestigationProviders::http(
        catalog.uri(),
        health.uri(),
        alerting.uri(),
        control.uri(),
        reqwest::Client::new(),
    );
    let baseline: Arc<dyn BaselineMetricsSource> = Arc::new(MockBaselineSource {
        baseline: PeriodMetrics {
            p95_latency_ms: 80.0,
            error_rate: 0.002,
            request_count: 1000,
            observed_buckets: 15,
            expected_buckets: 15,
        },
        degraded: PeriodMetrics {
            p95_latency_ms: 420.0,
            error_rate: 0.08,
            request_count: 1000,
            observed_buckets: 15,
            expected_buckets: 15,
        },
        baseline_end: from,
    });
    let app = build_app(providers, baseline, tenant_id);

    // Acceptance: investigator starts from service status and narrows the failure period
    let ok_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/investigations/context?service_id={allowed_id}&from={}&to={}",
                    urlencoding_utc(&from),
                    urlencoding_utc(&now),
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok_resp.status(), StatusCode::OK);
    let ok_json = json_body(ok_resp.into_body()).await;
    assert_eq!(ok_json["service_name"], "checkout");
    assert_eq!(ok_json["service_status"]["status"], "degraded");

    // Negative: restricted service cannot have its evidence opened
    let denied_resp = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/investigations/context?service_id={restricted_id}&from={}&to={}",
                    urlencoding_utc(&from),
                    urlencoding_utc(&now),
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied_resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn obs_09_02_related_evidence_and_lookback_changes_not_omitted() {
    let catalog = MockServer::start().await;
    let health = MockServer::start().await;
    let alerting = MockServer::start().await;
    let control = MockServer::start().await;

    let tenant_id = Uuid::new_v4();
    let service_id = Uuid::new_v4();
    let incident_id = Uuid::new_v4();
    let now = Utc::now();
    let from = now - Duration::minutes(20);
    let lookback_change_time = from - Duration::minutes(25); // outside [from, to], inside 60m lookback
    let in_period_deploy_time = now - Duration::minutes(10);

    Mock::given(method("GET"))
        .and(path(format!("/v1/catalog/services/{service_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": service_id,
            "operational_name": "checkout",
            "dependencies": [{"service": "payments", "kind": "sync"}]
        })))
        .mount(&catalog)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v1/health/services/{service_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "status": "degraded" })))
        .mount(&health)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v1/incidents/{incident_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "incident_id": incident_id,
            "title": "Checkout 5xx",
            "status": "investigating"
        })))
        .mount(&alerting)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/deployments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{
                "deployment_id": "dep-1",
                "service_version": "v2.1.0",
                "started_at": in_period_deploy_time.to_rfc3339()
            }]
        })))
        .mount(&control)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/events/changes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{
                "change_event_id": "chg-lookback",
                "event_type": "config_change",
                "title": "Increased retry timeout",
                "occurred_at": lookback_change_time.to_rfc3339()
            }]
        })))
        .mount(&control)
        .await;

    let providers = InvestigationProviders::http(
        catalog.uri(),
        health.uri(),
        alerting.uri(),
        control.uri(),
        reqwest::Client::new(),
    );
    let baseline: Arc<dyn BaselineMetricsSource> = Arc::new(MockBaselineSource {
        baseline: PeriodMetrics {
            p95_latency_ms: 80.0,
            error_rate: 0.002,
            request_count: 1000,
            observed_buckets: 20,
            expected_buckets: 20,
        },
        degraded: PeriodMetrics {
            p95_latency_ms: 420.0,
            error_rate: 0.08,
            request_count: 1000,
            observed_buckets: 20,
            expected_buckets: 20,
        },
        baseline_end: from,
    });
    let app = build_app(providers, baseline, tenant_id);

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/investigations/context?service_id={service_id}&incident_id={incident_id}&from={}&to={}&lookback_minutes=60",
                    urlencoding_utc(&from),
                    urlencoding_utc(&now),
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp.into_body()).await;

    // Acceptance: incident, dependencies, and changes all present in one context
    assert_eq!(body["incident"]["incident_id"], incident_id.to_string());
    assert_eq!(body["dependencies"][0]["service"], "payments");

    let changes = body["changes"].as_array().unwrap();
    let lookback_chg = changes.iter().find(|c| c["id"] == "chg-lookback").unwrap();
    assert_eq!(lookback_chg["within_selected_period"], false);
    assert_eq!(lookback_chg["within_lookback"], true);
    let in_period_dep = changes.iter().find(|c| c["id"] == "dep-1").unwrap();
    assert_eq!(in_period_dep["within_selected_period"], true);
}

#[tokio::test]
async fn obs_09_03_baseline_comparison_and_incomplete_evidence_labeling() {
    let catalog = MockServer::start().await;
    let health = MockServer::start().await;
    let alerting = MockServer::start().await;
    let control = MockServer::start().await;

    let tenant_id = Uuid::new_v4();
    let service_id = Uuid::new_v4();
    let now = Utc::now();
    let from = now - Duration::minutes(30);

    Mock::given(method("GET"))
        .and(path(format!("/v1/catalog/services/{service_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": service_id,
            "operational_name": "checkout",
            "dependencies": []
        })))
        .mount(&catalog)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/health/services/{service_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "status": "degraded" })))
        .mount(&health)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/deployments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
        .mount(&control)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/events/changes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
        .mount(&control)
        .await;

    let providers = InvestigationProviders::http(
        catalog.uri(),
        health.uri(),
        alerting.uri(),
        control.uri(),
        reqwest::Client::new(),
    );

    // 1. Complete baseline -> identifies deviation
    let complete_app = build_app(
        providers.clone(),
        Arc::new(MockBaselineSource {
            baseline: PeriodMetrics {
                p95_latency_ms: 100.0,
                error_rate: 0.001,
                request_count: 5000,
                observed_buckets: 30,
                expected_buckets: 30,
            },
            degraded: PeriodMetrics {
                p95_latency_ms: 450.0,
                error_rate: 0.05,
                request_count: 5000,
                observed_buckets: 30,
                expected_buckets: 30,
            },
            baseline_end: from,
        }),
        tenant_id,
    );
    let complete_resp = complete_app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/investigations/context?service_id={service_id}&from={}&to={}",
                    urlencoding_utc(&from),
                    urlencoding_utc(&now),
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let complete_body = json_body(complete_resp.into_body()).await;
    assert_eq!(complete_body["comparison"]["baseline_status"], "complete");
    assert_eq!(complete_body["comparison"]["deviation_detected"], true);
    assert_eq!(complete_body["comparison"]["latency_delta_ms"], 350.0);

    // 2. Incomplete baseline -> labeled incomplete_evidence before comparison
    let incomplete_app = build_app(
        providers,
        Arc::new(MockBaselineSource {
            baseline: PeriodMetrics {
                p95_latency_ms: 100.0,
                error_rate: 0.001,
                request_count: 500,
                observed_buckets: 8,
                expected_buckets: 30,
            },
            degraded: PeriodMetrics {
                p95_latency_ms: 450.0,
                error_rate: 0.05,
                request_count: 5000,
                observed_buckets: 30,
                expected_buckets: 30,
            },
            baseline_end: from,
        }),
        tenant_id,
    );
    let incomplete_resp = incomplete_app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/investigations/context?service_id={service_id}&from={}&to={}",
                    urlencoding_utc(&from),
                    urlencoding_utc(&now),
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let incomplete_body = json_body(incomplete_resp.into_body()).await;
    assert_eq!(
        incomplete_body["comparison"]["baseline_status"],
        "incomplete_evidence"
    );
    assert!(
        incomplete_body["comparison"]["baseline_incomplete_warning"]
            .as_str()
            .unwrap()
            .contains("8 of 30")
    );
    assert!(incomplete_body["comparison"]["latency_delta_ms"].is_null());
}

#[tokio::test]
async fn obs_09_08_distinguish_verified_causes_from_hypotheses_and_reject_unsupported_verified() {
    let tenant_id = Uuid::new_v4();
    let app = build_app(
        InvestigationProviders::disabled(),
        Arc::new(MockBaselineSource {
            baseline: PeriodMetrics {
                p95_latency_ms: 0.0,
                error_rate: 0.0,
                request_count: 0,
                observed_buckets: 0,
                expected_buckets: 0,
            },
            degraded: PeriodMetrics {
                p95_latency_ms: 0.0,
                error_rate: 0.0,
                request_count: 0,
                observed_buckets: 0,
                expected_buckets: 0,
            },
            baseline_end: Utc::now(),
        }),
        tenant_id,
    );

    // Acceptance: verified cause with evidence vs unverified hypothesis
    let ok_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/investigations/causes/validate")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "causes": [
                            {
                                "title": "DB pool exhaustion after v2.1.0",
                                "classification": "verified_cause",
                                "evidence_refs": [{
                                    "evidence_id": "trace-abc-123",
                                    "evidence_type": "trace",
                                    "summary": "Span db.acquire timed out after 5000ms"
                                }]
                            },
                            {
                                "title": "Possible downstream cache miss",
                                "classification": "hypothesis",
                                "evidence_refs": []
                            }
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok_resp.status(), StatusCode::OK);
    let ok_body = json_body(ok_resp.into_body()).await;
    assert_eq!(ok_body["verified_causes"].as_array().unwrap().len(), 1);
    assert_eq!(ok_body["verified_causes"][0]["verified"], true);
    assert_eq!(ok_body["hypotheses"].as_array().unwrap().len(), 1);
    assert_eq!(ok_body["hypotheses"][0]["verified"], false);

    // Negative: verified_cause with no supporting evidence is rejected (422)
    let bad_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/investigations/causes/validate")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "causes": [{
                            "title": "Unproven guess",
                            "classification": "verified_cause",
                            "evidence_refs": []
                        }]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let _unused: HashMap<String, String> = HashMap::new();
}
