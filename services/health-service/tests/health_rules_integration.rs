//! Threshold conditions and tenant rule revisions (phase 2 E1) through the router with real
//! PostgreSQL, the scoped `observable_health` login, and the evaluator against real ClickHouse.

mod common;

use common::{Harness, inputs_for, ns_ago, view};
use health_service::app;
use health_service::assessment::{current_rule, evaluate, persist};
use health_service::ports::{
    EnvironmentClass, Evidence, EvidenceTimes, RequestSample, TelemetryIdentity,
};
use observable_auth::authz::MockScenario;
use serde_json::{Value, json};
use uuid::Uuid;

fn identity(name: &str, env: &str) -> TelemetryIdentity {
    TelemetryIdentity {
        service_name: name.into(),
        environment: env.into(),
    }
}

/// Presence of any signal, a 5 % error ratio (degraded), every request failing (unavailable)
/// and an optional p95 latency above 500 ms (degraded).
fn threshold_rule() -> Value {
    json!({
        "freshness_seconds": 300,
        "lookback_seconds": 3600,
        "conditions": [
            {"signal": "any", "classification": "internal"},
            {"kind": "error_ratio", "signal": "traces", "classification": "internal",
             "threshold": 0.05, "window_seconds": 300, "min_requests": 10, "yields": "degraded"},
            {"kind": "error_ratio", "signal": "traces", "classification": "internal",
             "threshold": 1.0, "window_seconds": 300, "min_requests": 10, "yields": "unavailable"},
            {"kind": "latency_quantile", "signal": "traces", "classification": "internal",
             "quantile": 0.95, "threshold": 500.0, "window_seconds": 300, "min_requests": 10,
             "required": false},
        ],
    })
}

async fn publish(h: &Harness, rule: &Value) -> Value {
    let (status, body, raw) = h
        .send("PUT", "/v1/health/rules", "admin", &rule.to_string())
        .await;
    assert_eq!(status, 201, "{raw}");
    body
}

/// `n` requests in the last minute, `errors` of them failed, each lasting `ms`.
fn requests(n: usize, errors: usize, ms: u64) -> Vec<RequestSample> {
    (0..n)
        .map(|i| RequestSample {
            at_ns: ns_ago(5 + (i as i64 % 50)),
            duration_ns: ms * 1_000_000,
            error: i < errors,
        })
        .collect()
}

/// A live service bound to `name`, with current production log evidence.
fn service(h: &Harness, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    h.services.put(h.tenant, view(id, "live", "internal"));
    h.inputs.set(h.tenant, vec![inputs_for(id, name, "live")]);
    h.evidence.record(
        h.tenant,
        &identity(name, "prod"),
        EvidenceTimes {
            logs: Some(ns_ago(5)),
            ..Default::default()
        },
    );
    id
}

async fn state_of(h: &Harness, id: Uuid) -> (Value, Value, Value) {
    let (status, body, raw) = h.health(id, "production", "member").await;
    assert_eq!(status, 200, "{raw}");
    (
        body["state"].clone(),
        body["evidence_status"].clone(),
        body["conditions"].clone(),
    )
}

#[tokio::test]
async fn error_ratio_and_latency_breaches_yield_degraded_and_total_failure_unavailable() {
    let h = Harness::start().await;
    publish(&h, &threshold_rule()).await;
    let id = service(&h, "payments");
    let prod = identity("payments", "prod");

    h.evidence
        .record_requests(h.tenant, &prod, requests(100, 0, 20));
    h.evaluate().await;
    let (state, status, conditions) = state_of(&h, id).await;
    assert_eq!((state, status), (json!("normal"), json!("current")));
    assert_eq!(conditions[1]["outcome"], json!("ok"));
    assert_eq!(conditions[1]["requests"], json!(100));

    // Ten percent failing: degraded, not unavailable.
    h.evidence
        .record_requests(h.tenant, &prod, requests(100, 10, 20));
    h.evaluate().await;
    let (state, _, conditions) = state_of(&h, id).await;
    assert_eq!(state, json!("degraded"));
    assert_eq!(conditions[1]["outcome"], json!("breached"));
    assert_eq!(conditions[1]["value"], json!(0.1));
    assert_eq!(conditions[2]["outcome"], json!("ok"));

    // Slow but succeeding: degraded only.
    h.evidence
        .record_requests(h.tenant, &prod, requests(100, 0, 900));
    h.evaluate().await;
    let (state, _, conditions) = state_of(&h, id).await;
    assert_eq!(state, json!("degraded"));
    assert_eq!(conditions[3]["outcome"], json!("breached"));
    assert_eq!(conditions[3]["value"], json!(900.0));

    // Every request failing: unavailable.
    h.evidence
        .record_requests(h.tenant, &prod, requests(100, 100, 20));
    h.evaluate().await;
    assert_eq!(state_of(&h, id).await.0, json!("unavailable"));

    // Requests outside the window do not count.
    h.evidence.record_requests(
        h.tenant,
        &prod,
        (0..100)
            .map(|_| RequestSample {
                at_ns: ns_ago(600),
                duration_ns: 1,
                error: true,
            })
            .collect(),
    );
    h.evaluate().await;
    let (state, status, _) = state_of(&h, id).await;
    assert_eq!((state, status), (json!("unknown"), json!("insufficient")));
}

#[tokio::test]
async fn no_data_is_unknown_never_unavailable() {
    let h = Harness::start().await;
    publish(&h, &threshold_rule()).await;

    // Presence current, but no requests at all.
    let quiet = service(&h, "quiet");
    h.evaluate().await;
    let (state, status, conditions) = state_of(&h, quiet).await;
    assert_eq!((state, status), (json!("unknown"), json!("insufficient")));
    assert_eq!(conditions[2]["outcome"], json!("insufficient"));
    assert_eq!(conditions[2]["value"], json!(null));

    // Fewer failing requests than min_requests: no conclusion.
    h.evidence
        .record_requests(h.tenant, &identity("quiet", "prod"), requests(9, 9, 20));
    h.evaluate().await;
    assert_eq!(state_of(&h, quiet).await.0, json!("unknown"));

    // No evidence of any kind.
    let id = Uuid::new_v4();
    h.services.put(h.tenant, view(id, "live", "internal"));
    h.inputs
        .set(h.tenant, vec![inputs_for(id, "silent", "live")]);
    h.evaluate().await;
    let (state, status, _) = state_of(&h, id).await;
    assert_eq!((state, status), (json!("unknown"), json!("missing")));
}

#[tokio::test]
async fn a_breach_stops_counting_once_its_evidence_is_stale() {
    let h = Harness::start().await;
    publish(&h, &threshold_rule()).await;
    let id = service(&h, "payments");
    let prod = identity("payments", "prod");
    // Every request failed an hour ago, and the evaluator has not run since.
    let then = chrono::Utc::now() - chrono::Duration::hours(1);
    h.evidence.record_requests(
        h.tenant,
        &prod,
        (0..50)
            .map(|_| RequestSample {
                at_ns: ns_ago(3600 + 10),
                duration_ns: 1,
                error: true,
            })
            .collect(),
    );
    h.evidence.record(
        h.tenant,
        &prod,
        EvidenceTimes {
            traces: Some(ns_ago(3600 + 10)),
            ..Default::default()
        },
    );
    let rule = current_rule(&h.state.db, h.tenant).await.unwrap().unwrap();
    let snapshot = evaluate(
        &h.evidence,
        h.tenant,
        id,
        EnvironmentClass::Production,
        vec![prod],
        &rule,
        then,
    )
    .await
    .unwrap();
    assert_eq!(snapshot.freshness(then).0, "unavailable");
    persist(&h.state.db, &snapshot).await.unwrap();
    let (state, status, _) = state_of(&h, id).await;
    assert_eq!((state, status), (json!("unknown"), json!("stale")));
}

#[tokio::test]
async fn only_tenant_admins_publish_rules_and_each_publish_is_a_new_revision() {
    let h = Harness::start().await;
    let rule = threshold_rule().to_string();
    for token in ["viewer", "member"] {
        let (status, body, _) = h.send("PUT", "/v1/health/rules", token, &rule).await;
        assert_eq!(
            (status, body),
            (403, json!({"error": "forbidden"})),
            "{token}"
        );
    }
    let (status, body, _) = h.send("PUT", "/v1/health/rules", "admin-key", &rule).await;
    assert_eq!(
        (status, body),
        (403, json!({"error": "user_session_required"}))
    );
    let (status, _, _) = h.send("PUT", "/v1/health/rules", "forged", &rule).await;
    assert_eq!(status, 401);

    // Before any publish, every member reads the platform default.
    let (status, body, _) = h.get("/v1/health/rules", Some("viewer")).await;
    assert_eq!(status, 200);
    assert_eq!(
        (&body["source"], &body["rule_id"]),
        (&json!("platform"), &json!("platform-default"))
    );

    let first = publish(&h, &threshold_rule()).await;
    assert_eq!(first["revision"], json!(1));
    assert_eq!(first["source"], json!("tenant"));
    assert_eq!(first["rule_id"], json!(format!("tenant-{}", h.tenant)));
    assert!(first["published_by"].is_string());
    let second = publish(&h, &threshold_rule()).await;
    assert_eq!(second["revision"], json!(2));
    let (_, body, _) = h.get("/v1/health/rules", Some("member")).await;
    assert_eq!(body["revision"], json!(2));
    assert_eq!(body["conditions"], threshold_rule()["conditions"]);

    // Another tenant still reads the platform default.
    let (_, body, _) = h.get("/v1/health/rules", Some("other-member")).await;
    assert_eq!(body["source"], json!("platform"));

    // Concurrent publishes all land with distinct revisions.
    let mut tasks = vec![];
    for _ in 0..4 {
        let app = h.app.clone();
        let body = rule.clone();
        tasks.push(tokio::spawn(async move {
            use tower::ServiceExt;
            let req = axum::http::Request::put("/v1/health/rules")
                .header("Authorization", "Bearer admin")
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(body))
                .unwrap();
            app.oneshot(req).await.unwrap().status().as_u16()
        }));
    }
    for t in tasks {
        assert_eq!(t.await.unwrap(), 201);
    }
    let revisions: Vec<i32> = sqlx::query_scalar(
        "SELECT revision FROM health.rules WHERE tenant_id = $1 ORDER BY revision",
    )
    .bind(h.tenant)
    .fetch_all(&h.state.db)
    .await
    .unwrap();
    assert_eq!(revisions, (1..=6).collect::<Vec<_>>());

    // Policy outage: no read or publish.
    h.decider.set_scenario(MockScenario::Unavailable);
    assert_eq!(h.get("/v1/health/rules", Some("member")).await.0, 503);
    assert_eq!(
        h.send("PUT", "/v1/health/rules", "admin", &rule).await.0,
        503
    );
}

#[tokio::test]
async fn invalid_rules_are_refused_with_a_reason() {
    let h = Harness::start().await;
    let base = threshold_rule();
    let with = |f: &dyn Fn(&mut Value)| {
        let mut r = base.clone();
        f(&mut r);
        r.to_string()
    };
    for (case, body) in [
        (
            "unavailable below every request failing",
            with(&|r| r["conditions"][2]["threshold"] = json!(0.5)),
        ),
        (
            "latency yields unavailable",
            with(&|r| r["conditions"][3]["yields"] = json!("unavailable")),
        ),
        (
            "no presence condition",
            with(&|r| {
                r["conditions"].as_array_mut().unwrap().remove(0);
            }),
        ),
        (
            "window too short",
            with(&|r| r["conditions"][1]["window_seconds"] = json!(30)),
        ),
        (
            "window beyond lookback",
            with(&|r| r["conditions"][1]["window_seconds"] = json!(7200)),
        ),
        (
            "zero min_requests",
            with(&|r| r["conditions"][1]["min_requests"] = json!(0)),
        ),
        (
            "unknown field",
            with(&|r| r["conditions"][1]["bogus"] = json!(true)),
        ),
        (
            "unknown kind",
            with(&|r| r["conditions"][1]["kind"] = json!("request_absence")),
        ),
        (
            "lookback below freshness",
            with(&|r| r["lookback_seconds"] = json!(60)),
        ),
        ("not json", "{".to_owned()),
    ] {
        let (status, body, raw) = h.send("PUT", "/v1/health/rules", "admin", &body).await;
        assert_eq!(
            (status, &body["error"]),
            (400, &json!("invalid_request")),
            "{case}: {raw}"
        );
        assert!(body["message"].is_string(), "{case}");
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM health.rules WHERE tenant_id = $1")
        .bind(h.tenant)
        .fetch_one(&h.state.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn the_scoped_login_publishes_revisions_but_cannot_rewrite_them() {
    let h = Harness::start().await;
    let scoped = test_support::postgres::scoped_pool(&h.state.db, "observable_health").await;
    let mut state = h.state.clone();
    state.db = scoped.clone();
    let scoped_app = app(state);
    let req = axum::http::Request::put("/v1/health/rules")
        .header("Authorization", "Bearer admin")
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(threshold_rule().to_string()))
        .unwrap();
    use tower::ServiceExt;
    assert_eq!(scoped_app.oneshot(req).await.unwrap().status(), 201);
    assert!(current_rule(&scoped, h.tenant).await.unwrap().is_some());

    let rewrite = sqlx::query("UPDATE health.rules SET freshness_seconds = 1 WHERE tenant_id = $1")
        .bind(h.tenant)
        .execute(&scoped)
        .await;
    assert!(rewrite.is_err(), "published revisions are immutable");
    // A tenant row must name its publisher and its own rule id.
    for sql in [
        "INSERT INTO health.rules (rule_id, revision, tenant_id, freshness_seconds, \
         lookback_seconds, conditions) VALUES ('tenant-' || $1::text, 99, $1, 60, 300, '[{}]')",
        "INSERT INTO health.rules (rule_id, revision, tenant_id, freshness_seconds, \
         lookback_seconds, conditions, published_by) VALUES ('platform-default', 99, $1, 60, 300, \
         '[{}]', gen_random_uuid())",
    ] {
        assert!(
            sqlx::query(sql)
                .bind(h.tenant)
                .execute(&scoped)
                .await
                .is_err(),
            "{sql}"
        );
    }
}

#[tokio::test]
async fn clickhouse_window_counts_only_server_spans_of_the_tenant_and_scope() {
    let h = Harness::start().await;
    let ch = test_support::clickhouse::shared_client().await;
    let name = format!("svc-{}", Uuid::new_v4().simple());
    let tenant = h.tenant;
    let other = h.other_tenant;
    let mut rows = vec![];
    for i in 0..20 {
        let at = ns_ago(10 + i);
        // 4 of 20 server requests fail (20 %); durations 10..=200 ms.
        let status = if i < 4 { "ERROR" } else { "OK" };
        let ms = (i as u64 + 1) * 10;
        rows.push(format!(
            "('{tenant}', 't{i}', 's{i}', '{name}', 'GET /', 'SERVER', {at}, {at}, {}, '{status}', 'prod')",
            ms * 1_000_000
        ));
    }
    let at = ns_ago(15);
    for (j, (t, kind, env, when)) in [
        // Failing client calls, another tenant, another environment and an old window.
        (tenant, "CLIENT", "prod", at),
        (other, "SERVER", "prod", at),
        (tenant, "SERVER", "staging", at),
        (tenant, "SERVER", "prod", ns_ago(900)),
    ]
    .into_iter()
    .enumerate()
    {
        for k in 0..20 {
            rows.push(format!(
                "('{t}', 'x{j}-{k}', 'y{j}-{k}', '{name}', 'GET /', '{kind}', {when}, {when}, 1, 'ERROR', '{env}')"
            ));
        }
    }
    ch.query(&format!(
        "INSERT INTO observable.spans (tenant_id, trace_id, span_id, service_name, operation_name, \
         span_kind, start_time_unix_nano, end_time_unix_nano, duration_ns, status_code, environment) \
         VALUES {}",
        rows.join(", ")
    ))
    .execute()
    .await
    .unwrap();

    let evidence = Evidence::clickhouse(ch);
    let stats = evidence
        .window_stats(
            tenant,
            &[identity(&name, "prod"), identity(&name, "")],
            ns_ago(300),
            ns_ago(0),
            0.95,
        )
        .await
        .unwrap();
    assert_eq!((stats.requests, stats.errors), (20, 4));
    // quantileExact(0.95) over 20 values picks index 19: 200 ms.
    assert_eq!(stats.quantile_ns, Some(200_000_000.0));
    assert_eq!(stats.last_seen, Some(ns_ago_exact(&rows[0])));
    let none = evidence
        .window_stats(
            tenant,
            &[identity("absent", "prod")],
            ns_ago(300),
            ns_ago(0),
            0.5,
        )
        .await
        .unwrap();
    assert_eq!(
        (none.requests, none.quantile_ns, none.last_seen),
        (0, None, None)
    );

    // End to end: 20 % failing over real ClickHouse is degraded.
    publish(&h, &threshold_rule()).await;
    let id = Uuid::new_v4();
    h.services.put(h.tenant, view(id, "live", "internal"));
    h.inputs.set(h.tenant, vec![inputs_for(id, &name, "live")]);
    let summary = health_service::assessment::evaluate_all(&h.state.db, &h.inputs, &evidence).await;
    assert_eq!(summary.failed, 0);
    let (state, status, conditions) = state_of(&h, id).await;
    assert_eq!((state, status), (json!("degraded"), json!("current")));
    assert_eq!(conditions[1]["value"], json!(0.2));
    assert_eq!(conditions[2]["outcome"], json!("ok"));
    // Staging has only failing requests, so non-production is unavailable.
    let (_, staging, _) = h.health(id, "non_production", "member").await;
    assert_eq!(staging["state"], json!("unavailable"));
}

/// Start time of the first inserted row (the newest server span).
fn ns_ago_exact(row: &str) -> u64 {
    row.split(", ").nth(6).unwrap().parse().unwrap()
}
