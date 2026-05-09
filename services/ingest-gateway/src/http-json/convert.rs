use axum::http::StatusCode;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
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

fn parse_i64_value(v: &Value) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
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
    environment: &str,
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
                    environment: environment.to_string(),
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
    environment: &str,
) -> Result<(Vec<domain::MetricSeries>, Vec<domain::MetricPoint>), StatusCode> {
    let resource_metrics = body
        .get("resourceMetrics")
        .and_then(|v| v.as_array())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let mut all_series = Vec::new();
    let mut all_points = Vec::new();
    let mut seen_series = HashSet::new();

    for rm in resource_metrics {
        let resource_attrs = rm
            .get("resource")
            .and_then(|r| r.get("attributes"))
            .cloned()
            .unwrap_or_default();
        let service_name = extract_string_attr(&resource_attrs, "service.name").unwrap_or_default();
        let resource_attributes = otlp_attrs_to_map(&resource_attrs);

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

                let (metric_data, metric_type, is_monotonic, aggregation_temporality) =
                    if let Some(sum) = metric.get("sum") {
                        (
                            sum,
                            domain::MetricType::Sum,
                            sum.get("isMonotonic").and_then(|v| v.as_bool()),
                            sum.get("aggregationTemporality")
                                .and_then(parse_aggregation_temporality),
                        )
                    } else if let Some(gauge) = metric.get("gauge") {
                        (gauge, domain::MetricType::Gauge, None, None)
                    } else if let Some(histogram) = metric.get("histogram") {
                        (
                            histogram,
                            domain::MetricType::Histogram,
                            None,
                            histogram
                                .get("aggregationTemporality")
                                .and_then(parse_aggregation_temporality),
                        )
                    } else {
                        continue;
                    };

                let Some(data_points) = metric_data.get("dataPoints").and_then(|v| v.as_array())
                else {
                    continue;
                };

                for dp in data_points {
                    let attributes = dp
                        .get("attributes")
                        .map(otlp_attrs_to_string_map)
                        .unwrap_or_default();
                    let mut series = domain::MetricSeries {
                        tenant_id,
                        metric_name: metric_name.clone(),
                        description: metric
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        unit: metric
                            .get("unit")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        metric_type: metric_type.clone(),
                        is_monotonic,
                        aggregation_temporality: aggregation_temporality.clone(),
                        attributes,
                        resource_attributes: resource_attributes.clone(),
                        service_name: service_name.clone(),
                        environment: environment.to_string(),
                        ..Default::default()
                    };
                    series.metric_series_id = domain::deterministic_metric_series_id(&series);
                    if seen_series.insert(series.metric_series_id) {
                        all_series.push(series.clone());
                    }

                    let time = dp
                        .get("timeUnixNano")
                        .map(parse_nano_timestamp)
                        .unwrap_or(0);
                    let start_time_unix_nano =
                        dp.get("startTimeUnixNano").map(parse_nano_timestamp);
                    let mut point = domain::MetricPoint {
                        tenant_id,
                        metric_series_id: series.metric_series_id,
                        metric_name: metric_name.clone(),
                        service_name: service_name.clone(),
                        time_unix_nano: time,
                        start_time_unix_nano,
                        value_double: dp.get("asDouble").and_then(|v| v.as_f64()),
                        value_int: dp.get("asInt").and_then(parse_i64_value),
                        ..Default::default()
                    };
                    if metric_type == domain::MetricType::Histogram {
                        point.histogram_count = dp.get("count").and_then(|v| {
                            v.as_u64()
                                .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
                        });
                        point.histogram_sum = dp.get("sum").and_then(|v| v.as_f64());
                        point.histogram_bucket_counts = dp
                            .get("bucketCounts")
                            .and_then(|v| v.as_array())
                            .map(|values| {
                                values
                                    .iter()
                                    .filter_map(|v| {
                                        v.as_u64().or_else(|| {
                                            v.as_str().and_then(|s| s.parse::<u64>().ok())
                                        })
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .filter(|values| !values.is_empty());
                        point.histogram_explicit_bounds = dp
                            .get("explicitBounds")
                            .and_then(|v| v.as_array())
                            .map(|values| {
                                values.iter().filter_map(|v| v.as_f64()).collect::<Vec<_>>()
                            })
                            .filter(|values| !values.is_empty());
                    }
                    all_points.push(point);
                }
            }
        }
    }
    Ok((all_series, all_points))
}

fn parse_aggregation_temporality(v: &Value) -> Option<domain::AggregationTemporality> {
    match v {
        Value::Number(n) if n.as_i64() == Some(1) => Some(domain::AggregationTemporality::Delta),
        Value::Number(n) if n.as_i64() == Some(2) => {
            Some(domain::AggregationTemporality::Cumulative)
        }
        Value::String(s)
            if s == "AGGREGATION_TEMPORALITY_DELTA" || s.eq_ignore_ascii_case("delta") =>
        {
            Some(domain::AggregationTemporality::Delta)
        }
        Value::String(s)
            if s == "AGGREGATION_TEMPORALITY_CUMULATIVE"
                || s.eq_ignore_ascii_case("cumulative") =>
        {
            Some(domain::AggregationTemporality::Cumulative)
        }
        _ => None,
    }
}

fn otlp_attrs_to_string_map(attrs: &Value) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(arr) = attrs.as_array() {
        for kv in arr {
            if let (Some(k), Some(val)) = (kv.get("key").and_then(|x| x.as_str()), kv.get("value"))
            {
                let value = extract_otlp_any_value(val);
                let value = match value {
                    Value::String(s) => s,
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::Null => String::new(),
                    other => other.to_string(),
                };
                map.insert(k.to_owned(), value);
            }
        }
    }
    map
}

fn span_kind_from_json(v: &Value) -> domain::SpanKind {
    // OTLP JSON encodes SpanKind as an integer (proto3 JSON) or a string enum name.
    let n = v
        .as_i64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()));
    match n {
        Some(2) => domain::SpanKind::Server,
        Some(3) => domain::SpanKind::Client,
        Some(4) => domain::SpanKind::Producer,
        Some(5) => domain::SpanKind::Consumer,
        _ => domain::SpanKind::Internal,
    }
}

fn status_from_json(status: Option<&Value>) -> (domain::StatusCode, String) {
    let Some(st) = status else {
        return Default::default();
    };
    let code = st
        .get("code")
        .and_then(|c| {
            c.as_i64()
                .or_else(|| c.as_str().and_then(|s| s.parse::<i64>().ok()))
        })
        .unwrap_or(0);
    let status_code = match code {
        1 => domain::StatusCode::Ok,
        2 => domain::StatusCode::Error,
        _ => domain::StatusCode::Unset,
    };
    let status_message = st
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or_default()
        .to_string();
    (status_code, status_message)
}

pub fn parse_otlp_traces(
    body: &Value,
    tenant_id: Uuid,
    environment: &str,
) -> Result<Vec<domain::Span>, StatusCode> {
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
        let service_version =
            extract_string_attr(&resource_attrs, "service.version").unwrap_or_default();
        let resource_attributes = otlp_attrs_to_map(&resource_attrs);
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
                let trace_id: String = s
                    .get("traceId")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .into();
                let span_id: String = s
                    .get("spanId")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .into();
                let parent_span_id: Option<String> = s
                    .get("parentSpanId")
                    .and_then(|v| v.as_str())
                    .filter(|v| !v.is_empty())
                    .map(String::from);
                let span_kind = s
                    .get("kind")
                    .map(span_kind_from_json)
                    .unwrap_or(domain::SpanKind::Internal);
                let (status_code, status_message) = status_from_json(s.get("status"));
                spans.push(domain::Span {
                    tenant_id,
                    trace_id: trace_id.clone(),
                    span_id: span_id.clone(),
                    parent_span_id,
                    service_name: service_name.clone(),
                    service_version: service_version.clone(),
                    operation_name: s
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .into(),
                    span_kind,
                    start_time_unix_nano: start,
                    end_time_unix_nano: end,
                    duration_ns: end.saturating_sub(start),
                    status_code,
                    status_message,
                    attributes: s
                        .get("attributes")
                        .map(otlp_attrs_to_map)
                        .unwrap_or_default(),
                    resource_attributes: resource_attributes.clone(),
                    environment: environment.to_string(),
                    ..Default::default()
                });
                let span_events: Vec<domain::SpanEvent> = s
                    .get("events")
                    .and_then(|v| v.as_array())
                    .map(|events| {
                        events
                            .iter()
                            .enumerate()
                            .map(|(i, e)| domain::SpanEvent {
                                tenant_id,
                                trace_id: trace_id.clone(),
                                span_id: span_id.clone(),
                                event_index: i as u32,
                                name: e
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default()
                                    .into(),
                                timestamp_unix_nano: e
                                    .get("timeUnixNano")
                                    .map(parse_nano_timestamp)
                                    .unwrap_or(0),
                                attributes: e
                                    .get("attributes")
                                    .map(otlp_attrs_to_map)
                                    .unwrap_or_default(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if let Some(span) = spans.last_mut() {
                    span.events = span_events;
                }
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
        let logs = parse_otlp_logs(&body, tenant, "testbench").unwrap();
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
        assert_eq!(
            logs[0].environment, "testbench",
            "environment must be set from token"
        );
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
        let logs = parse_otlp_logs(&body, tenant, "testbench").unwrap();
        assert_eq!(logs[0].timestamp_unix_nano, 1_745_606_400_000_000_000u64);
    }

    #[test]
    fn parse_otlp_metrics_preserves_sum_metadata_and_labels() {
        let tenant = Uuid::nil();
        let body = json!({
            "resourceMetrics": [{
                "resource": {
                    "attributes": [
                        {"key": "service.name", "value": {"stringValue": "checkout"}},
                        {"key": "deployment.environment", "value": {"stringValue": "prod"}},
                        {"key": "host.name", "value": {"stringValue": "node-a"}}
                    ]
                },
                "scopeMetrics": [{
                    "metrics": [{
                        "name": "http.server.requests",
                        "description": "HTTP server requests",
                        "unit": "1",
                        "sum": {
                            "aggregationTemporality": 2,
                            "isMonotonic": true,
                            "dataPoints": [{
                                "attributes": [
                                    {"key": "http.route", "value": {"stringValue": "/checkout"}},
                                    {"key": "http.status_code", "value": {"intValue": "200"}}
                                ],
                                "startTimeUnixNano": "100",
                                "timeUnixNano": "200",
                                "asInt": "7"
                            }]
                        }
                    }]
                }]
            }]
        });

        let (series, points) = parse_otlp_metrics(&body, tenant, "prod").unwrap();

        assert_eq!(series.len(), 1);
        assert_eq!(points.len(), 1);
        assert_eq!(series[0].metric_name, "http.server.requests");
        assert_eq!(series[0].description, "HTTP server requests");
        assert_eq!(series[0].unit, "1");
        assert_eq!(series[0].metric_type, domain::MetricType::Sum);
        assert_eq!(series[0].is_monotonic, Some(true));
        assert_eq!(
            series[0].aggregation_temporality,
            Some(domain::AggregationTemporality::Cumulative)
        );
        assert_eq!(series[0].service_name, "checkout");
        assert_eq!(series[0].environment, "prod");
        assert_eq!(
            series[0].attributes.get("http.route").map(String::as_str),
            Some("/checkout")
        );
        assert_eq!(
            series[0]
                .attributes
                .get("http.status_code")
                .map(String::as_str),
            Some("200")
        );
        assert_eq!(
            series[0].resource_attributes.get("host.name"),
            Some(&json!("node-a"))
        );
        assert_eq!(points[0].metric_series_id, series[0].metric_series_id);
        assert_eq!(points[0].start_time_unix_nano, Some(100));
        assert_eq!(points[0].time_unix_nano, 200);
        assert_eq!(points[0].value_int, Some(7));
    }

    #[test]
    fn parse_otlp_metrics_uses_stable_series_id_for_same_series() {
        let tenant = Uuid::nil();
        let body = json!({
            "resourceMetrics": [{
                "resource": {"attributes": [{"key": "service.name", "value": {"stringValue": "checkout"}}]},
                "scopeMetrics": [{
                    "metrics": [{
                        "name": "cpu.usage",
                        "gauge": {
                            "dataPoints": [{
                                "attributes": [{"key": "core", "value": {"stringValue": "0"}}],
                                "timeUnixNano": "200",
                                "asDouble": 0.7
                            }]
                        }
                    }]
                }]
            }]
        });

        let (first_series, first_points) = parse_otlp_metrics(&body, tenant, "testbench").unwrap();
        let (second_series, second_points) =
            parse_otlp_metrics(&body, tenant, "testbench").unwrap();

        assert_eq!(
            first_series[0].metric_series_id,
            second_series[0].metric_series_id
        );
        assert_eq!(
            first_points[0].metric_series_id,
            second_points[0].metric_series_id
        );
    }

    #[test]
    fn parse_otlp_traces_extracts_parent_span_id_and_environment() {
        let tenant = Uuid::nil();
        let body = json!({
            "resourceSpans": [{
                "resource": {
                    "attributes": [
                        {"key": "service.name", "value": {"stringValue": "shop-frontend"}},
                        {"key": "deployment.environment", "value": {"stringValue": "testbench"}},
                        {"key": "host.name", "value": {"stringValue": "node-1"}}
                    ]
                },
                "scopeSpans": [{
                    "spans": [{
                        "traceId": "aabbccddaabbccddaabbccddaabbccdd",
                        "spanId": "1122334411223344",
                        "parentSpanId": "aabbccdd11223344",
                        "name": "GET /products",
                        "kind": 3,
                        "startTimeUnixNano": "1000000000",
                        "endTimeUnixNano": "2000000000",
                        "status": {"code": 2, "message": "internal error"},
                        "attributes": [
                            {"key": "http.method", "value": {"stringValue": "GET"}},
                            {"key": "http.status_code", "value": {"intValue": "500"}}
                        ]
                    }]
                }]
            }]
        });

        let spans = parse_otlp_traces(&body, tenant, "testbench").unwrap();
        assert_eq!(spans.len(), 1);

        let span = &spans[0];
        assert_eq!(span.parent_span_id.as_deref(), Some("aabbccdd11223344"));
        assert_eq!(span.environment, "testbench");
        assert_eq!(span.span_kind, domain::SpanKind::Client);
        assert_eq!(span.status_code, domain::StatusCode::Error);
        assert_eq!(span.status_message, "internal error");
        assert_eq!(span.attributes.get("http.method"), Some(&json!("GET")));
        assert_eq!(
            span.resource_attributes.get("host.name"),
            Some(&json!("node-1"))
        );
        assert_eq!(span.duration_ns, 1_000_000_000);
    }

    #[test]
    fn parse_otlp_traces_empty_parent_span_id_becomes_none() {
        let tenant = Uuid::nil();
        let body = json!({
            "resourceSpans": [{
                "resource": {"attributes": [
                    {"key": "service.name", "value": {"stringValue": "svc"}}
                ]},
                "scopeSpans": [{
                    "spans": [{
                        "traceId": "aabb",
                        "spanId": "ccdd",
                        "parentSpanId": "",
                        "name": "root",
                        "startTimeUnixNano": "0",
                        "endTimeUnixNano": "0"
                    }]
                }]
            }]
        });

        let spans = parse_otlp_traces(&body, tenant, "testbench").unwrap();
        assert_eq!(spans[0].parent_span_id, None);
    }

    #[test]
    fn parse_otlp_traces_status_code_ok() {
        let tenant = Uuid::nil();
        let body = json!({
            "resourceSpans": [{
                "resource": {"attributes": [
                    {"key": "service.name", "value": {"stringValue": "svc"}}
                ]},
                "scopeSpans": [{
                    "spans": [{
                        "traceId": "aa",
                        "spanId": "bb",
                        "name": "op",
                        "startTimeUnixNano": "0",
                        "endTimeUnixNano": "0",
                        "status": {"code": 1}
                    }]
                }]
            }]
        });

        let spans = parse_otlp_traces(&body, tenant, "testbench").unwrap();
        assert_eq!(spans[0].status_code, domain::StatusCode::Ok);
    }
}
