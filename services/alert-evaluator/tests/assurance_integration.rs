//! W5a (docs/assurance-relocations.md) through the routes as the scoped `observable_alerting`
//! login: similar incidents with confirmed causes (OBS-09-07), the delivery and reliability
//! review (OBS-11-05) and the cost and outcomes review (OBS-17-06). Control's deployment feed
//! and cost totals come from the guidance substitute; reliability evidence is real ClickHouse.

use std::sync::{Arc, Mutex};

use alert_evaluator::guidance::{FeedDeployment, GuidanceSources, StaticSources};
use alert_evaluator::middleware::auth::TenantContext;
use alert_evaluator::objectives::{self, CatalogRecord, ObjectiveCatalog, ScopedBinding};
use alert_evaluator::{AppState, assurance, observability};
use axum::{
    Extension, Router,
    body::Body,
    http::Request,
    routing::{get, post, put},
};
use chrono::{DateTime, Duration, TimeZone, Utc};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

struct World {
    state: AppState,
    owner: PgPool,
    catalog: ObjectiveCatalog,
    sources: GuidanceSources,
    feeds: Arc<Mutex<StaticSources>>,
    tenant: Uuid,
    admin: Uuid,
    member: Uuid,
    _db: test_support::postgres::TestDatabase,
}

impl World {
    async fn start() -> Self {
        let (owner, guard) = test_support::postgres::isolated_pool().await;
        let db = test_support::postgres::scoped_pool(&owner, "observable_alerting").await;
        let ch = test_support::clickhouse::shared_client().await;
        let (sources, feeds) = GuidanceSources::substitute();
        Self {
            state: AppState {
                db,
                ch,
                auth_service_url: "http://unused".into(),
                http_client: reqwest::Client::new(),
                metrics: Arc::new(observability::AlertEvaluatorMetrics::new()),
                producer: None,
            },
            owner,
            catalog: ObjectiveCatalog::substitute(),
            sources,
            feeds,
            tenant: Uuid::new_v4(),
            admin: Uuid::new_v4(),
            member: Uuid::new_v4(),
            _db: guard,
        }
    }

    fn ctx(&self, who: &str) -> TenantContext {
        let (tenant_id, user_id, role) = match who {
            "admin" => (self.tenant, Some(self.admin), "tenant_admin"),
            "member" => (self.tenant, Some(self.member), "member"),
            "stranger" => (self.tenant, Some(Uuid::new_v4()), "member"),
            "admin-key" => (self.tenant, None, "tenant_admin"),
            "other" => (Uuid::new_v4(), Some(Uuid::new_v4()), "tenant_admin"),
            _ => unreachable!(),
        };
        TenantContext {
            tenant_id,
            user_id,
            role: role.into(),
        }
    }

    async fn call(&self, who: &str, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
        let app = Router::new()
            .route("/v1/objectives", post(objectives::handle_create))
            .route(
                "/v1/objectives/{id}/approve",
                post(objectives::handle_approve),
            )
            .route(
                "/v1/incidents/{incident_id}/cause",
                put(assurance::handle_put_cause),
            )
            .route(
                "/v1/incidents/{incident_id}/similar",
                get(assurance::handle_similar),
            )
            .route(
                "/v1/objectives/review-measures",
                put(assurance::handle_put_measures),
            )
            .route(
                "/v1/objectives/benefits-review",
                get(assurance::handle_benefits_review),
            )
            .route(
                "/v1/objectives/{id}/delivery-review",
                get(assurance::handle_delivery_review),
            )
            .layer(Extension(self.ctx(who)))
            .layer(Extension(self.catalog.clone()))
            .layer(Extension(self.sources.clone()))
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

    async fn rule(&self, tenant: Uuid) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO alerting.alert_rules (rule_id, tenant_id, name, alert_type, severity, condition) \
             VALUES ($1, $2, 'Checkout errors', 'threshold', 'critical', '{}')",
        )
        .bind(id)
        .bind(tenant)
        .execute(&self.owner)
        .await
        .expect("rule");
        id
    }

    #[allow(clippy::too_many_arguments)]
    async fn incident(
        &self,
        tenant: Uuid,
        rule: Option<Uuid>,
        severity: &str,
        services: &[&str],
        status: &str,
        triggered_at: DateTime<Utc>,
        resolved_after_min: Option<i64>,
        owner: Option<String>,
    ) -> Uuid {
        let id = Uuid::new_v4();
        let services: Vec<String> = services.iter().map(|s| s.to_string()).collect();
        sqlx::query(
            "INSERT INTO alerting.incidents (incident_id, tenant_id, title, severity, status, dedup_key, \
             triggered_by_rule_id, triggered_at, resolved_at, affected_services, owner) \
             VALUES ($1, $2, 'Checkout failing', $3, $4, $1::text, $5, $6, $7, $8, $9)",
        )
        .bind(id)
        .bind(tenant)
        .bind(severity)
        .bind(status)
        .bind(rule)
        .bind(triggered_at)
        .bind(resolved_after_min.map(|m| triggered_at + Duration::minutes(m)))
        .bind(services)
        .bind(owner)
        .execute(&self.owner)
        .await
        .expect("incident");
        id
    }
}

#[tokio::test]
async fn similar_incidents_share_a_signal_and_rank_confirmed_causes_first() {
    let w = World::start().await;
    let now = Utc::now();
    let rule = w.rule(w.tenant).await;
    let other_rule = w.rule(w.tenant).await;
    let member_owner = Some(format!("user:{}", w.member));
    let t = &w.tenant;
    let target = w
        .incident(
            *t,
            Some(rule),
            "critical",
            &["checkout"],
            "triggered",
            now,
            None,
            None,
        )
        .await;
    let same_rule = w
        .incident(
            *t,
            Some(rule),
            "warning",
            &["search"],
            "resolved",
            now - Duration::days(3),
            Some(30),
            None,
        )
        .await;
    let shared_confirmed = w
        .incident(
            *t,
            Some(other_rule),
            "critical",
            &["checkout", "db"],
            "resolved",
            now - Duration::days(2),
            Some(45),
            member_owner.clone(),
        )
        .await;
    let both = w
        .incident(
            *t,
            Some(rule),
            "critical",
            &["checkout"],
            "resolved",
            now - Duration::days(1),
            Some(20),
            None,
        )
        .await;
    // No shared signal: other rule, other severity on the same service.
    let unrelated = w
        .incident(
            *t,
            Some(other_rule),
            "warning",
            &["checkout"],
            "resolved",
            now - Duration::days(1),
            Some(5),
            None,
        )
        .await;
    // Another tenant with the same symptoms, and a later incident, are never listed.
    let foreign = w
        .incident(
            Uuid::new_v4(),
            Some(rule),
            "critical",
            &["checkout"],
            "resolved",
            now - Duration::days(1),
            Some(5),
            None,
        )
        .await;
    let later = w
        .incident(
            *t,
            Some(rule),
            "critical",
            &["checkout"],
            "triggered",
            now + Duration::minutes(1),
            None,
            None,
        )
        .await;

    // Recording a confirmed cause: owner or tenant admin, as a person, on a resolved incident.
    let cause = json!({"confirmed_cause": "connection pool exhausted after db failover",
                       "cause_category": "capacity"});
    let uri = |id: Uuid| format!("/v1/incidents/{id}/cause");
    assert_eq!(
        w.call(
            "stranger",
            "PUT",
            &uri(shared_confirmed),
            Some(cause.clone())
        )
        .await
        .0,
        403
    );
    assert_eq!(
        w.call(
            "admin-key",
            "PUT",
            &uri(shared_confirmed),
            Some(cause.clone())
        )
        .await
        .0,
        403
    );
    assert_eq!(
        w.call("admin", "PUT", &uri(target), Some(cause.clone()))
            .await
            .0,
        409
    );
    assert_eq!(
        w.call(
            "member",
            "PUT",
            &uri(shared_confirmed),
            Some(json!({"confirmed_cause": " "}))
        )
        .await
        .0,
        400
    );
    let (status, body) = w
        .call("member", "PUT", &uri(shared_confirmed), Some(cause))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["cause_confirmed_by"], json!(w.member));
    let (by, at): (Option<Uuid>, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT cause_confirmed_by, cause_confirmed_at FROM alerting.incidents WHERE incident_id = $1",
    )
    .bind(shared_confirmed)
    .fetch_one(&w.owner)
    .await
    .unwrap();
    assert_eq!(by, Some(w.member));
    assert!(at.is_some(), "who confirmed it and when are recorded");

    let (status, body) = w
        .call(
            "stranger",
            "GET",
            &format!("/v1/incidents/{target}/similar"),
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = body["items"].as_array().unwrap();
    let ids: Vec<Uuid> = items
        .iter()
        .map(|i| i["incident_id"].as_str().unwrap().parse().unwrap())
        .collect();
    assert_eq!(ids, vec![shared_confirmed, both, same_rule], "{body}");
    for excluded in [unrelated, foreign, later, target] {
        assert!(!ids.contains(&excluded));
    }
    assert_eq!(
        items[0]["confirmed_cause"],
        json!("connection pool exhausted after db failover")
    );
    assert_eq!(items[0]["cause_category"], json!("capacity"));
    assert_eq!(items[0]["matched"][0]["signal"], json!("shared_services"));
    assert_eq!(items[0]["matched"][0]["services"], json!(["checkout"]));
    let signals: Vec<&str> = items[1]["matched"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["signal"].as_str().unwrap())
        .collect();
    assert_eq!(signals, ["same_rule", "shared_services"]);
    assert_eq!(items[2]["matched"][0]["signal"], json!("same_rule"));
    assert!(items[2]["confirmed_cause"].is_null());

    let limited = w
        .call(
            "member",
            "GET",
            &format!("/v1/incidents/{target}/similar?limit=1"),
            None,
        )
        .await
        .1;
    assert_eq!(limited["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        w.call(
            "other",
            "GET",
            &format!("/v1/incidents/{target}/similar"),
            None
        )
        .await
        .0,
        404
    );
}

#[tokio::test]
async fn delivery_review_reports_control_and_alerting_measures_separately() {
    let w = World::start().await;
    let name = format!("svc-{}", Uuid::new_v4().simple());
    let service = Uuid::new_v4();
    let b = |env: &str, class: Option<&'static str>| ScopedBinding {
        service_name: name.clone(),
        environment: env.into(),
        class,
    };
    w.catalog.put(
        w.tenant,
        service,
        CatalogRecord {
            lifecycle: "live".into(),
            bindings: vec![
                b("prod", Some("production")),
                b("staging", Some("non_production")),
            ],
        },
    );
    // 1000 production requests in the period, 10 failed: attainment 0.99.
    let at = (Utc::now() - Duration::seconds(30))
        .timestamp_nanos_opt()
        .unwrap();
    w.state
        .ch
        .query(
            "INSERT INTO observable.spans (tenant_id, trace_id, span_id, service_name, operation_name, \
             start_time_unix_nano, end_time_unix_nano, duration_ns, environment, status_code) \
             SELECT ?, toString(generateUUIDv4()), toString(number), ?, 'GET /', ?, ?, 1, 'prod', \
                    if(number < 10, 'ERROR', 'OK') FROM numbers(1000)",
        )
        .bind(w.tenant)
        .bind(&name)
        .bind(at)
        .bind(at)
        .execute()
        .await
        .unwrap();
    let (status, body) = w
        .call(
            "member",
            "POST",
            "/v1/objectives",
            Some(
                json!({"service_id": service, "environment_class": "production",
                        "target": 0.98, "window_days": 7}),
            ),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let objective = body["objective_id"].as_str().unwrap().to_string();
    assert_eq!(
        w.call(
            "admin",
            "POST",
            &format!("/v1/objectives/{objective}/approve"),
            None
        )
        .await
        .0,
        200
    );

    let now = Utc::now();
    let dep = |minutes: i64, env: &str, svc: &str, status: &str| FeedDeployment {
        deployment_id: Uuid::new_v4(),
        service_name: svc.into(),
        environment: env.into(),
        service_version: format!("v{minutes}"),
        status: status.into(),
        started_at: now - Duration::minutes(minutes),
    };
    w.feeds.lock().unwrap().deployments.insert(
        w.tenant,
        vec![
            dep(50, "prod", &name, "success"),
            dep(40, "prod", &name, "failed"),
            dep(30, "prod", &name, "success"),
            dep(20, "prod", &name, "rolled_back"),
            dep(5, "prod", &name, "success"),
            // Outside the objective's scope or period: not counted.
            dep(45, "staging", &name, "failed"),
            dep(45, "prod", "other-service", "failed"),
            dep(600, "prod", &name, "failed"),
        ],
    );
    let period = format!(
        "from={}&to={}",
        (now - Duration::hours(1)).format("%Y-%m-%dT%H:%M:%SZ"),
        (now + Duration::minutes(1)).format("%Y-%m-%dT%H:%M:%SZ")
    );
    let uri = format!("/v1/objectives/{objective}/delivery-review?{period}");

    // Nothing is shown until a tenant admin approves the measures for the review.
    assert_eq!(w.call("member", "GET", &uri, None).await.0, 409);
    let approve = |m: Value| Some(json!({"review": "delivery", "measures": m}));
    assert_eq!(
        w.call(
            "member",
            "PUT",
            "/v1/objectives/review-measures",
            approve(json!(["deployment_count"]))
        )
        .await
        .0,
        403
    );
    assert_eq!(
        w.call(
            "admin",
            "PUT",
            "/v1/objectives/review-measures",
            approve(json!(["delivery_index"]))
        )
        .await
        .0,
        400
    );
    let approved = json!([
        "deployment_count",
        "change_failure_rate",
        "median_restore_minutes",
        "attainment"
    ]);
    let (status, body) = w
        .call(
            "admin",
            "PUT",
            "/v1/objectives/review-measures",
            approve(approved),
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = w.call("member", "GET", &uri, None).await;
    assert_eq!(status, 200, "{body}");
    let section = |key: &str| -> Vec<(String, Value, String)> {
        body[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                (
                    m["measure"].as_str().unwrap().to_string(),
                    m["value"].clone(),
                    m["source"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    };
    let delivery = section("delivery");
    assert_eq!(
        delivery,
        vec![
            ("deployment_count".into(), json!(5), "control".into()),
            ("change_failure_rate".into(), json!(0.4), "control".into()),
            // Restores after 10 and 15 minutes.
            ("median_restore_minutes".into(), json!(15), "control".into()),
        ]
    );
    let reliability = section("reliability");
    assert_eq!(
        reliability.len(),
        1,
        "error_budget_remaining was not approved: {body}"
    );
    assert_eq!(reliability[0].0, "attainment");
    assert_eq!(reliability[0].2, "alerting");
    assert!((reliability[0].1.as_f64().unwrap() - 0.99).abs() < 1e-9);

    // Negative: no field combines a delivery measure with a reliability measure.
    let mut keys: Vec<&str> = body
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "delivery",
            "note",
            "objective_id",
            "period",
            "reliability",
            "target"
        ]
    );

    // With no deployments in the period, a rate has no basis and is insufficient data.
    *w.feeds.lock().unwrap() = StaticSources::default();
    let (status, body) = w.call("member", "GET", &uri, None).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["delivery"][0]["value"], json!(0));
    assert_eq!(body["delivery"][1]["basis"], json!("insufficient_data"));
}

#[tokio::test]
async fn benefits_review_shows_cost_change_beside_outcomes_without_a_ratio() {
    let w = World::start().await;
    let sept = Utc.with_ymd_and_hms(2026, 9, 10, 12, 0, 0).unwrap();
    let oct = Utc.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap();
    let t = w.tenant;
    for (at, severity, mins) in [
        (sept, "critical", 60),
        (sept, "warning", 60),
        (sept, "warning", 60),
        (oct, "warning", 30),
    ] {
        w.incident(
            t,
            None,
            severity,
            &["checkout"],
            "resolved",
            at,
            Some(mins),
            None,
        )
        .await;
    }
    // Another tenant's incidents never count.
    w.incident(
        Uuid::new_v4(),
        None,
        "critical",
        &["checkout"],
        "resolved",
        oct,
        Some(5),
        None,
    )
    .await;
    {
        let mut f = w.feeds.lock().unwrap();
        f.costs.insert((t, "2026-09".into()), 10_000);
        f.costs.insert((t, "2026-10".into()), 12_500);
    }
    let uri = "/v1/objectives/benefits-review?baseline=2026-09&period=2026-10";
    assert_eq!(w.call("member", "GET", uri, None).await.0, 409);
    let (status, _) = w
        .call(
            "admin",
            "PUT",
            "/v1/objectives/review-measures",
            Some(json!({"review": "benefits",
                        "measures": ["incident_count", "mean_time_to_resolve_minutes"]})),
        )
        .await;
    assert_eq!(status, 200);

    let (status, body) = w.call("member", "GET", uri, None).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["cost"],
        json!({"baseline_cents": 10_000, "period_cents": 12_500, "change_cents": 2_500,
               "source": "control", "basis": "recorded"})
    );
    let indicators = body["outcome_indicators"].as_array().unwrap();
    assert_eq!(indicators.len(), 2, "only approved indicators: {body}");
    assert_eq!(indicators[0]["indicator"], json!("incident_count"));
    assert_eq!(
        (&indicators[0]["baseline"], &indicators[0]["period"]),
        (&json!(3), &json!(1))
    );
    assert_eq!(indicators[0]["change"], json!(-2.0));
    assert_eq!(indicators[0]["source"], json!("alerting"));
    assert_eq!(
        indicators[1]["indicator"],
        json!("mean_time_to_resolve_minutes")
    );
    assert!((indicators[1]["baseline"].as_f64().unwrap() - 60.0).abs() < 1e-6);
    assert!((indicators[1]["period"].as_f64().unwrap() - 30.0).abs() < 1e-6);
    // Negative: no value divides or combines cost with an outcome.
    let mut keys: Vec<&str> = body
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["baseline", "cost", "note", "outcome_indicators", "period"]
    );

    // An unrecorded period is insufficient data, never zero cost.
    let (_, body) = w
        .call(
            "member",
            "GET",
            "/v1/objectives/benefits-review?baseline=2026-08&period=2026-10",
            None,
        )
        .await;
    assert_eq!(body["cost"]["baseline_cents"], Value::Null);
    assert_eq!(body["cost"]["change_cents"], Value::Null);
    assert_eq!(body["cost"]["basis"], json!("insufficient_data"));
    assert_eq!(
        w.call(
            "member",
            "GET",
            "/v1/objectives/benefits-review?baseline=2026-10&period=2026-09",
            None
        )
        .await
        .0,
        400
    );
}
