//! Approved objectives per catalog service and class (phase 1 C3, OBS-01-04) through the
//! handlers, with real PostgreSQL and real ClickHouse evidence and a catalog substitute.

use std::sync::Arc;

use alert_evaluator::middleware::auth::TenantContext;
use alert_evaluator::objectives::{self, CatalogRecord, ObjectiveCatalog, ScopedBinding};
use alert_evaluator::{AppState, observability};
use axum::{
    Extension, Router,
    body::Body,
    http::Request,
    routing::{get, patch, post, put},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

struct World {
    state: AppState,
    catalog: ObjectiveCatalog,
    tenant: Uuid,
    admin: Uuid,
    member: Uuid,
    _db: test_support::postgres::TestDatabase,
}

impl World {
    async fn start() -> Self {
        let (owner, guard) = test_support::postgres::isolated_pool().await;
        // The component's own login, so missing grants fail here and not in production.
        let db = test_support::postgres::scoped_pool(&owner, "observable_alerting").await;
        let ch = test_support::clickhouse::shared_client().await;
        Self {
            state: AppState {
                db,
                ch,
                auth_service_url: "http://unused".into(),
                http_client: reqwest::Client::new(),
                metrics: Arc::new(observability::AlertEvaluatorMetrics::new()),
                producer: None,
            },
            catalog: ObjectiveCatalog::substitute(),
            tenant: Uuid::new_v4(),
            admin: Uuid::new_v4(),
            member: Uuid::new_v4(),
            _db: guard,
        }
    }

    fn ctx(&self, who: &str) -> TenantContext {
        let (user_id, role) = match who {
            "admin" => (Some(self.admin), "tenant_admin"),
            "member" => (Some(self.member), "member"),
            "viewer" => (Some(Uuid::new_v4()), "viewer"),
            "admin-key" => (None, "tenant_admin"),
            "other" => {
                return TenantContext {
                    tenant_id: Uuid::new_v4(),
                    user_id: Some(Uuid::new_v4()),
                    role: "tenant_admin".into(),
                };
            }
            _ => unreachable!(),
        };
        TenantContext {
            tenant_id: self.tenant,
            user_id,
            role: role.into(),
        }
    }

    async fn call(&self, who: &str, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
        let app = Router::new()
            .route(
                "/v1/objectives",
                get(objectives::handle_list).post(objectives::handle_create),
            )
            .route("/v1/objectives/{id}", patch(objectives::handle_update))
            .route(
                "/v1/objectives/{id}/approve",
                post(objectives::handle_approve),
            )
            .route(
                "/v1/objectives/{id}/compliance",
                get(objectives::handle_compliance),
            )
            .route("/v1/objectives/{id}/export", get(objectives::handle_export))
            .route(
                "/v1/objectives/{id}/contract",
                put(objectives::handle_put_contract),
            )
            .route(
                "/v1/objectives/{id}/sla-review",
                get(objectives::handle_sla_review),
            )
            .layer(Extension(self.ctx(who)))
            .layer(Extension(self.catalog.clone()))
            .with_state(self.state.clone());
        let mut req = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(b) => {
                req = req.header("content-type", "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let resp = app.oneshot(req.body(body).unwrap()).await.unwrap();
        let status = resp.status().as_u16();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// A live catalog service bound to `name` in prod (production) and staging (non-production).
    fn service(&self, name: &str) -> Uuid {
        let id = Uuid::new_v4();
        self.put(id, name, "live");
        id
    }

    fn put(&self, id: Uuid, name: &str, lifecycle: &str) {
        let b = |env: &str, class: Option<&'static str>| ScopedBinding {
            service_name: name.into(),
            environment: env.into(),
            class,
        };
        self.catalog.put(
            self.tenant,
            id,
            CatalogRecord {
                lifecycle: lifecycle.into(),
                bindings: vec![
                    b("prod", Some("production")),
                    b("staging", Some("non_production")),
                    b("", None),
                    b("lab", None),
                ],
            },
        );
    }

    /// Spans for (name, env): `total` requests of which `bad` errored, `age_s` seconds ago.
    async fn spans(&self, name: &str, env: &str, total: u32, bad: u32, age_s: i64) {
        let at = (chrono::Utc::now() - chrono::Duration::seconds(age_s))
            .timestamp_nanos_opt()
            .unwrap();
        let rows: Vec<String> = (0..total)
            .map(|i| {
                let status = if i < bad { "ERROR" } else { "OK" };
                format!(
                    "('{}', '{}', '{}', '{name}', 'GET /', {at}, {at}, 1, '{env}', '{status}')",
                    self.tenant,
                    Uuid::new_v4().simple(),
                    Uuid::new_v4().simple()
                )
            })
            .collect();
        self.state
            .ch
            .query(&format!(
                "INSERT INTO observable.spans (tenant_id, trace_id, span_id, service_name, \
                 operation_name, start_time_unix_nano, end_time_unix_nano, duration_ns, \
                 environment, status_code) VALUES {}",
                rows.join(", ")
            ))
            .execute()
            .await
            .expect("insert spans");
    }

    async fn objective(&self, service: Uuid, class: &str, approve: bool) -> Uuid {
        let (status, body) = self
            .call(
                "member",
                "POST",
                "/v1/objectives",
                Some(json!({"service_id": service, "environment_class": class,
                            "target": 0.99, "window_days": 7})),
            )
            .await;
        assert_eq!(status, 201, "{body}");
        let id: Uuid = body["objective_id"].as_str().unwrap().parse().unwrap();
        if approve {
            let (status, body) = self
                .call(
                    "admin",
                    "POST",
                    &format!("/v1/objectives/{id}/approve"),
                    None,
                )
                .await;
            assert_eq!(status, 200, "{body}");
        }
        id
    }

    async fn compliance(&self, id: Uuid) -> (u16, Value) {
        self.call(
            "viewer",
            "GET",
            &format!("/v1/objectives/{id}/compliance"),
            None,
        )
        .await
    }
}

fn name() -> String {
    format!("svc-{}", Uuid::new_v4().simple())
}

#[tokio::test]
async fn compliant_at_risk_and_breached_for_a_selected_service() {
    let w = World::start().await;
    // Target 99%: 0.5% errors is compliant, 0.8% consumes 80% of the budget, 2% breaches.
    for (bad, want) in [(5, "compliant"), (8, "at_risk"), (20, "breached")] {
        let n = name();
        let service = w.service(&n);
        w.spans(&n, "prod", 1000, bad, 30).await;
        let id = w.objective(service, "production", true).await;
        let (status, body) = w.compliance(id).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["status"], json!(want), "{bad}: {body}");
        assert_eq!(body["evidence_status"], json!("current"));
        assert_eq!(body["total_events"], json!(1000));
        assert_eq!(body["good_events"], json!(1000 - bad));
        let attainment = body["attainment"].as_f64().unwrap();
        assert!((attainment - (1.0 - f64::from(bad) / 1000.0)).abs() < 1e-9);
    }
}

#[tokio::test]
async fn missing_or_stale_evidence_is_unknown_never_compliant() {
    let w = World::start().await;
    let absent = w.service(&name());
    let id = w.objective(absent, "production", true).await;
    let (_, body) = w.compliance(id).await;
    assert_eq!(
        (
            &body["status"],
            &body["evidence_status"],
            &body["attainment"]
        ),
        (&json!("unknown"), &json!("missing"), &json!(null))
    );

    // Perfect but old evidence (older than the 900 s freshness bound) is not current.
    let n = name();
    let stale = w.service(&n);
    w.spans(&n, "prod", 100, 0, 3600).await;
    let id = w.objective(stale, "production", true).await;
    let (_, body) = w.compliance(id).await;
    assert_eq!(
        (
            &body["status"],
            &body["evidence_status"],
            &body["attainment"]
        ),
        (&json!("unknown"), &json!("stale"), &json!(null))
    );
    assert!(!body["last_evidence_at"].is_null());

    // Unscoped and unclassified bindings never count: spans with no environment or in "lab".
    let n = name();
    let unscoped = w.service(&n);
    w.spans(&n, "", 100, 0, 10).await;
    w.spans(&n, "lab", 100, 0, 10).await;
    let id = w.objective(unscoped, "production", true).await;
    assert_eq!(
        w.compliance(id).await.1["evidence_status"],
        json!("missing")
    );
}

#[tokio::test]
async fn production_and_non_production_are_evaluated_apart() {
    let w = World::start().await;
    let n = name();
    let service = w.service(&n);
    w.spans(&n, "prod", 1000, 1, 10).await;
    w.spans(&n, "staging", 1000, 300, 10).await;
    let prod = w.objective(service, "production", true).await;
    let staging = w.objective(service, "non_production", true).await;
    assert_eq!(w.compliance(prod).await.1["status"], json!("compliant"));
    assert_eq!(w.compliance(staging).await.1["status"], json!("breached"));
    // One objective per service, class and SLI.
    let (status, _) = w
        .call(
            "member",
            "POST",
            "/v1/objectives",
            Some(
                json!({"service_id": service, "environment_class": "production",
                        "target": 0.95, "window_days": 7}),
            ),
        )
        .await;
    assert_eq!(status, 409);
}

#[tokio::test]
async fn unapproved_expectations_produce_no_compliance_result() {
    let w = World::start().await;
    let n = name();
    let service = w.service(&n);
    w.spans(&n, "prod", 1000, 0, 10).await;
    let id = w.objective(service, "production", false).await;
    assert_eq!(
        w.compliance(id).await,
        (409, json!({"error": "objective_not_approved"}))
    );

    // Only a tenant admin, as a person, can approve.
    for who in ["member", "viewer", "admin-key"] {
        let (status, _) = w
            .call(who, "POST", &format!("/v1/objectives/{id}/approve"), None)
            .await;
        assert_eq!(status, 403, "{who}");
    }
    let (status, body) = w
        .call(
            "admin",
            "POST",
            &format!("/v1/objectives/{id}/approve"),
            None,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(body["approved_by"], json!(w.admin));
    assert_eq!(w.compliance(id).await.1["status"], json!("compliant"));

    // Changing the definition clears the approval until it is agreed again.
    let (status, body) = w
        .call(
            "member",
            "PATCH",
            &format!("/v1/objectives/{id}"),
            Some(json!({"target": 0.999})),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["approved_at"], json!(null));
    assert_eq!(w.compliance(id).await.0, 409);
    // A viewer cannot redefine an objective.
    let (status, _) = w
        .call(
            "viewer",
            "PATCH",
            &format!("/v1/objectives/{id}"),
            Some(json!({"target": 0.5})),
        )
        .await;
    assert_eq!(status, 403);
}

#[tokio::test]
async fn hidden_retired_and_foreign_services_fail_closed() {
    let w = World::start().await;
    let n = name();
    let service = w.service(&n);
    w.spans(&n, "prod", 100, 0, 10).await;
    let id = w.objective(service, "production", true).await;

    // Another tenant's caller sees nothing.
    let (status, _) = w
        .call(
            "other",
            "GET",
            &format!("/v1/objectives/{id}/compliance"),
            None,
        )
        .await;
    assert_eq!(status, 404);

    // A retired service has no current compliance and takes no new objective.
    w.put(service, &n, "retired");
    assert_eq!(w.compliance(id).await.0, 404);
    let other = w.service(&name());
    w.put(other, "x", "retired");
    let (status, body) = w
        .call(
            "member",
            "POST",
            "/v1/objectives",
            Some(
                json!({"service_id": other, "environment_class": "production",
                        "target": 0.99, "window_days": 7}),
            ),
        )
        .await;
    assert_eq!((status, &body["error"]), (422, &json!("service_retired")));
    w.put(service, &n, "live");

    // A service the caller cannot read hides its objectives like a missing one.
    w.catalog.hide(service);
    assert_eq!(w.compliance(id).await, (404, json!({"error": "not_found"})));
    let (status, _) = w
        .call(
            "viewer",
            "GET",
            &format!("/v1/objectives?service_id={service}"),
            None,
        )
        .await;
    assert_eq!(status, 404);
    assert_eq!(w.compliance(Uuid::new_v4()).await.0, 404);

    // Catalog outage: no result.
    w.catalog.set_unavailable(true);
    assert_eq!(
        w.compliance(id).await,
        (503, json!({"error": "catalog_unavailable"}))
    );
}

#[tokio::test]
async fn invalid_definitions_are_refused() {
    let w = World::start().await;
    let service = w.service(&name());
    for body in [
        json!({"service_id": service, "environment_class": "staging", "target": 0.99, "window_days": 7}),
        json!({"service_id": service, "environment_class": "production", "target": 1.0, "window_days": 7}),
        json!({"service_id": service, "environment_class": "production", "target": 0.99, "window_days": 0}),
        json!({"service_id": service, "environment_class": "production", "target": 0.99, "window_days": 7, "at_risk_budget_fraction": 1.0}),
        json!({"service_id": service, "environment_class": "production", "target": 0.99, "window_days": 7, "freshness_seconds": 5}),
    ] {
        let (status, _) = w
            .call("member", "POST", "/v1/objectives", Some(body.clone()))
            .await;
        assert_eq!(status, 400, "{body}");
    }
    let (status, _) = w
        .call(
            "viewer",
            "POST",
            "/v1/objectives",
            Some(
                json!({"service_id": service, "environment_class": "production",
                        "target": 0.99, "window_days": 7}),
            ),
        )
        .await;
    assert_eq!(status, 403);
}

#[tokio::test]
async fn http_catalog_scope_uses_bindings_and_approved_contexts_with_the_callers_credential() {
    use axum::{Json, extract::Path, http::HeaderMap, http::StatusCode, response::IntoResponse};
    let (known, tenant) = (Uuid::new_v4(), Uuid::new_v4());
    let authed = |h: &HeaderMap| {
        h.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer session")
    };
    let app = Router::new()
        .route(
            "/v1/catalog/services/{id}",
            get(move |h: HeaderMap, Path(id): Path<Uuid>| async move {
                if !authed(&h) {
                    return StatusCode::UNAUTHORIZED.into_response();
                }
                if id != known {
                    return (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"})))
                        .into_response();
                }
                Json(json!({"id": known, "lifecycle": "live"})).into_response()
            }),
        )
        .route(
            "/v1/catalog/services/{id}/bindings",
            get(move |h: HeaderMap| async move {
                if !authed(&h) {
                    return StatusCode::UNAUTHORIZED.into_response();
                }
                Json(json!({"bindings": [
                    {"service_name": "pay", "environment": ""},
                    {"service_name": "pay", "environment": "lab"},
                    {"service_name": "pay", "environment": "prod"},
                    {"service_name": "pay", "environment": "staging"},
                ]}))
                .into_response()
            }),
        )
        .route(
            "/v1/catalog/contexts",
            get(move |h: HeaderMap| async move {
                if !authed(&h) {
                    return StatusCode::UNAUTHORIZED.into_response();
                }
                Json(json!({
                    "contexts": [
                        {"context": "production", "environments": ["prod"]},
                        {"context": "non_production", "environments": ["staging"]},
                        {"context": "business_critical", "services": []},
                    ],
                    "unclassified_environments": ["lab"],
                }))
                .into_response()
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let catalog = ObjectiveCatalog::Http {
        base_url: Arc::new(format!("http://{addr}")),
        client: reqwest::Client::new(),
    };
    let ctx = TenantContext {
        tenant_id: tenant,
        user_id: Some(Uuid::new_v4()),
        role: "viewer".into(),
    };
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer session".parse().unwrap());
    assert_eq!(
        catalog.lifecycle(&ctx, &headers, known).await.unwrap(),
        "live"
    );
    assert_eq!(
        catalog
            .scope(&ctx, &headers, known, "production")
            .await
            .unwrap(),
        vec![("pay".to_owned(), "prod".to_owned())]
    );
    assert_eq!(
        catalog
            .scope(&ctx, &headers, known, "non_production")
            .await
            .unwrap(),
        vec![("pay".to_owned(), "staging".to_owned())]
    );
    assert_eq!(
        catalog.lifecycle(&ctx, &headers, Uuid::new_v4()).await,
        Err(objectives::CatalogError::NotFound)
    );
    assert_eq!(
        catalog.lifecycle(&ctx, &HeaderMap::new(), known).await,
        Err(objectives::CatalogError::Unauthorized)
    );
    assert_eq!(
        ObjectiveCatalog::Disabled
            .lifecycle(&ctx, &headers, known)
            .await,
        Err(objectives::CatalogError::Unavailable)
    );
}

/// OBS-16-09: the approved objective and sample evidence leave as an OpenSLO v1 `SLO`
/// document that a consumer interprets without Observable's types: the checks below read
/// only the OpenSLO v1 field names and the evidence numbers.
#[tokio::test]
async fn objective_and_sample_evidence_export_in_openslo_form() {
    let w = World::start().await;
    let n = name();
    let service = w.service(&n);
    w.spans(&n, "prod", 1000, 5, 30).await;
    let id = w.objective(service, "production", true).await;

    let (status, body) = w
        .call(
            "viewer",
            "GET",
            &format!("/v1/objectives/{id}/export?format=openslo"),
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["format"], json!("openslo/v1"));
    let doc = &body["documents"][0];
    // Round-trip through text, as an independent consumer receives it.
    let doc: Value = serde_json::from_str(&doc.to_string()).unwrap();
    assert_eq!(doc["apiVersion"], json!("openslo/v1"));
    assert_eq!(doc["kind"], json!("SLO"));
    assert!(
        doc["metadata"]["name"]
            .as_str()
            .unwrap()
            .starts_with("objective-")
    );
    let spec = &doc["spec"];
    assert_eq!(spec["service"], json!(service.to_string()));
    assert_eq!(spec["budgetingMethod"], json!("Occurrences"));
    assert_eq!(spec["timeWindow"][0]["duration"], json!("7d"));
    assert_eq!(spec["timeWindow"][0]["isRolling"], json!(true));
    assert_eq!(spec["objectives"][0]["target"], json!(0.99));
    let ratio = &spec["indicator"]["spec"]["ratioMetric"];
    assert_eq!(ratio["counter"], json!(true));
    assert!(ratio["good"]["metricSource"]["type"].is_string());
    assert!(ratio["total"]["metricSource"]["type"].is_string());

    // Sample evidence a reviewer can recompute: good / total against the exported target.
    let ev = &body["evidence"];
    assert_eq!(ev["evidence_status"], json!("current"));
    assert_eq!(ev["total_events"], json!(1000));
    assert_eq!(ev["good_events"], json!(995));
    let good = ev["good_events"].as_f64().unwrap() / ev["total_events"].as_f64().unwrap();
    assert!(good >= spec["objectives"][0]["target"].as_f64().unwrap());
    assert_eq!(ev["status"], json!("compliant"));
}

#[tokio::test]
async fn unapproved_hidden_or_unknown_format_objectives_are_not_exported() {
    let w = World::start().await;
    let n = name();
    let service = w.service(&n);
    let draft = w.objective(service, "production", false).await;
    let (status, _) = w
        .call(
            "viewer",
            "GET",
            &format!("/v1/objectives/{draft}/export"),
            None,
        )
        .await;
    assert_eq!(status, 409, "an unapproved definition is not exchanged");

    let approved = w.objective(service, "non_production", true).await;
    let (status, _) = w
        .call(
            "viewer",
            "GET",
            &format!("/v1/objectives/{approved}/export?format=prometheus"),
            None,
        )
        .await;
    assert_eq!(status, 400);
    let (status, _) = w
        .call(
            "other",
            "GET",
            &format!("/v1/objectives/{approved}/export"),
            None,
        )
        .await;
    assert_eq!(status, 404, "another tenant cannot export it");
}

// ── OBS-06-10 contractual SLA review ─────────────────────────────────────────

fn ts(age_s: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::seconds(age_s))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn terms() -> Value {
    json!({
        "contract_ref": "MSA-2026-031 schedule B",
        "contracted_target": 0.995,
        "period_fee_minor": 500_000,
        "currency": "EUR",
        "credit_tiers": [
            {"below": 0.95, "credit_percent": 25.0},
            {"below": 0.99, "credit_percent": 10.0}
        ]
    })
}

async fn contracted(w: &World, n: &str) -> Uuid {
    let id = w.objective(w.service(n), "production", true).await;
    let (status, body) = w
        .call(
            "admin",
            "PUT",
            &format!("/v1/objectives/{id}/contract"),
            Some(terms()),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    id
}

#[tokio::test]
async fn contract_review_shows_attainment_and_credit_labeled_evidence_or_estimate() {
    let w = World::start().await;
    let n = name();
    let id = contracted(&w, &n).await;
    // Closed period from 3 h to 1 h ago, covered from start to end: 2% errors.
    for age in [10_700, 9_000, 7_300] {
        w.spans(&n, "prod", 100, 2, age).await;
    }
    let uri = format!(
        "/v1/objectives/{id}/sla-review?from={}&to={}",
        ts(10_800),
        ts(7_200)
    );
    let (status, body) = w.call("viewer", "GET", &uri, None).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["basis"], "evidence", "{body}");
    assert_eq!(body["basis_reasons"], json!([]));
    assert_eq!(body["contract_ref"], "MSA-2026-031 schedule B");
    assert_eq!(body["total_events"], 300);
    assert!((body["attainment"].as_f64().unwrap() - 0.98).abs() < 1e-9);
    assert_eq!(
        body["obligation"],
        json!({"met": false, "credit_percent": 10.0, "credit_minor": 50_000, "currency": "EUR"})
    );

    // The same contract over a period still running is an estimate.
    let uri = format!(
        "/v1/objectives/{id}/sla-review?from={}&to={}",
        ts(10_800),
        ts(-3_600)
    );
    let (status, body) = w.call("viewer", "GET", &uri, None).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["basis"], "estimate");
    let reasons = body["basis_reasons"].as_array().unwrap();
    assert!(reasons.contains(&json!("period_in_progress")), "{body}");
    assert!(reasons.contains(&json!("evidence_ends_early")), "{body}");

    // A closed period whose evidence stops early is an estimate, never evidence.
    let n2 = name();
    let id2 = contracted(&w, &n2).await;
    w.spans(&n2, "prod", 1000, 0, 10_700).await;
    let uri = format!(
        "/v1/objectives/{id2}/sla-review?from={}&to={}",
        ts(10_800),
        ts(3_600)
    );
    let (_, body) = w.call("viewer", "GET", &uri, None).await;
    assert_eq!(body["basis"], "estimate");
    assert_eq!(body["basis_reasons"], json!(["evidence_ends_early"]));
    assert_eq!(body["obligation"]["met"], true);
    assert_eq!(body["obligation"]["credit_minor"], 0);

    // No evidence at all: no attainment and no obligation, labeled estimate.
    let id3 = contracted(&w, &name()).await;
    let uri = format!(
        "/v1/objectives/{id3}/sla-review?from={}&to={}",
        ts(10_800),
        ts(7_200)
    );
    let (_, body) = w.call("viewer", "GET", &uri, None).await;
    assert_eq!(
        (&body["basis"], &body["attainment"], &body["obligation"]),
        (&json!("estimate"), &Value::Null, &Value::Null)
    );
}

#[tokio::test]
async fn contract_terms_and_reviews_are_guarded() {
    let w = World::start().await;
    let id = w.objective(w.service(&name()), "production", true).await;
    let put = |who: &'static str, body: Value| {
        let uri = format!("/v1/objectives/{id}/contract");
        let w = &w;
        async move { w.call(who, "PUT", &uri, Some(body)).await.0 }
    };
    assert_eq!(put("member", terms()).await, 403);
    assert_eq!(put("admin-key", terms()).await, 403);
    assert_eq!(put("other", terms()).await, 404);
    for bad in [
        json!({"contracted_target": 1.0}),
        json!({"currency": "eur"}),
        json!({"period_fee_minor": -1}),
        json!({"credit_tiers": []}),
        json!({"credit_tiers": [{"below": 0.99, "credit_percent": 10.0},
                                {"below": 0.95, "credit_percent": 25.0}]}),
        json!({"credit_tiers": [{"below": 0.999, "credit_percent": 10.0}]}),
        json!({"credit_tiers": [{"below": 0.99, "credit_percent": 0.0}]}),
    ] {
        let mut body = terms();
        body.as_object_mut()
            .unwrap()
            .extend(bad.as_object().unwrap().clone());
        assert_eq!(put("admin", body).await, 400, "{bad}");
    }
    let review = |id: Uuid, who: &'static str, from: String, to: String| {
        let uri = format!("/v1/objectives/{id}/sla-review?from={from}&to={to}");
        let w = &w;
        async move { w.call(who, "GET", &uri, None).await }
    };
    // No terms recorded yet.
    assert_eq!(review(id, "viewer", ts(7200), ts(3600)).await.0, 404);
    assert_eq!(put("admin", terms()).await, 200);
    assert_eq!(review(id, "viewer", ts(3600), ts(7200)).await.0, 400);
    assert_eq!(review(id, "viewer", ts(-60), ts(-30)).await.0, 400);
    assert_eq!(review(id, "viewer", ts(100 * 86_400), ts(0)).await.0, 400);
    assert_eq!(review(id, "other", ts(7200), ts(3600)).await.0, 404);
    let unapproved = w.objective(w.service(&name()), "production", false).await;
    assert_eq!(
        review(unapproved, "viewer", ts(7200), ts(3600)).await.0,
        409
    );
}
