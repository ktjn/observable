//! Scheduled status reports (W5b, OBS-13-07, docs/assurance-relocations.md 4.5) through the
//! router and the report worker, with real PostgreSQL as the scoped `observable_health` login.

mod common;

use chrono::{DateTime, Duration, Utc};
use common::{Harness, inputs_for, view};
use health_service::reports::{Period, generate_due};
use serde_json::{Value, json};
use uuid::Uuid;

async fn scoped_harness() -> Harness {
    let h = Harness::start().await;
    let scoped = test_support::postgres::scoped_pool(&h.state.db, "observable_health").await;
    let mut state = h.state.clone();
    state.db = scoped;
    Harness {
        app: health_service::app(state.clone()),
        state,
        ..h
    }
}

async fn transition(h: &Harness, service: Uuid, at: DateTime<Utc>, to: &str, evidence: &str) {
    sqlx::query(
        "INSERT INTO health.transitions (tenant_id, service_id, environment_class, at, to_state, \
         evidence_status, rule_id, rule_revision) \
         VALUES ($1, $2, 'production', $3, $4, $5, 'platform-default', 1)",
    )
    .bind(h.tenant)
    .bind(service)
    .bind(at)
    .bind(to)
    .bind(evidence)
    .execute(&h.state.db)
    .await
    .expect("transition");
}

fn entries(body: &Value) -> Vec<Value> {
    body["reports"][0]["entries"].as_array().unwrap().clone()
}

#[tokio::test]
async fn approved_schedule_reports_exactly_the_chosen_business_scope() {
    let h = scoped_harness().await;
    let (payments, ledger, fraud, unrelated) = (
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    );
    for id in [payments, ledger, fraud, unrelated] {
        h.services.put(h.tenant, view(id, "live", "internal"));
    }
    h.inputs.set(
        h.tenant,
        vec![
            inputs_for(payments, "payments", "live"),
            inputs_for(ledger, "ledger", "live"),
            inputs_for(fraud, "fraud", "live"),
            inputs_for(unrelated, "search", "live"),
        ],
    );
    let body = |ids: Value| {
        json!({"name": "Retail weekly status", "scope": {"service_ids": ids},
               "environment_class": "production", "period": "weekly"})
        .to_string()
    };
    let scope = json!([payments, ledger, fraud, payments]);

    // Only a signed-in tenant admin requests a schedule; the scope must be readable.
    for who in ["member", "viewer", "admin-key"] {
        let (status, _, raw) = h
            .send(
                "POST",
                "/v1/health/report-schedules",
                who,
                &body(scope.clone()),
            )
            .await;
        assert_eq!(status, 403, "{who}: {raw}");
    }
    let (status, _, _) = h
        .send(
            "POST",
            "/v1/health/report-schedules",
            "admin",
            &body(json!([Uuid::new_v4()])),
        )
        .await;
    assert_eq!(status, 400);
    let (status, created, raw) = h
        .send("POST", "/v1/health/report-schedules", "admin", &body(scope))
        .await;
    assert_eq!(status, 201, "{raw}");
    assert_eq!(created["status"], "pending_approval");
    assert_eq!(
        created["scope"]["service_ids"],
        json!([payments, ledger, fraud])
    );
    let id = created["schedule_id"].as_str().unwrap().to_owned();

    // Unapproved schedules never run.
    let later = Utc::now() + Duration::days(14);
    assert_eq!(generate_due(&h.state.db, &h.inputs, later).await, 0);

    // A second tenant admin approves; the requester cannot.
    let approve = format!("/v1/health/report-schedules/{id}/approve");
    let (status, _, _) = h.send("POST", &approve, "admin", "").await;
    assert_eq!(status, 403);
    let (status, _, _) = h.send("POST", &approve, "member", "").await;
    assert_eq!(status, 403);
    let (status, approved, raw) = h.send("POST", &approve, "admin2", "").await;
    assert_eq!(status, 200, "{raw}");
    assert_eq!(approved["status"], "approved");
    assert_eq!(approved["approved_by"], json!(h.users["admin2"]));
    let (status, _, _) = h.send("POST", &approve, "admin2", "").await;
    assert_eq!(status, 409);

    // The reported period: the last completed week before `later`.
    let (week, _) = Period::Weekly.containing(later);
    let (start, end) = Period::Weekly.containing(week - Duration::seconds(1));
    let day = |d: i64| start + Duration::days(d);
    // payments: degraded before, unavailable then normal during the week -> improving.
    transition(
        &h,
        payments,
        start - Duration::days(1),
        "degraded",
        "current",
    )
    .await;
    transition(&h, payments, day(1), "unavailable", "current").await;
    transition(&h, payments, day(2), "normal", "current").await;
    // ledger: normal before, degraded during the week -> degrading.
    transition(&h, ledger, start - Duration::days(2), "normal", "current").await;
    transition(&h, ledger, day(3), "degraded", "current").await;
    // After the period: never part of this report.
    transition(&h, ledger, end + Duration::hours(1), "normal", "current").await;
    // A service outside the scope.
    transition(&h, unrelated, day(1), "unavailable", "current").await;
    // fraud has no evidence at all.

    assert_eq!(generate_due(&h.state.db, &h.inputs, later).await, 1);
    assert_eq!(
        generate_due(&h.state.db, &h.inputs, later).await,
        0,
        "one snapshot per period"
    );

    let reports = format!("/v1/health/report-schedules/{id}/reports");
    let (status, body, raw) = h.get(&reports, Some("viewer")).await;
    assert_eq!(status, 200, "{raw}");
    assert_eq!(body["reports"].as_array().unwrap().len(), 1);
    assert_eq!(body["reports"][0]["period_start"], json!(start));
    assert_eq!(body["reports"][0]["period_end"], json!(end));
    let got = entries(&body);
    let ids: Vec<Uuid> = got
        .iter()
        .map(|e| e["service_id"].as_str().unwrap().parse().unwrap())
        .collect();
    assert_eq!(
        ids,
        vec![payments, ledger, fraud],
        "exactly the chosen scope"
    );
    let by = |s: Uuid| got.iter().find(|e| e["service_id"] == json!(s)).unwrap();
    let p = by(payments);
    assert_eq!(
        (&p["state"], &p["trend"], &p["evidence_status"]),
        (&json!("normal"), &json!("improving"), &json!("current"))
    );
    assert_eq!(p["state_as_of"], json!(day(2)));
    assert_eq!(p["interruptions"], json!(1));
    assert_eq!(
        p["scope"],
        json!({"status": "bound", "identities": [{"service_name": "payments", "environment": "prod"}]})
    );
    let l = by(ledger);
    assert_eq!(
        (&l["state"], &l["trend"]),
        (&json!("degraded"), &json!("degrading"))
    );
    let f = by(fraud);
    assert_eq!(
        (&f["state"], &f["evidence_status"], &f["trend"]),
        (
            &json!("unknown"),
            &json!("missing"),
            &json!("insufficient_data")
        )
    );
    assert_eq!(body["entries_withheld"], false);

    // Read with the caller's catalog visibility: a hidden service is omitted, never counted.
    h.services.hide(fraud);
    let (_, body, _) = h.get(&reports, Some("viewer")).await;
    let ids: Vec<String> = entries(&body)
        .iter()
        .map(|e| e["service_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ids, [payments.to_string(), ledger.to_string()]);
    assert_eq!(body["entries_withheld"], true);
    assert_eq!(
        body["schedule"]["scope"]["service_ids"],
        json!([payments, ledger])
    );
    assert!(!body.to_string().contains(&fraud.to_string()));

    let (status, _, _) = h.get(&reports, Some("other-member")).await;
    assert_eq!(status, 404, "another tenant never sees the schedule");
}
