use axum::http::StatusCode;
use serde_json::Value;
use uuid::Uuid;

pub fn extract_string_attr(attrs: &Value, key: &str) -> Option<String> {
    attrs
        .as_array()?
        .iter()
        .find(|a| a.get("key").and_then(|k| k.as_str()) == Some(key))?
        .get("value")?
        .get("stringValue")?
        .as_str()
        .map(String::from)
}

pub fn parse_otlp_logs(
    body: &Value,
    tenant_id: Uuid,
) -> Result<Vec<domain::LogRecord>, StatusCode> {
    let resource_logs = body
        .get("resourceLogs")
        .and_then(|v| v.as_array())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let mut logs = Vec::new();
    for rl in resource_logs {
        let resource_attrs = rl
            .get("resource")
            .and_then(|r| r.get("attributes"))
            .cloned()
            .unwrap_or_default();
        let service_name = extract_string_attr(&resource_attrs, "service.name").unwrap_or_default();
        for scope_logs in rl
            .get("scopeLogs")
            .and_then(|v| v.as_array())
            .unwrap_or(&vec![])
        {
            for lr in scope_logs
                .get("logRecords")
                .and_then(|v| v.as_array())
                .unwrap_or(&vec![])
            {
                logs.push(domain::LogRecord {
                    tenant_id,
                    log_id: Uuid::new_v4(),
                    timestamp_unix_nano: lr
                        .get("timeUnixNano")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0),
                    observed_timestamp_unix_nano: 0,
                    severity_number: lr
                        .get("severityNumber")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as i32,
                    severity_text: lr
                        .get("severityText")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .into(),
                    body: lr.get("body").cloned().unwrap_or(Value::Null),
                    service_name: service_name.clone(),
                    ..Default::default()
                });
            }
        }
    }
    Ok(logs)
}

pub fn parse_otlp_metrics(
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
        let service_name = extract_string_attr(&resource_attrs, "service.name").unwrap_or_default();

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

pub fn parse_otlp_traces(body: &Value, tenant_id: Uuid) -> Result<Vec<domain::Span>, StatusCode> {
    let resource_spans = body
        .get("resourceSpans")
        .and_then(|v| v.as_array())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let mut spans = Vec::new();
    for rs in resource_spans {
        let resource_attrs = rs
            .get("resource")
            .and_then(|r| r.get("attributes"))
            .cloned()
            .unwrap_or_default();
        let service_name = extract_string_attr(&resource_attrs, "service.name").unwrap_or_default();
        for scope_spans in rs
            .get("scopeSpans")
            .and_then(|v| v.as_array())
            .unwrap_or(&vec![])
        {
            for s in scope_spans
                .get("spans")
                .and_then(|v| v.as_array())
                .unwrap_or(&vec![])
            {
                let start: u64 = s
                    .get("startTimeUnixNano")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let end: u64 = s
                    .get("endTimeUnixNano")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                spans.push(domain::Span {
                    tenant_id,
                    trace_id: s
                        .get("traceId")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .into(),
                    span_id: s
                        .get("spanId")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .into(),
                    service_name: service_name.clone(),
                    operation_name: s
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .into(),
                    start_time_unix_nano: start,
                    end_time_unix_nano: end,
                    duration_ns: end.saturating_sub(start),
                    ..Default::default()
                });
            }
        }
    }
    Ok(spans)
}
