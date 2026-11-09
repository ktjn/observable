//! Evaluation and scope-bound snapshots (phase 1 C2.3) through the actual router with real
//! PostgreSQL, and the evaluator against real ClickHouse.

mod common;

use std::sync::Arc;

use chrono::{Duration, Utc};
use common::{Harness, inputs_for, ns_ago, view};
use health_service::assessment::{
    ConditionEvidence, Snapshot, current_rule, evaluate, load, persist, scope_key,
};
use health_service::ports::{
    EnvironmentClass, Evidence, EvidenceTimes, ServiceReadError, ServiceReads, TelemetryIdentity,
};
use serde_json::json;
use uuid::Uuid;

fn identity(name: &str, env: &str) -> TelemetryIdentity {
    TelemetryIdentity {
        service_name: name.into(),
        environment: env.into(),
    }
}

/// A live, evaluated service with fresh production traces.
async fn evaluated(h: &Harness, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    h.services.put(h.tenant, view(id, "live", "internal"));
    h.evidence.record(
        h.tenant,
        &identity(name, "prod"),
        EvidenceTimes {
            traces: Some(ns_ago(10)),
            ..Default::default()
        },
    );
    h.inputs.set(h.tenant, vec![inputs_for(id, name, "live")]);
    assert_eq!(h.evaluate().await.failed, 0);
    id
}

#[tokio::test]
async fn an_older_evaluation_never_replaces_a_newer_one() {
    let h = Harness::start().await;
    let id = Uuid::new_v4();
    let rule = current_rule(&h.state.db, h.tenant).await.unwrap().unwrap();
    let ids = vec![identity("payments", "prod")];
    let at = |s: i64| Utc::now() - Duration::seconds(s);
    let snapshot = |evaluated_at, evidence_at| Snapshot {
        tenant_id: h.tenant,
        service_id: id,
        environment_class: EnvironmentClass::Production,
        rule_id: rule.rule_id.clone(),
        rule_revision: rule.revision,
        scope_key: scope_key(&ids),
        identities: ids.clone(),
        conditions: vec![ConditionEvidence {
            signal: "any".into(),
            classification: "internal".into(),
            last_evidence_at: evidence_at,
        }],
        freshness_seconds: rule.freshness_seconds,
        evaluated_at,
    };
    let newer = snapshot(at(5), Some(at(6)));
    let older = snapshot(at(60), None);
    assert!(persist(&h.state.db, &newer).await.unwrap());
    assert!(!persist(&h.state.db, &older).await.unwrap());
    let stored = load(&h.state.db, h.tenant, id, EnvironmentClass::Production)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.conditions, newer.conditions);

    // Concurrent writers: whatever the completion order, the latest start wins.
    let starts: Vec<_> = (0..16)
        .map(|i| snapshot(at(40 - i), Some(at(41 - i))))
        .collect();
    let latest = starts.last().unwrap().evaluated_at;
    let mut tasks = vec![];
    for s in starts.into_iter().rev() {
        let db = h.state.db.clone();
        tasks.push(tokio::spawn(async move { persist(&db, &s).await.unwrap() }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    let stored = load(&h.state.db, h.tenant, id, EnvironmentClass::Production)
        .await
        .unwrap()
        .unwrap();
    // The newest stored snapshot (5 s ago) still holds against all of them.
    assert!(stored.evaluated_at >= latest);
    // PostgreSQL keeps microseconds.
    assert_eq!(
        stored.evaluated_at.timestamp_micros(),
        newer.evaluated_at.timestamp_micros()
    );
}

#[tokio::test]
async fn freshness_is_judged_at_read_time() {
    let h = Harness::start().await;
    let id = Uuid::new_v4();
    h.services.put(h.tenant, view(id, "live", "internal"));
    h.inputs
        .set(h.tenant, vec![inputs_for(id, "payments", "live")]);
    let rule = current_rule(&h.state.db, h.tenant).await.unwrap().unwrap();
    h.evidence.record(
        h.tenant,
        &identity("payments", "prod"),
        EvidenceTimes {
            traces: Some(ns_ago(3600 + 10)),
            ..Default::default()
        },
    );
    // Evaluated an hour ago, when the evidence was ten seconds old: normal then, stale now.
    let snapshot = evaluate(
        &h.evidence,
        h.tenant,
        id,
        EnvironmentClass::Production,
        vec![identity("payments", "prod")],
        &rule,
        Utc::now() - Duration::hours(1),
    )
    .await
    .unwrap();
    assert_eq!(snapshot.freshness(snapshot.evaluated_at).0, "normal");
    persist(&h.state.db, &snapshot).await.unwrap();
    let (status, body, raw) = h.health(id, "production", "member").await;
    assert_eq!(status, 200, "{raw}");
    assert_eq!(
        (&body["state"], &body["evidence_status"]),
        (&json!("unknown"), &json!("stale"))
    );
    assert!(!body["last_evidence_at"].is_null());
    assert_eq!(body["conditions"][0]["evidence_status"], json!("stale"));
}

#[tokio::test]
async fn a_changed_scope_or_rule_invalidates_the_snapshot_until_reevaluation() {
    let h = Harness::start().await;
    let id = evaluated(&h, "payments").await;
    let (_, body, _) = h.health(id, "production", "member").await;
    assert_eq!(body["state"], json!("normal"));

    // The binding moves to another identity: the old snapshot is not disclosed.
    h.inputs
        .set(h.tenant, vec![inputs_for(id, "payments-v2", "live")]);
    let (_, body, _) = h.health(id, "production", "member").await;
    assert_eq!(
        (&body["state"], &body["evidence_status"]),
        (&json!("unknown"), &json!("missing"))
    );
    // Reevaluation over the new identity, which has no evidence yet.
    h.evaluate().await;
    let (_, body, _) = h.health(id, "production", "member").await;
    assert_eq!(body["evidence_status"], json!("missing"));
    h.evidence.record(
        h.tenant,
        &identity("payments-v2", "prod"),
        EvidenceTimes {
            logs: Some(ns_ago(5)),
            ..Default::default()
        },
    );
    h.evaluate().await;
    let (_, body, _) = h.health(id, "production", "member").await;
    assert_eq!(body["state"], json!("normal"));

    // The environment is reclassified: production scope is now empty.
    let mut reclassified = inputs_for(id, "payments-v2", "live");
    reclassified.bindings[0].environment_class = Some(EnvironmentClass::NonProduction);
    reclassified.bindings.remove(1);
    h.inputs.set(h.tenant, vec![reclassified]);
    let (_, body, _) = h.health(id, "production", "member").await;
    assert_eq!(body["evidence_status"], json!("missing"));
    h.inputs
        .set(h.tenant, vec![inputs_for(id, "payments-v2", "live")]);

    // A newer tenant rule revision invalidates snapshots of the old one.
    sqlx::query(
        "INSERT INTO health.rules (rule_id, revision, tenant_id, freshness_seconds, \
         lookback_seconds, conditions) VALUES ('tenant-rule', 1, $1, 60, 3600, \
         '[{\"signal\": \"traces\", \"classification\": \"internal\"}]')",
    )
    .bind(h.tenant)
    .execute(&h.state.db)
    .await
    .unwrap();
    let (_, body, _) = h.health(id, "production", "member").await;
    assert_eq!(body["evidence_status"], json!("missing"));
    // The new rule needs traces, which payments-v2 does not send.
    h.evaluate().await;
    let (_, body, _) = h.health(id, "production", "member").await;
    assert_eq!(
        (&body["state"], &body["evidence_status"]),
        (&json!("unknown"), &json!("missing"))
    );
    // Another tenant still uses the platform default.
    let other = current_rule(&h.state.db, h.other_tenant)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(other.rule_id, "platform-default");
}

#[tokio::test]
async fn reads_fail_closed_and_stay_within_the_callers_tenant() {
    let h = Harness::start().await;
    let id = evaluated(&h, "payments").await;
    let uri = format!("/v1/health/services/{id}?environment_class=production");

    assert_eq!(h.get(&uri, None).await.0, 401);
    assert_eq!(h.get(&uri, Some("forged")).await.0, 401);
    for q in ["", "?environment_class=", "?environment_class=Production"] {
        let (status, body, _) = h
            .get(&format!("/v1/health/services/{id}{q}"), Some("member"))
            .await;
        assert_eq!(
            (status, &body["error"]),
            (400, &json!("invalid_request")),
            "{q}"
        );
    }
    // A malformed id is answered like a missing one.
    let (status, _, _) = h
        .get(
            "/v1/health/services/not-a-uuid?environment_class=production",
            Some("member"),
        )
        .await;
    assert_eq!(status, 404);

    // The same id in another tenant is not found, even with identical names and evidence.
    let (status, body, _) = h.health(id, "production", "other-member").await;
    assert_eq!((status, body), (404, json!({"error": "not_found"})));

    // Catalog outage: no state is disclosed.
    h.services.set_unavailable(true);
    let (status, body, _) = h.health(id, "production", "member").await;
    assert_eq!(
        (status, body),
        (503, json!({"error": "access_decision_unavailable"}))
    );
    h.services.set_unavailable(false);

    // The worker can no longer read the current scope: evaluation unavailable, no times.
    h.inputs.set_unavailable();
    let (status, body, _) = h.health(id, "production", "member").await;
    assert_eq!(status, 200);
    assert_eq!(
        (
            &body["state"],
            &body["evidence_status"],
            &body["evaluated_at"]
        ),
        (
            &json!("unknown"),
            &json!("evaluation_unavailable"),
            &json!(null)
        )
    );
}

#[tokio::test]
async fn planned_services_have_no_current_health() {
    let h = Harness::start().await;
    let id = evaluated(&h, "payments").await;
    h.services.put(h.tenant, view(id, "planned", "internal"));
    assert_eq!(h.health(id, "production", "member").await.0, 404);
    h.services.put(h.tenant, view(id, "deprecated", "internal"));
    assert_eq!(h.health(id, "production", "member").await.0, 200);
}

#[tokio::test]
async fn evaluator_reads_real_clickhouse_evidence_per_class() {
    let h = Harness::start().await;
    let ch = test_support::clickhouse::shared_client().await;
    let name = format!("svc-{}", Uuid::new_v4().simple());
    let recent = ns_ago(20);
    ch.query(&format!(
        "INSERT INTO observable.spans (tenant_id, trace_id, span_id, service_name, operation_name, \
         start_time_unix_nano, end_time_unix_nano, duration_ns, environment) VALUES \
         ('{}', 't1', 's1', '{name}', 'GET /', {recent}, {recent}, 1, 'prod')",
        h.tenant
    ))
    .execute()
    .await
    .unwrap();
    let id = Uuid::new_v4();
    h.services.put(h.tenant, view(id, "live", "internal"));
    h.inputs.set(h.tenant, vec![inputs_for(id, &name, "live")]);
    let evidence = Evidence::clickhouse(ch);
    let summary = health_service::assessment::evaluate_all(&h.state.db, &h.inputs, &evidence).await;
    assert_eq!((summary.stored, summary.failed), (2, 0));

    let (_, prod, raw) = h.health(id, "production", "member").await;
    assert_eq!(prod["state"], json!("normal"), "{raw}");
    let at =
        chrono::DateTime::parse_from_rfc3339(prod["last_evidence_at"].as_str().unwrap()).unwrap();
    assert_eq!(at.timestamp_nanos_opt().unwrap() as u64, recent);
    // Production evidence never makes non-production normal.
    let (_, staging, _) = h.health(id, "non_production", "member").await;
    assert_eq!(
        (&staging["state"], &staging["evidence_status"]),
        (&json!("unknown"), &json!("missing"))
    );

    // ClickHouse outage: nothing is stored and the snapshot ages out at read time.
    let down = Evidence::clickhouse(
        clickhouse::Client::default()
            .with_url("http://127.0.0.1:1")
            .with_compression(clickhouse::Compression::None),
    );
    let summary = health_service::assessment::evaluate_all(&h.state.db, &h.inputs, &down).await;
    assert_eq!((summary.stored, summary.failed), (0, 2));
}

#[tokio::test]
async fn http_service_reads_forward_the_callers_credential() {
    use axum::extract::Path;
    use axum::response::IntoResponse;
    use axum::{Json, Router, http::HeaderMap, http::StatusCode, routing::get};

    let (known, other) = (Uuid::new_v4(), Uuid::new_v4());
    let app = Router::new().route(
        "/v1/catalog/services/{id}",
        get(move |headers: HeaderMap, Path(id): Path<Uuid>| async move {
            let auth = headers.get("authorization").and_then(|v| v.to_str().ok());
            if auth != Some("Bearer user-session") {
                return StatusCode::UNAUTHORIZED.into_response();
            }
            if headers.get("x-tenant-id").is_none() {
                return StatusCode::BAD_REQUEST.into_response();
            }
            if id == known {
                Json(
                    json!({"id": known, "lifecycle": "live", "classification": "internal",
                            "business_name": "Payments"}),
                )
                .into_response()
            } else if id == other {
                // A misbehaving provider answering for a different record.
                Json(json!({"id": known, "lifecycle": "live", "classification": "internal"}))
                    .into_response()
            } else {
                (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))).into_response()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let reads = ServiceReads::Http {
        base_url: Arc::new(format!("http://{addr}/")),
        client: reqwest::Client::new(),
    };
    let caller = health_service::ports::Caller {
        tenant_id: Uuid::new_v4(),
        user_id: Some(Uuid::new_v4()),
        role: "member".into(),
    };
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer user-session".parse().unwrap());
    headers.insert("x-tenant-id", caller.tenant_id.to_string().parse().unwrap());
    assert_eq!(
        reads
            .read(&caller, &headers, known)
            .await
            .unwrap()
            .lifecycle,
        "live"
    );
    assert_eq!(
        reads.read(&caller, &headers, Uuid::new_v4()).await,
        Err(ServiceReadError::NotFound)
    );
    assert_eq!(
        reads.read(&caller, &headers, other).await,
        Err(ServiceReadError::Unavailable)
    );
    assert_eq!(
        reads.read(&caller, &HeaderMap::new(), known).await,
        Err(ServiceReadError::Unauthorized)
    );
    let gone = ServiceReads::Http {
        base_url: Arc::new("http://127.0.0.1:1".into()),
        client: reqwest::Client::new(),
    };
    assert_eq!(
        gone.read(&caller, &headers, known).await,
        Err(ServiceReadError::Unavailable)
    );
}
