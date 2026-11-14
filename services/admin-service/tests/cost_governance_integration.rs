//! Integration tests for observability cost attribution (`OBS-17-01`) and
//! budget overrun notifications with backup escalation (`OBS-17-02`).

use std::sync::Arc;

use admin_service::{
    AdminServiceAppState,
    cost_governance::{BudgetAlertResult, CostAttributionReport, router},
    observability::AdminServiceMetrics,
};
use axum::{Json, Router, http::StatusCode, routing::post};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use tokio::sync::Mutex;
use uuid::Uuid;

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
    let app = router(app_state(db));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    format!("http://{addr}")
}

#[tokio::test]
async fn finance_review_identifies_cost_ownership_and_isolates_unattributed_cost() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let base = serve_admin(db).await;
    let client = reqwest::Client::new();
    let tenant_id = Uuid::new_v4();

    // Seed two attributed services and one unattributed service.
    for payload in [
        json!({
            "tenant_id": tenant_id,
            "service_name": "checkout-api",
            "owner_team": "payments-platform",
            "cost_center": "CC-4100",
            "expenditure_cents": 125_000,
            "period": "2026-11"
        }),
        json!({
            "tenant_id": tenant_id,
            "service_name": "billing-worker",
            "owner_team": "payments-platform",
            "cost_center": "CC-4100",
            "expenditure_cents": 75_000,
            "period": "2026-11"
        }),
        json!({
            "tenant_id": tenant_id,
            "service_name": "orphan-batch-ingest",
            "owner_team": null,
            "cost_center": null,
            "expenditure_cents": 40_000,
            "period": "2026-11"
        }),
    ] {
        let resp = client
            .post(format!("{base}/v1/cost/expenditures"))
            .json(&payload)
            .send()
            .await
            .expect("record expenditure");
        assert_eq!(resp.status(), 201);
    }

    let report: CostAttributionReport = client
        .get(format!(
            "{base}/v1/cost/attribution-report?tenant_id={tenant_id}&period=2026-11"
        ))
        .send()
        .await
        .expect("report")
        .json()
        .await
        .expect("json");

    assert_eq!(report.attributed_total_cents, 200_000);
    assert_eq!(report.unattributed_total_cents, 40_000);
    assert_eq!(report.total_cents, 240_000);
    assert_eq!(report.attributed.len(), 1);
    assert_eq!(report.attributed[0].owner_team, "payments-platform");
    assert_eq!(report.attributed[0].cost_center, "CC-4100");
    assert_eq!(report.attributed[0].total_cents, 200_000);
    assert_eq!(
        report.attributed[0].services,
        vec!["billing-worker", "checkout-api"]
    );
    // Unattributed cost is explicitly reported as unattributed, not silently distributed.
    assert_eq!(report.unattributed.len(), 1);
    assert_eq!(report.unattributed[0].service_name, "orphan-batch-ingest");
    assert_eq!(report.unattributed[0].expenditure_cents, 40_000);
}

#[tokio::test]
async fn controlled_budget_exception_reaches_designated_accountable_owner() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let base = serve_admin(db.clone()).await;
    let client = reqwest::Client::new();
    let tenant_id = Uuid::new_v4();

    let primary_inbox: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let inbox_clone = Arc::clone(&primary_inbox);
    let webhook_app = Router::new().route(
        "/owner/primary",
        post(move |Json(body): Json<Value>| {
            let inbox = Arc::clone(&inbox_clone);
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
    tokio::spawn(async move { axum::serve(listener, webhook_app).await.expect("serve") });

    client
        .put(format!("{base}/v1/cost/budgets"))
        .json(&json!({
            "tenant_id": tenant_id,
            "cost_scope": "checkout-traces",
            "budget_cents": 100_000,
            "primary_owner": "finops-payments@example.test",
            "backup_owner": "vp-eng-oncall@example.test"
        }))
        .send()
        .await
        .expect("budget");

    let result: BudgetAlertResult = client
        .post(format!("{base}/v1/cost/budgets/evaluate"))
        .json(&json!({
            "tenant_id": tenant_id,
            "cost_scope": "checkout-traces",
            "actual_cents": 135_000,
            "primary_webhook_url": format!("http://{addr}/owner/primary"),
            "backup_webhook_url": format!("http://{addr}/owner/backup"),
            "reason": "Controlled load-test trace exception"
        }))
        .send()
        .await
        .expect("evaluate")
        .json()
        .await
        .expect("json");

    assert!(result.breached);
    assert_eq!(result.delivery_status, "delivered");
    assert!(!result.escalated_to_backup);
    assert_eq!(
        result.recipient.as_deref(),
        Some("finops-payments@example.test")
    );
    assert_eq!(primary_inbox.lock().await.len(), 1);
}

#[tokio::test]
async fn budget_breach_escalates_to_backup_owner_when_usual_recipient_is_absent() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let base = serve_admin(db.clone()).await;
    let client = reqwest::Client::new();
    let tenant_id = Uuid::new_v4();

    let backup_inbox: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let backup_clone = Arc::clone(&backup_inbox);
    let webhook_app = Router::new()
        .route("/owner/primary", post(|| async { StatusCode::NOT_FOUND }))
        .route(
            "/owner/backup",
            post(move |Json(body): Json<Value>| {
                let inbox = Arc::clone(&backup_clone);
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
    tokio::spawn(async move { axum::serve(listener, webhook_app).await.expect("serve") });

    client
        .put(format!("{base}/v1/cost/budgets"))
        .json(&json!({
            "tenant_id": tenant_id,
            "cost_scope": "platform-logs",
            "budget_cents": 50_000,
            "primary_owner": "usual-owner-absent@example.test",
            "backup_owner": "backup-finops-lead@example.test"
        }))
        .send()
        .await
        .expect("budget");

    let result: BudgetAlertResult = client
        .post(format!("{base}/v1/cost/budgets/evaluate"))
        .json(&json!({
            "tenant_id": tenant_id,
            "cost_scope": "platform-logs",
            "actual_cents": 92_000,
            "primary_webhook_url": format!("http://{addr}/owner/primary"),
            "backup_webhook_url": format!("http://{addr}/owner/backup"),
            "reason": "Debug logging spike in billing-worker"
        }))
        .send()
        .await
        .expect("evaluate")
        .json()
        .await
        .expect("json");

    assert!(result.breached);
    assert_eq!(result.delivery_status, "escalated_delivered");
    assert!(result.escalated_to_backup);
    assert_eq!(
        result.recipient.as_deref(),
        Some("backup-finops-lead@example.test")
    );
    assert_eq!(backup_inbox.lock().await.len(), 1);

    let row = sqlx::query(
        "SELECT recipient, delivery_status, escalated_to_backup \
         FROM control.cost_budget_alerts WHERE tenant_id = $1 AND cost_scope = $2",
    )
    .bind(tenant_id)
    .bind("platform-logs")
    .fetch_one(&db)
    .await
    .expect("alert row");
    assert_eq!(
        row.get::<String, _>("recipient"),
        "backup-finops-lead@example.test"
    );
    assert_eq!(
        row.get::<String, _>("delivery_status"),
        "escalated_delivered"
    );
    assert!(row.get::<bool, _>("escalated_to_backup"));
}
