// Cost attribution (OBS-17-01) and budget overrun warnings with backup escalation (OBS-17-02)
// through the real routes: `require_tenant` with a stand-in auth-service, the control schema as
// the scoped `observable_control` login, and stand-in owner endpoints.

use std::sync::Arc;

use admin_service::{
    cost_governance::{CostState, router},
    middleware::auth::require_tenant,
    response::net::TargetPolicy,
};
use axum::{
    Extension, Router,
    body::Body,
    http::{Request, StatusCode},
    middleware,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use test_support::postgres::{isolated_pool, scoped_pool};
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_partial_json, method, path},
};

struct Fixture {
    owner: PgPool,
    _db: test_support::postgres::TestDatabase,
    auth: MockServer,
    tenant: Uuid,
    other_tenant: Uuid,
    app: Router,
}

async fn session(auth: &MockServer, token: &str, tenant: Uuid, role: &str) {
    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .and(body_partial_json(json!({ "session_token": token })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user_id": Uuid::new_v4(), "tenant_id": tenant, "role": role, "environment": "production"
        })))
        .mount(auth)
        .await;
}

fn build_app(db: PgPool, auth_url: String, targets: TargetPolicy) -> Router {
    Router::new()
        .merge(router(CostState { db, targets }))
        .layer(middleware::from_fn(require_tenant))
        .layer(Extension(Arc::new(auth_url)))
        .layer(Extension(reqwest::Client::new()))
}

fn permissive() -> TargetPolicy {
    TargetPolicy {
        allow_http: true,
        allow_private: true,
        ..Default::default()
    }
}

async fn fixture() -> Fixture {
    let (owner, db) = isolated_pool().await;
    let scoped = scoped_pool(&owner, "observable_control").await;
    let auth = MockServer::start().await;
    let (tenant, other_tenant) = (Uuid::new_v4(), Uuid::new_v4());
    session(&auth, "admin", tenant, "tenant_admin").await;
    session(&auth, "member", tenant, "member").await;
    session(&auth, "other", other_tenant, "tenant_admin").await;
    let app = build_app(scoped, auth.uri(), permissive());
    Fixture {
        owner,
        _db: db,
        auth,
        tenant,
        other_tenant,
        app,
    }
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    session: &str,
    body: Value,
) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("cookie", format!("session={session}"));
    let req = if body.is_null() {
        req.body(Body::empty()).unwrap()
    } else {
        req.header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn owner_endpoint(status: u16) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(status))
        .mount(&server)
        .await;
    server
}

async fn received(server: &MockServer) -> Vec<Value> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

async fn budget(f: &Fixture, scope: &str, cents: i64, primary: &str, backup: &str) {
    let (status, body) = call(
        &f.app,
        "PUT",
        "/v1/cost/budgets",
        "admin",
        json!({
            "cost_scope": scope,
            "budget_cents": cents,
            "primary_owner": "finops-payments@example.test",
            "backup_owner": "backup-finops-lead@example.test",
            "primary_url": primary,
            "backup_url": backup,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn finance_review_identifies_cost_ownership_and_isolates_unattributed_cost() {
    let f = fixture().await;
    for payload in [
        json!({ "service_name": "checkout-api", "owner_team": "payments-platform", "cost_center": "CC-4100", "expenditure_cents": 125_000, "period": "2026-11" }),
        json!({ "service_name": "billing-worker", "owner_team": "payments-platform", "cost_center": "CC-4100", "expenditure_cents": 75_000, "period": "2026-11" }),
        json!({ "service_name": "orphan-batch-ingest", "owner_team": null, "cost_center": " ", "expenditure_cents": 40_000, "period": "2026-11" }),
        // Ignored tenant field: the row lands in the caller's tenant.
        json!({ "tenant_id": f.other_tenant, "service_name": "search", "owner_team": "search", "cost_center": "CC-1", "expenditure_cents": 1, "period": "2026-10" }),
    ] {
        let (status, body) = call(&f.app, "POST", "/v1/cost/expenditures", "admin", payload).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }
    // Another tenant's spend never appears in this tenant's report.
    let (status, _) = call(
        &f.app,
        "POST",
        "/v1/cost/expenditures",
        "other",
        json!({ "service_name": "checkout-api", "owner_team": "x", "cost_center": "y", "expenditure_cents": 999, "period": "2026-11" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, report) = call(
        &f.app,
        "GET",
        "/v1/cost/attribution-report?period=2026-11",
        "member",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["tenant_id"], f.tenant.to_string());
    assert_eq!(report["attributed_total_cents"], 200_000);
    assert_eq!(report["unattributed_total_cents"], 40_000);
    assert_eq!(report["total_cents"], 240_000);
    assert_eq!(report["attributed"].as_array().unwrap().len(), 1);
    assert_eq!(report["attributed"][0]["owner_team"], "payments-platform");
    assert_eq!(report["attributed"][0]["cost_center"], "CC-4100");
    assert_eq!(
        report["attributed"][0]["services"],
        json!(["billing-worker", "checkout-api"])
    );
    // Unattributed cost is reported as unattributed, not distributed silently.
    assert_eq!(report["unattributed"].as_array().unwrap().len(), 1);
    assert_eq!(
        report["unattributed"][0]["service_name"],
        "orphan-batch-ingest"
    );
    assert_eq!(report["unattributed"][0]["expenditure_cents"], 40_000);

    let other_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM control.service_cost_attribution WHERE tenant_id = $1",
    )
    .bind(f.other_tenant)
    .fetch_one(&f.owner)
    .await
    .unwrap();
    assert_eq!(
        other_rows, 1,
        "the body tenant field wrote nothing to the other tenant"
    );

    // Members read; only tenant admins record spend.
    let (status, _) = call(
        &f.app,
        "POST",
        "/v1/cost/expenditures",
        "member",
        json!({ "service_name": "x", "expenditure_cents": 1, "period": "2026-11" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn controlled_budget_exception_reaches_designated_accountable_owner() {
    let f = fixture().await;
    let primary = owner_endpoint(202).await;
    let backup = owner_endpoint(202).await;
    budget(
        &f,
        "checkout-traces",
        100_000,
        &primary.uri(),
        &backup.uri(),
    )
    .await;

    let (status, result) = call(
        &f.app,
        "POST",
        "/v1/cost/budgets/evaluate",
        "admin",
        json!({ "cost_scope": "checkout-traces", "actual_cents": 135_000, "reason": "Controlled load-test trace exception" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["breached"], true);
    assert_eq!(result["delivery_status"], "delivered");
    assert_eq!(result["escalated_to_backup"], false);
    assert_eq!(result["recipient"], "finops-payments@example.test");
    let sent = received(&primary).await;
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["tenant_id"], f.tenant.to_string());
    assert_eq!(sent[0]["actual_cents"], 135_000);
    assert!(received(&backup).await.is_empty());

    // Within budget: nothing is sent.
    let (status, result) = call(
        &f.app,
        "POST",
        "/v1/cost/budgets/evaluate",
        "admin",
        json!({ "cost_scope": "checkout-traces", "actual_cents": 90_000, "reason": "r" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["delivery_status"], "within_budget");
    assert_eq!(received(&primary).await.len(), 1);
}

#[tokio::test]
async fn budget_breach_escalates_to_backup_owner_when_usual_recipient_is_absent() {
    let f = fixture().await;
    let primary = owner_endpoint(404).await;
    let backup = owner_endpoint(202).await;
    budget(&f, "platform-logs", 50_000, &primary.uri(), &backup.uri()).await;

    let (status, result) = call(
        &f.app,
        "POST",
        "/v1/cost/budgets/evaluate",
        "admin",
        json!({ "cost_scope": "platform-logs", "actual_cents": 92_000, "reason": "Debug logging spike in billing-worker" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["delivery_status"], "escalated_delivered");
    assert_eq!(result["escalated_to_backup"], true);
    assert_eq!(result["recipient"], "backup-finops-lead@example.test");
    assert_eq!(received(&backup).await.len(), 1);

    let (recipient, delivery, escalated, last_error): (String, String, bool, Option<String>) =
        sqlx::query_as(
            "SELECT recipient, delivery_status, escalated_to_backup, last_error \
         FROM control.cost_budget_alerts WHERE tenant_id = $1 AND cost_scope = $2",
        )
        .bind(f.tenant)
        .bind("platform-logs")
        .fetch_one(&f.owner)
        .await
        .unwrap();
    assert_eq!(recipient, "backup-finops-lead@example.test");
    assert_eq!(delivery, "escalated_delivered");
    assert!(escalated);
    assert_eq!(last_error.as_deref(), Some("target returned HTTP 404"));

    // Both owners unreachable: reported as failed (502) and recorded, never silent.
    let dead = owner_endpoint(503).await;
    budget(&f, "platform-metrics", 10, &dead.uri(), &dead.uri()).await;
    let (status, result) = call(
        &f.app,
        "POST",
        "/v1/cost/budgets/evaluate",
        "admin",
        json!({ "cost_scope": "platform-metrics", "actual_cents": 11, "reason": "r" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(result["delivery_status"], "failed");
    let failed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM control.cost_budget_alerts WHERE tenant_id = $1 AND delivery_status = 'failed'",
    )
    .bind(f.tenant)
    .fetch_one(&f.owner)
    .await
    .unwrap();
    assert_eq!(failed, 1);
}

#[tokio::test]
async fn budgets_are_tenant_scoped_admin_only_and_owner_urls_are_guarded() {
    let f = fixture().await;
    let primary = owner_endpoint(202).await;
    budget(&f, "scope", 1, &primary.uri(), &primary.uri()).await;

    // Another tenant cannot evaluate, and so cannot trigger, this tenant's budget.
    let (status, _) = call(
        &f.app,
        "POST",
        "/v1/cost/budgets/evaluate",
        "other",
        json!({ "cost_scope": "scope", "actual_cents": 5, "reason": "r" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // A member cannot change or trigger budgets.
    let (status, _) = call(
        &f.app,
        "POST",
        "/v1/cost/budgets/evaluate",
        "member",
        json!({ "cost_scope": "scope", "actual_cents": 5, "reason": "r" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // An evaluate request cannot redirect the warning: URLs in the body are ignored.
    let (status, _) = call(
        &f.app,
        "POST",
        "/v1/cost/budgets/evaluate",
        "admin",
        json!({ "cost_scope": "scope", "actual_cents": 5, "reason": "r", "primary_webhook_url": "http://169.254.169.254/" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(received(&primary).await.len(), 1);

    let scoped = scoped_pool(&f.owner, "observable_control").await;
    let strict = build_app(scoped, f.auth.uri(), TargetPolicy::default());
    for (url, code) in [
        ("http://hooks.example.com/x", "target_scheme_not_allowed"),
        (
            "https://169.254.169.254/latest/meta-data",
            "target_address_not_allowed",
        ),
        ("https://10.0.0.5/x", "target_address_not_allowed"),
    ] {
        let (status, body) = call(
            &strict,
            "PUT",
            "/v1/cost/budgets",
            "admin",
            json!({
                "cost_scope": "s2", "budget_cents": 1, "primary_owner": "a", "backup_owner": "b",
                "primary_url": url, "backup_url": "https://hooks.example.com/b"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{url}");
        assert_eq!(body["error"], code, "{url}");
    }
}
