use axum::{
    extract::{Extension, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::Value;
use uuid::Uuid;

use crate::auth::TenantContext;
use crate::queue::producer::build_envelope;

pub async fn export_metrics(
    State(state): State<crate::AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(body): Json<Value>,
) -> Response {
    if state.metric_rate_limiter.check_key(&ctx.tenant_id).is_err() {
        tracing::warn!(tenant_id = %ctx.tenant_id, "metric ingest rate limit exceeded");
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "1")],
            Json(serde_json::json!({
                "error": "rate_limit_exceeded",
                "message": "Metric ingest rate limit exceeded"
            })),
        )
            .into_response();
    }

    let resource_metrics = match body.get("resourceMetrics").and_then(|v| v.as_array()) {
        Some(s) => s,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    let (series, points) = match parse_otlp_metrics(&body, ctx.tenant_id) {
        Ok(m) => m,
        Err(status) => return status.into_response(),
    };

    state
        .metric_cardinality
        .observe(ctx.tenant_id, series.len());

    tracing::info!(
        tenant_id = %ctx.tenant_id,
        resource_count = resource_metrics.len(),
        series_count = series.len(),
        "received metrics export request"
    );

    if let Some(producer) = &state.producer {
        let envelope = build_envelope(
            ctx.tenant_id,
            domain::EnvelopePayload::Metrics { series, points },
        );
        if producer.publish(&envelope).await.is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    Json(serde_json::json!({})).into_response()
}

fn parse_otlp_metrics(
    body: &Value,
    tenant_id: Uuid,
) -> Result<(Vec<domain::MetricSeries>, Vec<domain::MetricPoint>), StatusCode> {
    let resource_metrics = body
        .get("resourceMetrics")
        .and_then(|v| v.as_array())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let mut all_series = Vec::new();
    let mut all_points = Vec::new();

    for rm in resource_metrics {
        let resource_attrs = rm
            .get("resource")
            .and_then(|r| r.get("attributes"))
            .cloned()
            .unwrap_or_default();
        let service_name = super::convert::extract_string_attr(&resource_attrs, "service.name")
            .unwrap_or_default();

        for scope_metrics in rm
            .get("scopeMetrics")
            .and_then(|v| v.as_array())
            .unwrap_or(&vec![])
        {
            for metric in scope_metrics
                .get("metrics")
                .and_then(|v| v.as_array())
                .unwrap_or(&vec![])
            {
                let metric_name = metric
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let series_id = Uuid::new_v4();

                let series = domain::MetricSeries {
                    tenant_id,
                    metric_series_id: series_id,
                    metric_name: metric_name.clone(),
                    service_name: service_name.clone(),
                    ..Default::default()
                };
                all_series.push(series);

                // Extract data points from sum/gauge/histogram
                let data_points = metric
                    .get("sum")
                    .or_else(|| metric.get("gauge"))
                    .or_else(|| metric.get("histogram"))
                    .and_then(|m| m.get("dataPoints"))
                    .and_then(|v| v.as_array());

                if let Some(dps) = data_points {
                    for dp in dps {
                        let time: u64 = dp
                            .get("timeUnixNano")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);
                        let value_double = dp.get("asDouble").and_then(|v| v.as_f64());
                        let value_int = dp.get("asInt").and_then(|v| v.as_i64());
                        all_points.push(domain::MetricPoint {
                            tenant_id,
                            metric_series_id: series_id,
                            metric_name: metric_name.clone(),
                            service_name: service_name.clone(),
                            time_unix_nano: time,
                            value_double,
                            value_int,
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }
    Ok((all_series, all_points))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use axum_test::TestServer;

    use crate::build_router;
    use crate::AppState;

    const TENANT: &str = "00000000-0000-0000-0000-000000000001";

    fn auth_header() -> (axum::http::HeaderName, axum::http::HeaderValue) {
        (
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("Bearer dev-api-key-0000"),
        )
    }

    fn two_series_payload() -> serde_json::Value {
        serde_json::json!({
            "resourceMetrics": [{
                "resource": {"attributes": [{"key": "service.name", "value": {"stringValue": "svc-a"}}]},
                "scopeMetrics": [{
                    "metrics": [
                        {"name": "http.requests", "sum": {"dataPoints": [{"timeUnixNano": "1000", "asDouble": 10.0}]}},
                        {"name": "http.errors",   "sum": {"dataPoints": [{"timeUnixNano": "1000", "asDouble": 2.0}]}}
                    ]
                }]
            }]
        })
    }

    #[tokio::test]
    async fn metrics_export_returns_200() {
        let app = build_router(AppState::with_stub_auth(TENANT));
        let server = TestServer::new(app).unwrap();
        let resp = server
            .post("/v1/metrics")
            .add_header(auth_header().0, auth_header().1)
            .json(&two_series_payload())
            .await;
        assert_eq!(resp.status_code(), StatusCode::OK);
    }

    #[tokio::test]
    async fn exceeding_rate_limit_returns_429() {
        let app = build_router(AppState::with_stub_auth_and_rate_limit(TENANT, 1));
        let server = TestServer::new(app).unwrap();

        let first = server
            .post("/v1/metrics")
            .add_header(auth_header().0, auth_header().1)
            .json(&two_series_payload())
            .await;
        assert_eq!(first.status_code(), StatusCode::OK);

        let second = server
            .post("/v1/metrics")
            .add_header(auth_header().0, auth_header().1)
            .json(&two_series_payload())
            .await;
        assert_eq!(second.status_code(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(second.headers()["retry-after"], "1");
        let body: serde_json::Value = second.json();
        assert_eq!(body["error"], "rate_limit_exceeded");
    }

    #[tokio::test]
    async fn metrics_export_updates_cardinality_counter() {
        let state = AppState::with_stub_auth(TENANT);
        let cardinality = state.metric_cardinality.clone();
        let tenant_id = uuid::Uuid::parse_str(TENANT).unwrap();

        let app = build_router(state);
        let server = TestServer::new(app).unwrap();
        server
            .post("/v1/metrics")
            .add_header(auth_header().0, auth_header().1)
            .json(&two_series_payload())
            .await;

        assert_eq!(cardinality.current_count(tenant_id), 2);
    }

    #[tokio::test]
    async fn metrics_export_above_budget_still_returns_200() {
        // Budget of 1; request carries 2 series — ingest must NOT be rejected.
        let state = AppState::with_stub_auth_and_metric_budget(TENANT, 1);
        let app = build_router(state);
        let server = TestServer::new(app).unwrap();
        let resp = server
            .post("/v1/metrics")
            .add_header(auth_header().0, auth_header().1)
            .json(&two_series_payload())
            .await;
        assert_eq!(resp.status_code(), StatusCode::OK);
    }

    #[tokio::test]
    async fn metrics_export_missing_auth_returns_401() {
        let app = build_router(AppState::test_stub());
        let server = TestServer::new(app).unwrap();
        let resp = server.post("/v1/metrics").json(&two_series_payload()).await;
        assert_eq!(resp.status_code(), StatusCode::UNAUTHORIZED);
    }
}
