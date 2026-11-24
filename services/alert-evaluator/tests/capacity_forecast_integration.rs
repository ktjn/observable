//! Capacity forecast warnings (OBS-07-12) through the rule routes and the evaluator, with
//! real PostgreSQL (as the alerting login) and real ClickHouse metric points.

use std::sync::Arc;

use alert_evaluator::evaluator::eval_capacity_forecast_rules;
use alert_evaluator::{
    AppState, alerts, catalog_owner::CatalogOwnerPort, middleware::auth::TenantContext,
    observability,
};
use axum::{
    Extension, Router,
    body::Body,
    http::Request,
    routing::{get, post},
};
use http_body_util::BodyExt;
use observable_storage_contracts::MetricPointRow;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

struct World {
    state: AppState,
    tenant: Uuid,
    _db: test_support::postgres::TestDatabase,
}

impl World {
    async fn start() -> Self {
        let (owner, guard) = test_support::postgres::isolated_pool().await;
        let db = test_support::postgres::scoped_pool(&owner, "observable_alerting").await;
        Self {
            state: AppState {
                db,
                ch: test_support::clickhouse::shared_client().await,
                auth_service_url: "http://unused".into(),
                http_client: reqwest::Client::new(),
                metrics: Arc::new(observability::AlertEvaluatorMetrics::new()),
                producer: None,
            },
            tenant: Uuid::new_v4(),
            _db: guard,
        }
    }

    async fn call(&self, role: &str, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
        let app = Router::new()
            .route("/v1/admin/alerts/rules", post(alerts::handle_create_rule))
            .route("/v1/alerts/rules/{rule_id}", get(alerts::handle_get_rule))
            .layer(Extension(TenantContext {
                tenant_id: self.tenant,
                user_id: Some(Uuid::new_v4()),
                role: role.into(),
            }))
            .layer(Extension(CatalogOwnerPort::Disabled))
            .with_state(self.state.clone());
        let mut req = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(b) => {
                req = req.header("content-type", "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let resp = app.oneshot(req.body(body).unwrap()).await.unwrap();
        let status = resp.status().as_u16();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// One point per 5 minutes over the last hour: `start` rising by `step` per point.
    async fn series(&self, metric: &str, start: f64, step: f64) {
        let now = chrono::Utc::now().timestamp_nanos_opt().unwrap() as u64;
        let mut insert = self
            .state
            .ch
            .insert::<MetricPointRow>("metric_points")
            .await
            .unwrap();
        for i in 0..12u64 {
            let age = (55 - i * 5) * 60 * 1_000_000_000;
            insert
                .write(&MetricPointRow {
                    tenant_id: self.tenant,
                    metric_series_id: Uuid::new_v4(),
                    metric_name: metric.into(),
                    service_name: "storage".into(),
                    time_unix_nano: now - age,
                    start_time_unix_nano: None,
                    value_double: Some(start + step * i as f64),
                    value_int: None,
                    histogram_count: None,
                    histogram_sum: None,
                    histogram_bucket_counts: vec![],
                    histogram_explicit_bounds: vec![],
                })
                .await
                .unwrap();
        }
        insert.end().await.unwrap();
    }

    async fn rule(&self, metric: &str, horizon_secs: i64) -> Uuid {
        let (status, body) = self
            .call(
                "tenant_admin",
                "POST",
                "/v1/admin/alerts/rules",
                Some(json!({
                    "name": format!("{metric} capacity"),
                    "alert_type": "capacity_forecast",
                    "metric_name": metric,
                    "threshold": 90.0,
                    "lookback_secs": 3600,
                    "horizon_secs": horizon_secs,
                })),
            )
            .await;
        assert_eq!(status, 201, "{body}");
        body["rule_id"].as_str().unwrap().parse().unwrap()
    }

    async fn firings(&self, rule: Uuid) -> Vec<Value> {
        let (status, body) = self
            .call("viewer", "GET", &format!("/v1/alerts/rules/{rule}"), None)
            .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["alert_type"], "capacity_forecast");
        body["firings"].as_array().unwrap().clone()
    }
}

fn metric() -> String {
    format!("disk_used_percent_{}", Uuid::new_v4().simple())
}

#[tokio::test]
async fn forecast_capacity_breach_raises_a_warning_labeled_forecast_with_its_basis() {
    let w = World::start().await;
    // 50% rising 2 points per 5 minutes (24%/h): about 96% one hour ahead.
    let rising = metric();
    w.series(&rising, 50.0, 2.0).await;
    let warned = w.rule(&rising, 3600).await;
    // The same trend over 10 minutes stays below 90%; a flat series never warns.
    let near = w.rule(&rising, 600).await;
    let flat = metric();
    w.series(&flat, 70.0, 0.0).await;
    let steady = w.rule(&flat, 3600).await;

    eval_capacity_forecast_rules(&w.state.db, &w.state.ch)
        .await
        .unwrap();

    let firings = w.firings(warned).await;
    assert_eq!(firings.len(), 1, "{firings:?}");
    let f = &firings[0];
    assert_eq!(f["state"], "active");
    let forecast = &f["forecast"];
    assert_eq!(forecast["label"], "forecast");
    assert_eq!(forecast["threshold"], 90.0);
    assert_eq!(forecast["horizon_secs"], 3600);
    let predicted = forecast["predicted_value"].as_f64().unwrap();
    assert!((predicted - 96.0).abs() < 1.0, "{forecast}");
    assert_eq!(f["value"].as_f64().unwrap(), predicted);
    let current = forecast["current_value"].as_f64().unwrap();
    assert!(
        current < 90.0,
        "a forecast warns before the breach: {forecast}"
    );
    let eta = forecast["seconds_to_threshold"].as_i64().unwrap();
    assert!((0..3600).contains(&eta), "{forecast}");
    let basis = &forecast["basis"];
    assert_eq!(basis["method"], "linear_least_squares");
    assert_eq!(basis["points"], 12);
    assert_eq!(basis["lookback_secs"], 3600);
    assert!((basis["slope_per_hour"].as_f64().unwrap() - 24.0).abs() < 0.5);
    assert!(basis["r_squared"].as_f64().unwrap() > 0.99);

    assert!(w.firings(near).await.is_empty());
    assert!(w.firings(steady).await.is_empty());
}

#[tokio::test]
async fn forecast_rules_need_an_admin_and_a_bounded_history_and_horizon() {
    let w = World::start().await;
    let base = json!({
        "name": "disk capacity", "alert_type": "capacity_forecast",
        "metric_name": "disk_used_percent", "threshold": 90.0,
        "lookback_secs": 3600, "horizon_secs": 3600,
    });
    let (status, _) = w
        .call(
            "member",
            "POST",
            "/v1/admin/alerts/rules",
            Some(base.clone()),
        )
        .await;
    assert_eq!(status, 403);
    for bad in [
        json!({"lookback_secs": null}),
        json!({"horizon_secs": 30}),
        json!({"horizon_secs": 31 * 86_400}),
        json!({"metric_name": " "}),
    ] {
        let mut body = base.clone();
        body.as_object_mut()
            .unwrap()
            .extend(bad.as_object().unwrap().clone());
        let (status, _) = w
            .call("tenant_admin", "POST", "/v1/admin/alerts/rules", Some(body))
            .await;
        assert_eq!(status, 400, "{bad}");
    }
}
