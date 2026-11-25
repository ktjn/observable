// W5a (docs/assurance-relocations.md) through the routes as the scoped `observable_control`
// login: partial release comparison (OBS-11-08), change freezes and exceptions (OBS-11-09), and
// the internal cost totals the benefits review reads (OBS-17-06).

use admin_service::{
    AdminServiceAppState, change_assurance, middleware::auth::TenantContext, observability,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::{get, post},
};
use chrono::{DateTime, Duration, Utc};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn app(db: PgPool, ch: clickhouse::Client, ctx: TenantContext) -> Router {
    let state = AdminServiceAppState {
        db,
        ch,
        auth_service_url: "http://auth-service:4319".into(),
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AdminServiceMetrics::new()),
        producer: None,
        change_producer: None,
    };
    let internal = Router::new().route(
        "/internal/cost/totals",
        get(change_assurance::handle_internal_cost_totals),
    );
    Router::new()
        .route(
            "/v1/deployments/{deployment_id}/rollout-comparison",
            get(change_assurance::handle_rollout_comparison),
        )
        .route(
            "/v1/deployments/{deployment_id}/freeze-exception",
            post(change_assurance::handle_freeze_exception),
        )
        .route(
            "/v1/change-freezes",
            post(change_assurance::handle_create_freeze),
        )
        .route(
            "/v1/change-freezes/{freeze_id}/changes",
            get(change_assurance::handle_freeze_changes),
        )
        .layer(axum::Extension(ctx))
        .merge(internal)
        .with_state(state)
}

fn admin(tenant: Uuid, user: Uuid) -> TenantContext {
    TenantContext {
        tenant_id: tenant,
        user_id: Some(user),
        role: "tenant_admin".into(),
    }
}

async fn call(app: Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn pools() -> (PgPool, PgPool, test_support::postgres::TestDatabase) {
    let (owner, db) = test_support::postgres::isolated_pool().await;
    let scoped = test_support::postgres::scoped_pool(&owner, "observable_control").await;
    (owner, scoped, db)
}

#[allow(clippy::too_many_arguments)]
async fn deployment(
    owner: &PgPool,
    tenant: Uuid,
    service: &str,
    env: &str,
    version: &str,
    started_at: DateTime<Utc>,
    deployed_by: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO control.deployment_markers \
         (deployment_id, tenant_id, service_name, environment, service_version, status, started_at, deployed_by) \
         VALUES ($1, $2, $3, $4, $5, 'success', $6, $7)",
    )
    .bind(id)
    .bind(tenant)
    .bind(service)
    .bind(env)
    .bind(version)
    .bind(started_at)
    .bind(deployed_by)
    .execute(owner)
    .await
    .unwrap();
    id
}

/// `n` spans for one version, `errors` of them failed, each lasting `duration_ms`.
#[allow(clippy::too_many_arguments)]
async fn spans(
    ch: &clickhouse::Client,
    tenant: Uuid,
    service: &str,
    version: &str,
    at: DateTime<Utc>,
    n: u64,
    errors: u64,
    duration_ms: u64,
) {
    let start = at.timestamp_nanos_opt().unwrap() as u64;
    ch.query(
        "INSERT INTO observable.spans (tenant_id, trace_id, span_id, parent_span_id, service_name, \
         operation_name, start_time_unix_nano, end_time_unix_nano, duration_ns, environment, \
         status_code, service_version) \
         SELECT ?, toString(generateUUIDv4()), toString(number), NULL, ?, 'GET /checkout', \
                ? + number, ? + number + ?, ?, 'prod', if(number < ?, 'ERROR', 'OK'), ? \
         FROM numbers(?)",
    )
    .bind(tenant)
    .bind(service)
    .bind(start)
    .bind(start)
    .bind(duration_ms * 1_000_000)
    .bind(duration_ms * 1_000_000)
    .bind(errors)
    .bind(version)
    .bind(n)
    .execute()
    .await
    .unwrap();
}

#[tokio::test]
async fn rollout_comparison_separates_exposed_from_unexposed_and_never_reports_zero() {
    let ch = test_support::clickhouse::shared_client().await;
    let (owner, scoped, _db) = pools().await;
    let (tenant, other) = (Uuid::new_v4(), Uuid::new_v4());
    let started = Utc::now() - Duration::minutes(30);
    let dep = deployment(&owner, tenant, "checkout", "prod", "v2", started, "ci").await;
    let at = started + Duration::minutes(5);
    spans(&ch, tenant, "checkout", "v2", at, 200, 20, 300).await;
    spans(&ch, tenant, "checkout", "v1", at, 400, 4, 100).await;
    // Another tenant's traffic on the same service and version is never counted.
    spans(&ch, other, "checkout", "v2", at, 500, 500, 900).await;
    // Traffic before the deployment started is outside the window.
    spans(
        &ch,
        tenant,
        "checkout",
        "v2",
        started - Duration::minutes(5),
        300,
        300,
        900,
    )
    .await;

    let a = app(scoped.clone(), ch.clone(), admin(tenant, Uuid::new_v4()));
    let (status, body) = call(
        a.clone(),
        "GET",
        &format!("/v1/deployments/{dep}/rollout-comparison?window_minutes=60"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["service_version"], "v2");
    let exposed = &body["exposed"];
    assert_eq!(exposed["basis"], "measured");
    assert_eq!(exposed["requests"], 200);
    assert!((exposed["error_rate"].as_f64().unwrap() - 0.1).abs() < 1e-9);
    assert!((exposed["p95_latency_ms"].as_f64().unwrap() - 300.0).abs() < 1.0);
    let unexposed = &body["unexposed"];
    assert_eq!(unexposed["requests"], 400);
    assert!((unexposed["error_rate"].as_f64().unwrap() - 0.01).abs() < 1e-9);
    assert!((unexposed["p95_latency_ms"].as_f64().unwrap() - 100.0).abs() < 1.0);

    // Negative: a population below the minimum sample is insufficient_data, not zero.
    let canary = deployment(&owner, tenant, "basket", "prod", "v9", started, "ci").await;
    spans(&ch, tenant, "basket", "v9", at, 20, 20, 50).await;
    spans(&ch, tenant, "basket", "v8", at, 150, 0, 50).await;
    let (status, body) = call(
        a.clone(),
        "GET",
        &format!("/v1/deployments/{canary}/rollout-comparison"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["exposed"]["basis"], "insufficient_data");
    assert_eq!(body["exposed"]["requests"], 20);
    assert!(body["exposed"].get("error_rate").is_none());
    assert_eq!(body["unexposed"]["basis"], "measured");

    let (status, _) = call(
        a,
        "GET",
        &format!("/v1/deployments/{dep}/rollout-comparison?window_minutes=0"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let foreign = app(scoped, ch, admin(other, Uuid::new_v4()));
    let (status, _) = call(
        foreign,
        "GET",
        &format!("/v1/deployments/{dep}/rollout-comparison"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn freeze_lists_every_in_scope_change_with_its_exception_or_unapproved() {
    let ch = test_support::clickhouse::shared_client().await;
    let (owner, scoped, _db) = pools().await;
    let tenant = Uuid::new_v4();
    let (deployer, approver) = (Uuid::new_v4(), Uuid::new_v4());
    let now = Utc::now();
    let freeze_body = json!({
        "name": "Year-end freeze", "environment": "prod", "services": ["checkout"],
        "starts_at": now - Duration::hours(2), "ends_at": now + Duration::hours(1),
    });

    let viewer = TenantContext {
        tenant_id: tenant,
        user_id: Some(Uuid::new_v4()),
        role: "viewer".into(),
    };
    let (status, _) = call(
        app(scoped.clone(), ch.clone(), viewer),
        "POST",
        "/v1/change-freezes",
        Some(freeze_body.clone()),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "only a tenant admin approves a freeze"
    );

    let by_approver = app(scoped.clone(), ch.clone(), admin(tenant, approver));
    let (status, body) = call(
        by_approver.clone(),
        "POST",
        "/v1/change-freezes",
        Some(freeze_body),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["approved_by"], approver.to_string());
    let freeze = body["freeze_id"].as_str().unwrap().to_string();

    let during = now - Duration::hours(1);
    let by = format!("user:{deployer}");
    let excepted = deployment(&owner, tenant, "checkout", "prod", "v2", during, &by).await;
    let unapproved = deployment(&owner, tenant, "checkout", "prod", "v3", during, &by).await;
    let other_service = deployment(&owner, tenant, "payments", "prod", "v7", during, &by).await;
    let _staging = deployment(&owner, tenant, "checkout", "staging", "v2", during, &by).await;
    let _before = deployment(
        &owner,
        tenant,
        "checkout",
        "prod",
        "v1",
        now - Duration::hours(3),
        &by,
    )
    .await;

    let exception = |reason: &str| Some(json!({"freeze_id": freeze, "reason": reason}));
    let uri = |d: Uuid| format!("/v1/deployments/{d}/freeze-exception");
    let by_deployer = app(scoped.clone(), ch.clone(), admin(tenant, deployer));
    let (status, body) = call(by_deployer, "POST", &uri(excepted), exception("hotfix")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], "deployer_cannot_approve_own_exception");
    let (status, _) = call(by_approver.clone(), "POST", &uri(excepted), exception("  ")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, body) = call(
        by_approver.clone(),
        "POST",
        &uri(excepted),
        exception("payment outage hotfix"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, _) = call(
        by_approver.clone(),
        "POST",
        &uri(excepted),
        exception("again"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, body) = call(
        by_approver.clone(),
        "POST",
        &uri(other_service),
        exception("x"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "deployment_not_in_freeze");

    let (status, body) = call(
        by_approver,
        "GET",
        &format!("/v1/change-freezes/{freeze}/changes"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let changes = body["changes"].as_array().unwrap();
    let ids: Vec<&str> = changes
        .iter()
        .map(|c| c["deployment_id"].as_str().unwrap())
        .collect();
    let mut expected = vec![excepted.to_string(), unapproved.to_string()];
    expected.sort();
    let mut got: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
    got.sort();
    assert_eq!(
        got, expected,
        "only in-scope changes during the freeze are listed"
    );
    for c in changes {
        if c["deployment_id"] == excepted.to_string() {
            assert_eq!(c["approval"], "exception");
            assert_eq!(c["exception"]["approved_by"], approver.to_string());
            assert_eq!(c["exception"]["reason"], "payment outage hotfix");
        } else {
            assert_eq!(c["approval"], "unapproved");
            assert!(c["exception"].is_null());
        }
    }

    let foreign = app(scoped, ch, admin(Uuid::new_v4(), approver));
    let (status, _) = call(
        foreign,
        "GET",
        &format!("/v1/change-freezes/{freeze}/changes"),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "another tenant never sees the freeze"
    );
}

#[tokio::test]
async fn internal_cost_totals_sum_one_tenant_and_period_and_are_null_when_unrecorded() {
    let ch = test_support::clickhouse::shared_client().await;
    let (owner, scoped, _db) = pools().await;
    let (tenant, other) = (Uuid::new_v4(), Uuid::new_v4());
    for (t, service, cents, period) in [
        (tenant, "checkout", 1_000_i64, "2026-10"),
        (tenant, "payments", 500, "2026-10"),
        (tenant, "checkout", 9_000, "2026-11"),
        (other, "checkout", 7_777, "2026-10"),
    ] {
        sqlx::query(
            "INSERT INTO control.service_cost_attribution \
             (tenant_id, service_name, expenditure_cents, period) VALUES ($1, $2, $3, $4)",
        )
        .bind(t)
        .bind(service)
        .bind(cents)
        .bind(period)
        .execute(&owner)
        .await
        .unwrap();
    }
    let a = app(scoped, ch, admin(tenant, Uuid::new_v4()));
    let (status, body) = call(
        a.clone(),
        "GET",
        &format!("/internal/cost/totals?tenant_id={tenant}&period=2026-10"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total_cents"], 1_500);
    let (_, body) = call(
        a,
        "GET",
        &format!("/internal/cost/totals?tenant_id={tenant}&period=2026-09"),
        None,
    )
    .await;
    assert!(
        body["total_cents"].is_null(),
        "no record is not zero: {body}"
    );
}
