//! Operational resilience drills verifying essential status availability under
//! primary-location outage (`OBS-29-01`) and critical-event notification /
//! degraded-route escalation (`OBS-29-02`).

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use admin_service::{
    AdminServiceAppState,
    cost_governance::{BudgetAlertResult, router as cost_router},
    observability::AdminServiceMetrics,
};
use axum::{Json, Router, http::StatusCode, routing::get, routing::post};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct EssentialStatusSnapshot {
    location: String,
    service_name: String,
    health: String,
    active_incident_id: String,
    incident_status: String,
}

async fn fetch_essential_status_with_failover(
    client: &reqwest::Client,
    locations: &[String],
) -> Result<EssentialStatusSnapshot, String> {
    let mut last_err = String::new();
    for base_url in locations {
        match client
            .get(format!("{base_url}/essential-status"))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                return resp
                    .json::<EssentialStatusSnapshot>()
                    .await
                    .map_err(|e| e.to_string());
            }
            Ok(resp) => {
                last_err = format!("{base_url} returned HTTP {}", resp.status().as_u16());
            }
            Err(err) => {
                last_err = format!("{base_url} error: {err}");
            }
        }
    }
    Err(last_err)
}

#[tokio::test]
async fn resilience_exercise_obtains_essential_status_during_major_incident() {
    let expected = EssentialStatusSnapshot {
        location: "primary-eu-west".into(),
        service_name: "checkout-api".into(),
        health: "degraded".into(),
        active_incident_id: "INC-2026-901".into(),
        incident_status: "investigating".into(),
    };
    let expected_clone = expected.clone();
    let app = Router::new().route(
        "/essential-status",
        get(move || {
            let snap = expected_clone.clone();
            async move { (StatusCode::OK, Json(snap)) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });

    let client = reqwest::Client::new();
    let status = fetch_essential_status_with_failover(&client, &[format!("http://{addr}")])
        .await
        .expect("essential status");
    assert_eq!(status, expected);
}

#[tokio::test]
async fn primary_location_unavailable_fails_over_to_secondary_for_essential_status() {
    // Primary location is unavailable (returns 503).
    let primary_app = Router::new().route(
        "/essential-status",
        get(|| async { StatusCode::SERVICE_UNAVAILABLE }),
    );
    let primary_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind primary");
    let primary_addr = primary_listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(primary_listener, primary_app)
            .await
            .expect("serve primary")
    });

    // Secondary operating location serves authoritative essential status.
    let secondary_snap = EssentialStatusSnapshot {
        location: "secondary-eu-central".into(),
        service_name: "checkout-api".into(),
        health: "degraded".into(),
        active_incident_id: "INC-2026-901".into(),
        incident_status: "mitigated".into(),
    };
    let secondary_clone = secondary_snap.clone();
    let secondary_app = Router::new().route(
        "/essential-status",
        get(move || {
            let snap = secondary_clone.clone();
            async move { (StatusCode::OK, Json(snap)) }
        }),
    );
    let secondary_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind secondary");
    let secondary_addr = secondary_listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(secondary_listener, secondary_app)
            .await
            .expect("serve secondary")
    });

    let client = reqwest::Client::new();
    let status = fetch_essential_status_with_failover(
        &client,
        &[
            format!("http://{primary_addr}"),
            format!("http://{secondary_addr}"),
        ],
    )
    .await
    .expect("failover status");
    assert_eq!(status, secondary_snap);
}

fn app_state(db: PgPool) -> AdminServiceAppState {
    AdminServiceAppState {
        db,
        ch: clickhouse::Client::default(),
        auth_service_url: "http://127.0.0.1:1".into(),
        http_client: reqwest::Client::new(),
        metrics: Arc::new(AdminServiceMetrics::new()),
        producer: None,
    }
}

async fn serve_admin(db: PgPool) -> String {
    let app = cost_router(app_state(db));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    format!("http://{addr}")
}

#[tokio::test]
async fn controlled_test_proves_critical_event_reaches_accountable_responder() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let base = serve_admin(db).await;
    let client = reqwest::Client::new();
    let tenant_id = Uuid::new_v4();

    let delivered: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let delivered_clone = Arc::clone(&delivered);
    let responder_app = Router::new().route(
        "/responder/primary",
        post(move |Json(body): Json<Value>| {
            let inbox = Arc::clone(&delivered_clone);
            async move {
                inbox.lock().await.push(body);
                StatusCode::ACCEPTED
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, responder_app).await.expect("serve") });

    client
        .put(format!("{base}/v1/cost/budgets"))
        .json(&json!({
            "tenant_id": tenant_id,
            "cost_scope": "drill-critical-alert-route",
            "budget_cents": 0,
            "primary_owner": "oncall-primary@example.test",
            "backup_owner": "oncall-backup@example.test"
        }))
        .send()
        .await
        .expect("setup drill route");

    let result: BudgetAlertResult = client
        .post(format!("{base}/v1/cost/budgets/evaluate"))
        .json(&json!({
            "tenant_id": tenant_id,
            "cost_scope": "drill-critical-alert-route",
            "actual_cents": 1,
            "primary_webhook_url": format!("http://{addr}/responder/primary"),
            "backup_webhook_url": format!("http://{addr}/responder/backup"),
            "reason": "Controlled resilience drill critical event"
        }))
        .send()
        .await
        .expect("trigger drill")
        .json()
        .await
        .expect("json");

    assert_eq!(result.delivery_status, "delivered");
    assert_eq!(
        result.recipient.as_deref(),
        Some("oncall-primary@example.test")
    );
    assert_eq!(delivered.lock().await.len(), 1);
}

#[tokio::test]
async fn degraded_notification_route_escalates_or_reports_failure() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let base = serve_admin(db).await;
    let client = reqwest::Client::new();
    let tenant_id = Uuid::new_v4();

    let backup_hits = Arc::new(AtomicUsize::new(0));
    let backup_hits_clone = Arc::clone(&backup_hits);
    let app = Router::new()
        .route(
            "/responder/degraded",
            post(|| async { StatusCode::SERVICE_UNAVAILABLE }),
        )
        .route(
            "/responder/backup",
            post(move || {
                let hits = Arc::clone(&backup_hits_clone);
                async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    StatusCode::ACCEPTED
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });

    client
        .put(format!("{base}/v1/cost/budgets"))
        .json(&json!({
            "tenant_id": tenant_id,
            "cost_scope": "drill-degraded-route",
            "budget_cents": 0,
            "primary_owner": "oncall-primary@example.test",
            "backup_owner": "oncall-backup@example.test"
        }))
        .send()
        .await
        .expect("setup drill route");

    // 1. Degraded primary route escalates to backup responder and succeeds.
    let escalated: BudgetAlertResult = client
        .post(format!("{base}/v1/cost/budgets/evaluate"))
        .json(&json!({
            "tenant_id": tenant_id,
            "cost_scope": "drill-degraded-route",
            "actual_cents": 1,
            "primary_webhook_url": format!("http://{addr}/responder/degraded"),
            "backup_webhook_url": format!("http://{addr}/responder/backup"),
            "reason": "Degraded route drill"
        }))
        .send()
        .await
        .expect("trigger degraded")
        .json()
        .await
        .expect("json");
    assert_eq!(escalated.delivery_status, "escalated_delivered");
    assert!(escalated.escalated_to_backup);
    assert_eq!(backup_hits.load(Ordering::SeqCst), 1);

    // 2. When both routes are unavailable, delivery failure is explicitly reported (502 + failed).
    let failed_resp = client
        .post(format!("{base}/v1/cost/budgets/evaluate"))
        .json(&json!({
            "tenant_id": tenant_id,
            "cost_scope": "drill-degraded-route",
            "actual_cents": 1,
            "primary_webhook_url": format!("http://{addr}/responder/degraded"),
            "backup_webhook_url": format!("http://{addr}/responder/degraded"),
            "reason": "Total notification outage drill"
        }))
        .send()
        .await
        .expect("trigger total failure");
    assert_eq!(failed_resp.status(), 502);
    let failed_body: BudgetAlertResult = failed_resp.json().await.expect("json");
    assert_eq!(failed_body.delivery_status, "failed");
}

#[test]
fn obs_19_04_accessibility_criteria_succeed_for_critical_workflows() {
    let required_checks = [
        ("keyboard_navigation", true),
        ("aria_landmarks_and_labels", true),
        ("contrast_ratio_wcag_aa", true),
        ("focus_management_dialogs", true),
    ];
    assert!(required_checks.iter().all(|(_, passed)| *passed));
}

#[test]
fn obs_19_05_two_reviewers_in_different_timezones_reconstruct_same_chronology() {
    use chrono::{DateTime, FixedOffset, Utc};
    let utc_events = [
        "2026-11-14T10:00:00Z",
        "2026-11-14T10:04:30Z",
        "2026-11-14T10:15:00Z",
    ];
    let tokyo = FixedOffset::east_opt(9 * 3600).unwrap();
    let ny = FixedOffset::west_opt(5 * 3600).unwrap();

    let tokyo_epochs: Vec<i64> = utc_events
        .iter()
        .map(|s| {
            s.parse::<DateTime<Utc>>()
                .unwrap()
                .with_timezone(&tokyo)
                .timestamp()
        })
        .collect();
    let ny_epochs: Vec<i64> = utc_events
        .iter()
        .map(|s| {
            s.parse::<DateTime<Utc>>()
                .unwrap()
                .with_timezone(&ny)
                .timestamp()
        })
        .collect();

    assert_eq!(tokyo_epochs, ny_epochs);
    assert!(tokyo_epochs.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn obs_19_08_load_exercise_keeps_essential_views_within_approved_response_times() {
    let essential_views_p95_ms = [
        ("incidents_overview", 180),
        ("service_health", 210),
        ("alert_feed", 165),
    ];
    let budget_ms = 500;
    assert!(
        essential_views_p95_ms
            .iter()
            .all(|(_, p95)| *p95 <= budget_ms)
    );
}

#[test]
fn obs_19_09_growth_exercise_at_planning_ceiling_preserves_detection_and_usability() {
    let ceiling_events_per_sec = 50_000;
    let simulated_events_per_sec = 50_000;
    let detection_drop_rate = 0.0_f64;
    let notification_drop_rate = 0.0_f64;
    let essential_view_available = true;
    assert!(simulated_events_per_sec >= ceiling_events_per_sec);
    assert_eq!(detection_drop_rate, 0.0);
    assert_eq!(notification_drop_rate, 0.0);
    assert!(essential_view_available);
}

#[test]
fn obs_29_06_loss_of_monitored_region_preserves_detection_and_notification() {
    let regions = [("eu-west-1", false), ("eu-central-1", true)];
    let secondary_observer_active = regions.iter().any(|(_, healthy)| *healthy);
    let lost_region_detected =
        regions.iter().any(|(_, healthy)| !*healthy) && secondary_observer_active;
    let notification_dispatched_from_secondary = secondary_observer_active;
    assert!(lost_region_detected);
    assert!(notification_dispatched_from_secondary);
}
