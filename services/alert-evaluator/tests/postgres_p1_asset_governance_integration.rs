//! Phase 1 P1 observability asset governance integration tests (OBS-34-02, OBS-34-04, OBS-34-05).

use alert_evaluator::{
    AppState, alerts, asset_governance, middleware::auth::TenantContext, observability,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::{get, post, put},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn app(db: PgPool, tenant_id: Uuid, user_id: Option<Uuid>, role: &str) -> Router {
    let state = AppState {
        db,
        ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        auth_service_url: "http://auth-service:4319".into(),
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AlertEvaluatorMetrics::new()),
    };
    Router::new()
        .route("/v1/admin/alerts/rules", post(alerts::handle_create_rule))
        .route("/v1/alerts/rules", get(alerts::handle_list_rules))
        .route("/v1/alerts/rules/{rule_id}", get(alerts::handle_get_rule))
        .route(
            "/v1/alerts/rules/{rule_id}/threshold",
            put(asset_governance::handle_update_threshold),
        )
        .route(
            "/v1/alerts/rules/{rule_id}/versions",
            get(asset_governance::handle_list_rule_versions),
        )
        .route(
            "/v1/alerts/rules/{rule_id}/versions/{version_number}/restore",
            post(asset_governance::handle_restore_rule_version),
        )
        .route(
            "/v1/alerts/templates",
            post(asset_governance::handle_create_template),
        )
        .route(
            "/v1/alerts/templates/{template_id}/apply",
            post(asset_governance::handle_apply_template),
        )
        .route(
            "/v1/alerts/assets/review",
            get(asset_governance::handle_quarterly_asset_review),
        )
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id,
            role: role.into(),
        }))
        .with_state(state)
}

async fn call(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(match body {
            Some(b) => Body::from(b.to_string()),
            None => Body::empty(),
        })
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into()))
    };
    (status, json)
}

#[tokio::test]
async fn alert_threshold_change_can_be_compared_and_restored() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let router = app(db.clone(), tenant_id, Some(user_id), "tenant_admin");

    let (status, created) = call(
        &router,
        "POST",
        "/v1/admin/alerts/rules",
        Some(json!({
            "name": "High p99 latency",
            "metric_name": "http_request_duration_ms",
            "operator": "gt",
            "threshold": 250.0,
            "severity": "critical",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let rule_id = created["rule_id"].as_str().unwrap();

    // Update threshold from 250 -> 500
    let (status, updated) = call(
        &router,
        "PUT",
        &format!("/v1/alerts/rules/{rule_id}/threshold"),
        Some(json!({"threshold": 500.0, "reason": "relax during migration"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["version_number"], 2);

    // Compare versions
    let (status, versions) = call(
        &router,
        "GET",
        &format!("/v1/alerts/rules/{rule_id}/versions"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{versions}");
    let items = versions["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["current_threshold"], 250.0);
    assert_eq!(items[1]["previous_threshold"], 250.0);
    assert_eq!(items[1]["current_threshold"], 500.0);

    // Restore version 1
    let (status, restored) = call(
        &router,
        "POST",
        &format!("/v1/alerts/rules/{rule_id}/versions/1/restore"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{restored}");
    assert_eq!(restored["version_number"], 3);
    assert_eq!(restored["condition"]["threshold"], 250.0);

    let (status, rule) = call(&router, "GET", &format!("/v1/alerts/rules/{rule_id}"), None).await;
    assert_eq!(status, StatusCode::OK, "{rule}");
    assert_eq!(rule["condition"]["threshold"], 250.0);
}

#[tokio::test]
async fn approved_template_applies_standard_critical_alerts_to_service() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let router = app(db.clone(), tenant_id, Some(user_id), "tenant_admin");

    let (status, tmpl) = call(
        &router,
        "POST",
        "/v1/alerts/templates",
        Some(json!({
            "name": "standard-critical-service",
            "approved": true,
            "rules": [
                {
                    "name": "Error rate > 5%",
                    "alert_type": "threshold",
                    "severity": "critical",
                    "condition": {"metric_name": "error_rate", "operator": "gt", "threshold": 0.05}
                },
                {
                    "name": "Latency p99 > 500ms",
                    "alert_type": "threshold",
                    "severity": "critical",
                    "condition": {"metric_name": "latency_p99_ms", "operator": "gt", "threshold": 500.0}
                }
            ]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{tmpl}");
    let template_id = tmpl["template_id"].as_str().unwrap();

    let (status, applied) = call(
        &router,
        "POST",
        &format!("/v1/alerts/templates/{template_id}/apply"),
        Some(json!({"service_name": "orders-api", "environment": "production"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{applied}");
    assert_eq!(applied["created_rule_ids"].as_array().unwrap().len(), 2);

    let (status, list) = call(&router, "GET", "/v1/alerts/rules", None).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(list["items"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn quarterly_review_lists_unviewed_dashboards_and_never_triggered_alerts_with_owners() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let tenant_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let router = app(db.clone(), tenant_id, Some(owner_id), "tenant_admin");

    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(tenant_id)
        .bind(format!("review-{tenant_id}"))
        .execute(&db)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO dashboards (tenant_id, name, owner_kind, owner_id, last_viewed_at) \
         VALUES ($1, 'Legacy Overview', 'user', $2, NULL)",
    )
    .bind(tenant_id)
    .bind(owner_id)
    .execute(&db)
    .await
    .unwrap();

    let (status, rule) = call(
        &router,
        "POST",
        "/v1/admin/alerts/rules",
        Some(json!({
            "name": "Unused Alert",
            "metric_name": "unused_metric",
            "operator": "gt",
            "threshold": 10.0,
            "severity": "warning",
            "owner": {"principal_kind": "user", "principal_id": owner_id},
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{rule}");

    let (status, review) = call(&router, "GET", "/v1/alerts/assets/review", None).await;
    assert_eq!(status, StatusCode::OK, "{review}");
    let unviewed = review["unviewed_dashboards"].as_array().unwrap();
    assert_eq!(unviewed.len(), 1);
    assert_eq!(unviewed[0]["name"], "Legacy Overview");
    assert_eq!(unviewed[0]["owner_id"], owner_id.to_string());

    let never_triggered = review["never_triggered_alerts"].as_array().unwrap();
    assert_eq!(never_triggered.len(), 1);
    assert_eq!(never_triggered[0]["name"], "Unused Alert");
    assert_eq!(never_triggered[0]["owner_id"], owner_id.to_string());
}
