//! Planned maintenance windows (phase 2 E2) through the router with real PostgreSQL and the
//! scoped `observable_health` login.

mod common;

use chrono::{Duration, Utc};
use common::{Harness, inputs_for, ns_ago, view};
use health_service::app;
use health_service::ports::{EvidenceTimes, OwnerRef, RequestSample, TelemetryIdentity};
use observable_auth::authz::MockScenario;
use serde_json::{Value, json};
use uuid::Uuid;

fn identity(name: &str, env: &str) -> TelemetryIdentity {
    TelemetryIdentity {
        service_name: name.into(),
        environment: env.into(),
    }
}

/// Presence plus "every request failing" (unavailable) over five minutes.
async fn publish_rule(h: &Harness) {
    let rule = json!({
        "freshness_seconds": 300,
        "lookback_seconds": 3600,
        "conditions": [
            {"signal": "any", "classification": "internal"},
            {"kind": "error_ratio", "signal": "traces", "classification": "internal",
             "threshold": 1.0, "window_seconds": 300, "min_requests": 10, "yields": "unavailable"},
        ],
    });
    let (status, _, raw) = h
        .send("PUT", "/v1/health/rules", "admin", &rule.to_string())
        .await;
    assert_eq!(status, 201, "{raw}");
}

/// A live service whose production requests are all failing (`errors = true`) or succeeding.
fn service(h: &Harness, name: &str, failing: bool) -> Uuid {
    let id = Uuid::new_v4();
    h.services.put(h.tenant, view(id, "live", "internal"));
    h.inputs.set(h.tenant, vec![inputs_for(id, name, "live")]);
    let prod = identity(name, "prod");
    h.evidence.record(
        h.tenant,
        &prod,
        EvidenceTimes {
            logs: Some(ns_ago(5)),
            ..Default::default()
        },
    );
    h.evidence.record_requests(
        h.tenant,
        &prod,
        (0..20)
            .map(|i| RequestSample {
                at_ns: ns_ago(5 + i),
                duration_ns: 1_000_000,
                error: failing,
            })
            .collect(),
    );
    id
}

fn window_body(start_min: i64, end_min: i64) -> String {
    json!({
        "environment_class": "production",
        "starts_at": Utc::now() + Duration::minutes(start_min),
        "ends_at": Utc::now() + Duration::minutes(end_min),
        "reason": "database failover drill",
    })
    .to_string()
}

async fn request(h: &Harness, id: Uuid, token: &str, body: &str) -> (u16, Value, String) {
    h.send(
        "POST",
        &format!("/v1/health/services/{id}/maintenance"),
        token,
        body,
    )
    .await
}

async fn act(h: &Harness, window: &Value, action: &str, token: &str) -> (u16, Value, String) {
    let id = window["id"].as_str().unwrap();
    h.send(
        "POST",
        &format!("/v1/health/maintenance/{id}/{action}"),
        token,
        "",
    )
    .await
}

async fn state(h: &Harness, id: Uuid) -> Value {
    let (status, body, raw) = h.health(id, "production", "member").await;
    assert_eq!(status, 200, "{raw}");
    body
}

#[tokio::test]
async fn an_approved_current_window_discloses_failure_as_planned_maintenance() {
    let h = Harness::start().await;
    publish_rule(&h).await;
    let id = service(&h, "payments", true);
    h.evaluate().await;
    assert_eq!(state(&h, id).await["state"], json!("unavailable"));

    let (status, window, raw) = request(&h, id, "admin", &window_body(-1, 60)).await;
    assert_eq!(status, 201, "{raw}");
    assert_eq!(window["status"], json!("pending"));
    // Requested but not approved: no effect.
    let body = state(&h, id).await;
    assert_eq!(
        (&body["state"], &body["maintenance"]),
        (&json!("unavailable"), &json!(null))
    );

    // The requester cannot approve, whatever their role.
    let (status, body, _) = act(&h, &window, "approve", "admin").await;
    assert_eq!(
        (status, body),
        (403, json!({"error": "self_approval_refused"}))
    );
    let (status, approved, raw) = act(&h, &window, "approve", "admin2").await;
    assert_eq!(status, 200, "{raw}");
    assert_eq!(approved["status"], json!("active"));
    assert_eq!(approved["approved_by"], json!(h.users["admin2"]));
    assert_eq!(approved["requested_by"], json!(h.users["admin"]));

    let body = state(&h, id).await;
    assert_eq!(body["state"], json!("planned_maintenance"));
    assert_eq!(body["underlying_state"], json!("unavailable"));
    assert_eq!(body["maintenance"]["id"], window["id"]);
    assert_eq!(
        body["maintenance"]["reason"],
        json!("database failover drill")
    );
    // The underlying condition stays visible.
    assert_eq!(body["conditions"][1]["outcome"], json!("breached"));
    // Non-production is not covered by a production window.
    let (_, staging, _) = h.health(id, "non_production", "member").await;
    assert_eq!(staging["maintenance"], json!(null));

    // Approving twice is refused; cancelling ends the effect.
    assert_eq!(act(&h, &window, "approve", "admin2").await.0, 409);
    let (status, cancelled, _) = act(&h, &window, "cancel", "admin").await;
    assert_eq!(status, 200);
    assert_eq!(cancelled["status"], json!("cancelled"));
    assert_eq!(cancelled["cancelled_by"], json!(h.users["admin"]));
    let body = state(&h, id).await;
    assert_eq!(
        (&body["state"], &body["underlying_state"]),
        (&json!("unavailable"), &json!(null))
    );
    assert_eq!(act(&h, &window, "cancel", "admin2").await.0, 409);
    assert_eq!(act(&h, &window, "approve", "admin2").await.0, 409);
}

#[tokio::test]
async fn expired_future_and_unapproved_windows_have_no_effect_and_never_hide_unknown() {
    let h = Harness::start().await;
    publish_rule(&h).await;
    let failing = service(&h, "payments", true);
    h.evaluate().await;
    let (admin, admin2) = (h.users["admin"], h.users["admin2"]);
    let insert = |start: i64, end: i64, approved: bool| {
        let db = h.state.db.clone();
        let tenant = h.tenant;
        async move {
            sqlx::query(
                "INSERT INTO health.maintenance_windows (id, tenant_id, service_id, \
                 environment_class, starts_at, ends_at, reason, requested_by, requested_at, \
                 approved_by, approved_at) VALUES (gen_random_uuid(), $1, $2, 'production', \
                 now() + make_interval(mins => $3), now() + make_interval(mins => $4), 'drill', \
                 $5, now() - interval '2 hours', $6, $7)",
            )
            .bind(tenant)
            .bind(failing)
            .bind(start as i32)
            .bind(end as i32)
            .bind(admin)
            .bind(approved.then_some(admin2))
            .bind(approved.then(|| Utc::now() - Duration::hours(1)))
            .execute(&db)
            .await
            .unwrap();
        }
    };
    insert(-90, -30, true).await; // expired
    insert(30, 90, true).await; // approved, not started
    insert(-5, 60, false).await; // current, unapproved
    let body = state(&h, failing).await;
    assert_eq!(
        (&body["state"], &body["maintenance"]),
        (&json!("unavailable"), &json!(null))
    );
    let (_, listed, _) = h
        .get(
            &format!("/v1/health/services/{failing}/maintenance"),
            Some("viewer"),
        )
        .await;
    let mut statuses: Vec<&str> = listed["windows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["status"].as_str().unwrap())
        .collect();
    statuses.sort();
    assert_eq!(statuses, vec!["approved", "expired", "pending"]);
    // An expired window cannot be approved after the fact.
    let expired = listed["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["status"] == json!("expired"))
        .unwrap()
        .clone();
    assert_eq!(act(&h, &expired, "approve", "admin2").await.0, 409);

    // A window never turns normal or unknown into anything else.
    let healthy = service(&h, "ledger", false);
    let silent = Uuid::new_v4();
    h.services.put(h.tenant, view(silent, "live", "internal"));
    h.inputs.set(
        h.tenant,
        vec![
            inputs_for(healthy, "ledger", "live"),
            inputs_for(silent, "silent", "live"),
        ],
    );
    h.evaluate().await;
    for (id, expected) in [(healthy, "normal"), (silent, "unknown")] {
        let (_, w, _) = request(&h, id, "admin", &window_body(-1, 60)).await;
        assert_eq!(act(&h, &w, "approve", "admin2").await.0, 200);
        let body = state(&h, id).await;
        assert_eq!(body["state"], json!(expected));
        assert_eq!(body["underlying_state"], json!(null));
        assert_eq!(body["maintenance"]["id"], w["id"]);
    }
}

#[tokio::test]
async fn only_admins_and_the_operational_owner_manage_windows() {
    let h = Harness::start().await;
    let id = service(&h, "payments", false);
    let body = window_body(10, 70);
    for token in ["viewer", "member"] {
        let (status, b, _) = request(&h, id, token, &body).await;
        assert_eq!((status, b), (403, json!({"error": "forbidden"})), "{token}");
    }
    let (status, b, _) = request(&h, id, "admin-key", &body).await;
    assert_eq!(
        (status, b),
        (403, json!({"error": "user_session_required"}))
    );
    assert_eq!(request(&h, id, "forged", &body).await.0, 401);

    // The member who operates the service may request; a departed owner may not.
    let owner = |state: &str| OwnerRef {
        role: "operational_owner".into(),
        principal_kind: "user".into(),
        principal_id: h.users["member"],
        state: state.into(),
    };
    let mut record = view(id, "live", "internal");
    record.owners = vec![owner("departed")];
    h.services.put(h.tenant, record.clone());
    assert_eq!(request(&h, id, "member", &body).await.0, 403);
    record.owners = vec![owner("active")];
    h.services.put(h.tenant, record);
    let (status, window, raw) = request(&h, id, "member", &body).await;
    assert_eq!(status, 201, "{raw}");
    // A viewer cannot approve; an admin can.
    assert_eq!(act(&h, &window, "approve", "viewer").await.0, 403);
    assert_eq!(act(&h, &window, "approve", "admin").await.0, 200);

    // Another tenant's caller sees neither the window nor the service.
    let (status, b, _) = act(&h, &window, "cancel", "other-member").await;
    assert_eq!((status, b), (404, json!({"error": "not_found"})));
    let (status, _, _) = h
        .get(
            &format!("/v1/health/services/{id}/maintenance"),
            Some("other-member"),
        )
        .await;
    assert_eq!(status, 404);
    // A service hidden from the caller hides its windows.
    h.services.hide(id);
    assert_eq!(act(&h, &window, "cancel", "admin").await.0, 404);
    assert_eq!(request(&h, id, "admin", &body).await.0, 404);

    // Policy outage: the same 503 for existing and missing ids.
    h.decider.set_scenario(MockScenario::Unavailable);
    for raw in [
        window["id"].as_str().unwrap().to_owned(),
        Uuid::new_v4().to_string(),
    ] {
        let (status, b, _) = h
            .send(
                "POST",
                &format!("/v1/health/maintenance/{raw}/approve"),
                "admin",
                "",
            )
            .await;
        assert_eq!(
            (status, b),
            (503, json!({"error": "access_decision_unavailable"}))
        );
    }
}

#[tokio::test]
async fn invalid_window_requests_are_refused_with_a_reason() {
    let h = Harness::start().await;
    let id = service(&h, "payments", false);
    let now = Utc::now();
    let body = |class: &str, start: Duration, end: Duration, reason: &str| {
        json!({
            "environment_class": class,
            "starts_at": now + start,
            "ends_at": now + end,
            "reason": reason,
        })
        .to_string()
    };
    let m = Duration::minutes;
    for (case, b) in [
        ("class", body("staging", m(1), m(60), "r")),
        ("ends before start", body("production", m(60), m(1), "r")),
        ("in the past", body("production", m(-60), m(-1), "r")),
        ("too long", body("production", m(1), Duration::days(8), "r")),
        (
            "too far ahead",
            body("production", Duration::days(91), Duration::days(92), "r"),
        ),
        ("empty reason", body("production", m(1), m(60), "  ")),
        (
            "long reason",
            body("production", m(1), m(60), &"x".repeat(501)),
        ),
        ("not json", "{".to_owned()),
        (
            "unknown field",
            json!({"environment_class": "production", "starts_at": now, "ends_at": now + m(5),
                   "reason": "r", "approved_by": Uuid::new_v4()})
            .to_string(),
        ),
    ] {
        let (status, b, raw) = request(&h, id, "admin", &b).await;
        assert_eq!(
            (status, &b["error"]),
            (400, &json!("invalid_request")),
            "{case}: {raw}"
        );
    }
    // Planned and retired services take no windows.
    h.services.put(h.tenant, view(id, "planned", "internal"));
    assert_eq!(request(&h, id, "admin", &window_body(1, 60)).await.0, 404);
}

#[tokio::test]
async fn the_scoped_login_records_windows_that_cannot_be_rewritten() {
    let h = Harness::start().await;
    let id = service(&h, "payments", false);
    let scoped = test_support::postgres::scoped_pool(&h.state.db, "observable_health").await;
    let mut st = h.state.clone();
    st.db = scoped.clone();
    let scoped_app = app(st);
    let send = |path: String, token: &'static str, body: String| {
        let app = scoped_app.clone();
        async move {
            use http_body_util::BodyExt as _;
            use tower::ServiceExt;
            let req = axum::http::Request::post(path)
                .header("Authorization", format!("Bearer {token}"))
                .body(axum::body::Body::from(body))
                .unwrap();
            let resp = app.oneshot(req).await.unwrap();
            let status = resp.status().as_u16();
            let bytes = resp.into_body().collect().await.unwrap().to_bytes();
            (
                status,
                serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
            )
        }
    };
    let (status, window) = send(
        format!("/v1/health/services/{id}/maintenance"),
        "admin",
        window_body(-1, 30),
    )
    .await;
    assert_eq!(status, 201);
    let wid = window["id"].as_str().unwrap().to_owned();
    let (status, _) = send(
        format!("/v1/health/maintenance/{wid}/approve"),
        "admin2",
        String::new(),
    )
    .await;
    assert_eq!(status, 200);
    let wid: Uuid = wid.parse().unwrap();

    for (case, sql) in [
        (
            "reason rewrite",
            "UPDATE health.maintenance_windows SET reason = 'other' WHERE id = $1",
        ),
        (
            "period extension",
            "UPDATE health.maintenance_windows SET ends_at = ends_at + interval '1 hour' WHERE id = $1",
        ),
        (
            "approver swap",
            "UPDATE health.maintenance_windows SET approved_by = gen_random_uuid() WHERE id = $1",
        ),
        (
            "self approval",
            "UPDATE health.maintenance_windows SET approved_by = NULL, approved_at = NULL WHERE id = $1",
        ),
    ] {
        assert!(
            sqlx::query(sql).bind(wid).execute(&scoped).await.is_err(),
            "{case}"
        );
    }
    let self_approved = sqlx::query(
        "INSERT INTO health.maintenance_windows (id, tenant_id, service_id, environment_class, \
         starts_at, ends_at, reason, requested_by, approved_by, approved_at) VALUES \
         (gen_random_uuid(), $1, $2, 'production', now(), now() + interval '1 hour', 'r', $3, $3, now())",
    )
    .bind(h.tenant)
    .bind(id)
    .bind(h.users["admin"])
    .execute(&scoped)
    .await;
    assert!(
        self_approved.is_err(),
        "second-person approval is enforced in the schema"
    );
    // Cancellation is still recordable once.
    let cancelled = sqlx::query(
        "UPDATE health.maintenance_windows SET cancelled_by = $2, cancelled_at = now() WHERE id = $1",
    )
    .bind(wid)
    .bind(h.users["admin"])
    .execute(&scoped)
    .await
    .unwrap();
    assert_eq!(cancelled.rows_affected(), 1);
}
