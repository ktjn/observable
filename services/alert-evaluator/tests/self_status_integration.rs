//! Alerting self-status (phase 2 E5) through the internal router with the real evaluation
//! worker loop against real PostgreSQL and ClickHouse.

use std::sync::Arc;
use std::time::Duration;

use alert_evaluator::middleware::auth::{InternalServiceToken, require_internal_service};
use alert_evaluator::self_status::{EvalStatus, handle_self_status};
use axum::{Extension, Router, body::Body, http::Request, middleware, routing::get};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

const TOKEN: &str = "test-internal-token";

fn router(status: Arc<EvalStatus>) -> Router {
    Router::new()
        .route("/internal/alerting/self-status", get(handle_self_status))
        .layer(middleware::from_fn(require_internal_service))
        .layer(Extension(status))
        .layer(Extension(InternalServiceToken(TOKEN.into())))
}

async fn read(app: &Router, token: Option<&str>) -> (u16, Value) {
    let mut req = Request::get("/internal/alerting/self-status");
    if let Some(t) = token {
        req = req.header("X-Internal-Token", t);
    }
    let resp = app
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn wait_for(app: &Router, state: &str) -> Value {
    let mut body = Value::Null;
    for _ in 0..60 {
        body = read(app, Some(TOKEN)).await.1;
        if body["state"] == state {
            return body;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("never reached {state}: {body}");
}

#[tokio::test]
async fn a_paused_evaluator_is_visible_as_a_stale_pass() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let ch = test_support::clickhouse::shared_client().await;
    let status = EvalStatus::new();
    let app = router(status.clone());

    assert_eq!(read(&app, None).await.0, 401);
    assert_eq!(read(&app, Some("wrong")).await.0, 401);
    let (code, body) = read(&app, Some(TOKEN)).await;
    assert_eq!((code, body["state"].as_str()), (200, Some("not_started")));

    let worker = tokio::spawn(alert_evaluator::evaluator::start_eval_worker(
        db.clone(),
        ch,
        Duration::from_secs(1),
        status.clone(),
    ));
    let body = wait_for(&app, "current").await;
    assert_eq!(body["function"], "detect");
    assert_eq!(body["interval_seconds"], 1);
    assert_eq!(body["consecutive_failures"], 0);

    // Paused: within twice the interval the last pass is stale.
    worker.abort();
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    assert_eq!(read(&app, Some(TOKEN)).await.1["state"], "stale");
}

#[tokio::test]
async fn failing_passes_never_keep_the_evaluator_current() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let ch = test_support::clickhouse::shared_client().await;
    let status = EvalStatus::new();
    let app = router(status.clone());
    // The rule store is gone: every pass fails.
    db.close().await;
    let worker = tokio::spawn(alert_evaluator::evaluator::start_eval_worker(
        db,
        ch,
        Duration::from_secs(1),
        status.clone(),
    ));
    let mut body = Value::Null;
    for _ in 0..60 {
        body = read(&app, Some(TOKEN)).await.1;
        if body["consecutive_failures"].as_u64().unwrap_or(0) >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        body["consecutive_failures"].as_u64().unwrap() >= 2,
        "{body}"
    );
    assert_eq!(body["state"], "not_started");
    assert!(!body["last_failure_at"].is_null());
    worker.abort();
}
