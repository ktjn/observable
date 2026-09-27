//! Validates that domain's hand-authored wire types actually serialize to JSON
//! conforming to the versioned schema in contracts/schemas/<domain>/. This is
//! the compatibility check the Modelable compile step (scripts/check-generated-
//! drift.sh) doesn't provide: compile only checks the .mdl definitions are
//! internally consistent, not that the hand-authored Rust struct's real serde
//! output still matches the schema derived from those definitions.
//!
//! Field names are converted to camelCase before validation because the
//! json-schema emitter does not honor `@wire(json.fieldCase: "snake_case")`
//! (only the rust/typescript emitters do) -- see AGENTS.md. The schemas
//! declare camelCase property names; our real wire JSON is snake_case.
//!
//! Example instances below populate every `Option` field with `Some(..)`.
//! serde serializes `None` as JSON `null`, but the generated schemas mark
//! optional fields absent-from-`required` rather than nullable (`{"type":
//! "string"}`, not `{"type": ["string", "null"]}`), so a present-but-null
//! field would fail validation even though it's a legitimate wire value.
//! That's a real, separate gap from the field-case one -- not modeled here.
use crate::{LogRecord, MetricPoint, Span, SpanEvent, SpanKind, StatusCode};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

fn schema(domain: &str, file: &str) -> Value {
    let path = format!(
        "{}/../../contracts/schemas/{domain}/{file}",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

fn snake_to_camel(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upcase_next = false;
    for c in name.chars() {
        if c == '_' {
            upcase_next = true;
        } else if upcase_next {
            out.extend(c.to_uppercase());
            upcase_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Renames only the top-level keys of a serialized struct to camelCase. Nested
/// values (e.g. the free-form `attributes`/`resourceAttributes` maps) are left
/// untouched -- those are user-supplied map keys, not struct field names, and
/// the schemas declare them as fully open (`additionalProperties: {}`) anyway.
fn camelize_top_level(value: &Value) -> Value {
    let Value::Object(map) = value else {
        panic!("expected a JSON object at the top level");
    };
    let renamed: serde_json::Map<String, Value> = map
        .iter()
        .map(|(k, v)| (snake_to_camel(k), v.clone()))
        .collect();
    Value::Object(renamed)
}

fn assert_matches_schema(instance: &Value, schema: &Value, label: &str) {
    let validator = jsonschema::validator_for(schema)
        .unwrap_or_else(|e| panic!("{label}: invalid schema: {e}"));
    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|e| e.to_string())
        .collect();
    assert!(
        errors.is_empty(),
        "{label} does not match its contracts/schemas/ contract:\n{}\ninstance: {}",
        errors.join("\n"),
        serde_json::to_string_pretty(instance).unwrap()
    );
}

#[test]
fn span_matches_tracing_span_v1_schema() {
    let span = Span {
        tenant_id: Uuid::new_v4(),
        trace_id: "4bf92f3577b34da6a3ce929d0e0e4736".into(),
        span_id: "00f067aa0ba902b7".into(),
        parent_span_id: Some("00f067aa0ba902b6".into()),
        service_name: "checkout".into(),
        service_namespace: "commerce".into(),
        service_version: "1.0.0".into(),
        operation_name: "POST /order".into(),
        span_kind: SpanKind::Server,
        start_time_unix_nano: 1_700_000_000_000_000_000,
        end_time_unix_nano: 1_700_000_000_005_000_000,
        duration_ns: 5_000_000,
        status_code: StatusCode::Ok,
        status_message: "".into(),
        attributes: HashMap::new(),
        resource_attributes: HashMap::new(),
        environment: "prod".into(),
        host_id: "host-1".into(),
        workload: "checkout-deployment".into(),
        deployment_id: "dep-1".into(),
        events: vec![],
    };
    let value = serde_json::to_value(&span).unwrap();
    let instance = camelize_top_level(&value);
    let schema = schema("tracing", "tracing.Span.v1.json");
    assert_matches_schema(&instance, &schema, "Span");
}

#[test]
fn span_event_matches_tracing_span_event_v1_schema() {
    let event = SpanEvent {
        tenant_id: Uuid::new_v4(),
        trace_id: "4bf92f3577b34da6a3ce929d0e0e4736".into(),
        span_id: "00f067aa0ba902b7".into(),
        event_index: 0,
        name: "exception".into(),
        timestamp_unix_nano: 1_700_000_000_000_000_000,
        attributes: HashMap::new(),
    };
    let value = serde_json::to_value(&event).unwrap();
    let instance = camelize_top_level(&value);
    let schema = schema("tracing", "tracing.SpanEvent.v1.json");
    assert_matches_schema(&instance, &schema, "SpanEvent");
}

#[test]
fn log_record_matches_logs_log_record_v1_schema() {
    let log = LogRecord {
        tenant_id: Uuid::new_v4(),
        log_id: Uuid::new_v4(),
        timestamp_unix_nano: 1_700_000_000_000_000_000,
        observed_timestamp_unix_nano: 1_700_000_000_000_000_000,
        severity_number: 9,
        severity_text: "INFO".into(),
        body: serde_json::json!("order placed"),
        trace_id: Some("4bf92f3577b34da6a3ce929d0e0e4736".into()),
        span_id: Some("00f067aa0ba902b7".into()),
        attributes: HashMap::new(),
        resource_attributes: HashMap::new(),
        service_name: "checkout".into(),
        environment: "prod".into(),
        host_id: "host-1".into(),
        fingerprint: Some(42),
    };
    let value = serde_json::to_value(&log).unwrap();
    let instance = camelize_top_level(&value);
    let schema = schema("logs", "logs.LogRecord.v1.json");
    assert_matches_schema(&instance, &schema, "LogRecord");
}

#[test]
fn metric_point_matches_metrics_metric_point_v1_schema() {
    let point = MetricPoint {
        tenant_id: Uuid::new_v4(),
        metric_series_id: Uuid::new_v4(),
        metric_name: "http.server.requests".into(),
        service_name: "checkout".into(),
        time_unix_nano: 1_700_000_000_000_000_000,
        start_time_unix_nano: Some(1_700_000_000_000_000_000),
        value_double: Some(1.0),
        value_int: Some(1),
        histogram_count: Some(3),
        histogram_sum: Some(4.5),
        histogram_bucket_counts: Some(vec![1, 2, 3]),
        histogram_explicit_bounds: Some(vec![10.0, 20.0]),
    };
    let value = serde_json::to_value(&point).unwrap();
    let instance = camelize_top_level(&value);
    let schema = schema("metrics", "metrics.MetricPoint.v1.json");
    assert_matches_schema(&instance, &schema, "MetricPoint");
}
