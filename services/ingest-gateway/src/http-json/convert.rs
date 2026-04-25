use axum::http::StatusCode;
use serde_json::{Map, Value};
use std::collections::HashMap;
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

/// Parse a uint64 nanosecond timestamp from an OTLP/HTTP JSON value.
///
/// The OTLP/HTTP JSON spec allows uint64 fields such as `timeUnixNano` to be
/// encoded as either a decimal string or a bare JSON number.  Both are accepted.
fn parse_nano_timestamp(v: &Value) -> u64 {
    // String form: "1745606400000000000"
    if let Some(s) = v.as_str() {
        if let Ok(n) = s.parse::<u64>() {
            return n;
        }
    }
    // Number form: 1745606400000000000
    v.as_u64().unwrap_or(0)
}

/// Convert an OTLP AnyValue JSON object to a plain `serde_json::Value`.
///
/// OTLP JSON encodes AnyValue as `{"stringValue":"..."}`, `{"intValue":"123"}`,
/// etc.  This mirrors the gRPC `any_value_to_json` conversion so that stored
/// body / attribute values are consistent across transports.
fn extract_otlp_any_value(v: &Value) -> Value {
    if let Some(s) = v.get("stringValue").and_then(|x| x.as_str()) {
        return Value::String(s.to_owned());
    }
    if let Some(i) = v.get("intValue") {
        // intValue may be a string in proto3 JSON (uint64 overflow safety)
        if let Some(n) = i
            .as_str()
            .and_then(|s| s.parse::<i64>().ok())
            .or_else(|| i.as_i64())
        {
            return Value::Number(n.into());
        }
    }
    if let Some(d) = v.get("doubleValue").and_then(|x| x.as_f64()) {
        if let Some(n) = serde_json::Number::from_f64(d) {
            return Value::Number(n);
        }
    }
    if let Some(b) = v.get("boolValue").and_then(|x| x.as_bool()) {
        return Value::Bool(b);
    }
    if let Some(arr) = v
        .get("arrayValue")
        .and_then(|x| x.get("values"))
        .and_then(|x| x.as_array())
    {
        return Value::Array(arr.iter().map(extract_otlp_any_value).collect());
    }
    if let Some(kvlist) = v
        .get("kvlistValue")
        .and_then(|x| x.get("values"))
        .and_then(|x| x.as_array())
    {
        let mut map = Map::new();
        for kv in kvlist {
            if let (Some(k), Some(val)) = (kv.get("key").and_then(|x| x.as_str()), kv.get("value"))
            {
                map.insert(k.to_owned(), extract_otlp_any_value(val));
            }
        }
        return Value::Object(map);
    }
    // Unknown / null
    Value::Null
}

/// Convert an OTLP attributes array `[{"key":"k","value":{…}}, …]` to a
/// `HashMap` for storage in `LogRecord`, `Span`, etc.
fn otlp_attrs_to_map(attrs: &Value) -> HashMap<String, Value> {
    let mut map = HashMap::new();
    if let Some(arr) = attrs.as_array() {
        for kv in arr {
            if let (Some(k), Some(val)) = (kv.get("key").and_then(|x| x.as_str()), kv.get("value"))
            {
                map.insert(k.to_owned(), extract_otlp_any_value(val));
            }
        }
    }
    map
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
        let raw_resource_attrs = rl
            .get("resource")
            .and_then(|r| r.get("attributes"))
            .cloned()
            .unwrap_or_default();
        let service_name =
            extract_string_attr(&raw_resource_attrs, "service.name").unwrap_or_default();
        let resource_attributes = otlp_attrs_to_map(&raw_resource_attrs);
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
                let timestamp_unix_nano = lr
                    .get("timeUnixNano")
                    .map(parse_nano_timestamp)
                    .unwrap_or(0);
                let observed_timestamp_unix_nano = lr
                    .get("observedTimeUnixNano")
                    .map(parse_nano_timestamp)
                    .unwrap_or(timestamp_unix_nano);
                logs.push(domain::LogRecord {
                    tenant_id,
                    log_id: Uuid::new_v4(),
                    timestamp_unix_nano,
                    observed_timestamp_unix_nano,
                    severity_number: lr
                        .get("severityNumber")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as i32,
                    severity_text: lr
                        .get("severityText")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .into(),
                    body: lr
                        .get("body")
                        .map(extract_otlp_any_value)
                        .unwrap_or(Value::Null),
                    service_name: service_name.clone(),
                    attributes: lr
                        .get("attributes")
                        .map(otlp_attrs_to_map)
                        .unwrap_or_default(),
                    resource_attributes: resource_attributes.clone(),
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
                            .map(parse_nano_timestamp)
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
                    .map(parse_nano_timestamp)
                    .unwrap_or(0);
                let end: u64 = s
                    .get("endTimeUnixNano")
                    .map(parse_nano_timestamp)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_nano_timestamp_string_form() {
        assert_eq!(
            parse_nano_timestamp(&json!("1745606400000000000")),
            1_745_606_400_000_000_000u64
        );
    }

    #[test]
    fn parse_nano_timestamp_number_form() {
        // opentelemetry-otlp ≤0.26 emits a bare JSON number for timeUnixNano
        assert_eq!(
            parse_nano_timestamp(&json!(1_745_606_400_000_000_000u64)),
            1_745_606_400_000_000_000u64
        );
    }

    #[test]
    fn parse_nano_timestamp_null_returns_zero() {
        assert_eq!(parse_nano_timestamp(&json!(null)), 0);
    }

    #[test]
    fn extract_otlp_any_value_string() {
        assert_eq!(
            extract_otlp_any_value(&json!({"stringValue": "hello"})),
            json!("hello")
        );
    }

    #[test]
    fn extract_otlp_any_value_bool() {
        assert_eq!(
            extract_otlp_any_value(&json!({"boolValue": true})),
            json!(true)
        );
    }

    #[test]
    fn parse_otlp_logs_numeric_timestamp() {
        use uuid::Uuid;
        let tenant = Uuid::nil();
        let body = json!({
            "resourceLogs": [{
                "resource": {"attributes": [{"key": "service.name", "value": {"stringValue": "svc"}}]},
                "scopeLogs": [{
                    "logRecords": [{
                        "timeUnixNano": 1_745_606_400_000_000_000u64,
                        "severityText": "INFO",
                        "body": {"stringValue": "test"}
                    }]
                }]
            }]
        });
        let logs = parse_otlp_logs(&body, tenant).unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(
            logs[0].timestamp_unix_nano, 1_745_606_400_000_000_000u64,
            "timestamp must not be zero"
        );
        assert_eq!(
            logs[0].body,
            json!("test"),
            "body should be extracted string, not OTLP wrapper"
        );
        assert_eq!(logs[0].service_name, "svc");
    }

    #[test]
    fn parse_otlp_logs_string_timestamp() {
        use uuid::Uuid;
        let tenant = Uuid::nil();
        let body = json!({
            "resourceLogs": [{
                "resource": {"attributes": []},
                "scopeLogs": [{
                    "logRecords": [{
                        "timeUnixNano": "1745606400000000000",
                        "body": {"stringValue": "msg"}
                    }]
                }]
            }]
        });
        let logs = parse_otlp_logs(&body, tenant).unwrap();
        assert_eq!(logs[0].timestamp_unix_nano, 1_745_606_400_000_000_000u64);
    }
}
