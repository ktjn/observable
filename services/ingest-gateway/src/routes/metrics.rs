use axum::{extract::{Extension, State}, http::StatusCode, Json};
use serde_json::Value;
use uuid::Uuid;

use crate::auth::TenantContext;
use crate::queue::producer::build_envelope;

pub async fn export_metrics(
    State(state): State<crate::AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let resource_metrics = body
        .get("resourceMetrics")
        .and_then(|v| v.as_array())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let (series, points) = parse_otlp_metrics(&body, ctx.tenant_id)?;

    tracing::info!(
        tenant_id = %ctx.tenant_id,
        resource_count = resource_metrics.len(),
        "received metrics export request"
    );

    if let Some(producer) = &state.producer {
        let envelope = build_envelope(
            ctx.tenant_id,
            domain::EnvelopePayload::Metrics { series, points },
        );
        producer
            .publish(&envelope)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    Ok(Json(serde_json::json!({})))
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
        let service_name = extract_string_attr(&resource_attrs, "service.name")
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
                        let value_double = dp
                            .get("asDouble")
                            .and_then(|v| v.as_f64());
                        let value_int = dp
                            .get("asInt")
                            .and_then(|v| v.as_i64());
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

fn extract_string_attr(attrs: &Value, key: &str) -> Option<String> {
    attrs.as_array()?.iter().find(|a| {
        a.get("key").and_then(|k| k.as_str()) == Some(key)
    })?.get("value")?.get("stringValue")?.as_str().map(String::from)
}
