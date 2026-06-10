use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[cfg(feature = "storage")]
use crate::generated::tracing::{TracingSpanEventRowV1, TracingSpanRowV1};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Span {
    pub tenant_id: Uuid,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub service_name: String,
    pub service_namespace: String,
    pub service_version: String,
    pub operation_name: String,
    pub span_kind: SpanKind,
    pub start_time_unix_nano: u64,
    pub end_time_unix_nano: u64,
    pub duration_ns: u64,
    pub status_code: StatusCode,
    pub status_message: String,
    pub attributes: HashMap<String, serde_json::Value>,
    pub resource_attributes: HashMap<String, serde_json::Value>,
    pub environment: String,
    pub host_id: String,
    pub workload: String,
    pub deployment_id: String,
    #[serde(default)]
    pub events: Vec<SpanEvent>,
}

#[cfg(feature = "storage")]
pub type SpanRow = TracingSpanRowV1;

#[cfg(feature = "storage")]
impl From<Span> for SpanRow {
    fn from(s: Span) -> Self {
        let span_kind = match s.span_kind {
            SpanKind::Internal => "INTERNAL",
            SpanKind::Server => "SERVER",
            SpanKind::Client => "CLIENT",
            SpanKind::Producer => "PRODUCER",
            SpanKind::Consumer => "CONSUMER",
        }
        .into();
        let status_code = match s.status_code {
            StatusCode::Unset => "UNSET",
            StatusCode::Ok => "OK",
            StatusCode::Error => "ERROR",
        }
        .into();
        Self {
            tenant_id: s.tenant_id,
            trace_id: s.trace_id,
            span_id: s.span_id,
            parent_span_id: s.parent_span_id,
            service_name: s.service_name,
            service_namespace: s.service_namespace,
            service_version: s.service_version,
            operation_name: s.operation_name,
            span_kind,
            start_time_unix_nano: s.start_time_unix_nano,
            end_time_unix_nano: s.end_time_unix_nano,
            duration_ns: s.duration_ns,
            status_code,
            status_message: s.status_message,
            attributes: serde_json::to_string(&s.attributes).unwrap_or_default(),
            resource_attributes: serde_json::to_string(&s.resource_attributes).unwrap_or_default(),
            environment: s.environment,
            host_id: s.host_id,
            workload: s.workload,
            deployment_id: s.deployment_id,
        }
    }
}

#[cfg(feature = "storage")]
impl From<SpanRow> for Span {
    fn from(row: SpanRow) -> Self {
        let span_kind = match row.span_kind.as_str() {
            "SERVER" => SpanKind::Server,
            "CLIENT" => SpanKind::Client,
            "PRODUCER" => SpanKind::Producer,
            "CONSUMER" => SpanKind::Consumer,
            _ => SpanKind::Internal,
        };
        let status_code = match row.status_code.as_str() {
            "OK" => StatusCode::Ok,
            "ERROR" => StatusCode::Error,
            _ => StatusCode::Unset,
        };
        Self {
            tenant_id: row.tenant_id,
            trace_id: row.trace_id,
            span_id: row.span_id,
            parent_span_id: row.parent_span_id,
            service_name: row.service_name,
            service_namespace: row.service_namespace,
            service_version: row.service_version,
            operation_name: row.operation_name,
            span_kind,
            start_time_unix_nano: row.start_time_unix_nano,
            end_time_unix_nano: row.end_time_unix_nano,
            duration_ns: row.duration_ns,
            status_code,
            status_message: row.status_message,
            attributes: serde_json::from_str(&row.attributes).unwrap_or_default(),
            resource_attributes: serde_json::from_str(&row.resource_attributes).unwrap_or_default(),
            environment: row.environment,
            host_id: row.host_id,
            workload: row.workload,
            deployment_id: row.deployment_id,
            events: vec![],
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpanKind {
    #[default]
    Internal,
    Server,
    Client,
    Producer,
    Consumer,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StatusCode {
    #[default]
    Unset,
    Ok,
    Error,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpanEvent {
    pub tenant_id: Uuid,
    pub trace_id: String,
    pub span_id: String,
    pub event_index: u32,
    pub name: String,
    pub timestamp_unix_nano: u64,
    pub attributes: HashMap<String, serde_json::Value>,
}

#[cfg(feature = "storage")]
pub type SpanEventRow = TracingSpanEventRowV1;

#[cfg(feature = "storage")]
impl From<SpanEvent> for SpanEventRow {
    fn from(e: SpanEvent) -> Self {
        Self {
            tenant_id: e.tenant_id,
            trace_id: e.trace_id,
            span_id: e.span_id,
            event_index: e.event_index,
            name: e.name,
            timestamp_unix_nano: e.timestamp_unix_nano,
            attributes: serde_json::to_string(&e.attributes).unwrap_or_else(|_| "{}".to_string()),
        }
    }
}

#[cfg(feature = "storage")]
impl From<SpanEventRow> for SpanEvent {
    fn from(r: SpanEventRow) -> Self {
        let attributes = serde_json::from_str(&r.attributes).unwrap_or_default();
        Self {
            tenant_id: r.tenant_id,
            trace_id: r.trace_id,
            span_id: r.span_id,
            event_index: r.event_index,
            name: r.name,
            timestamp_unix_nano: r.timestamp_unix_nano,
            attributes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "storage")]
    #[test]
    fn span_event_row_roundtrip_preserves_all_fields() {
        let ev = SpanEvent {
            tenant_id: Uuid::new_v4(),
            trace_id: "abc".into(),
            span_id: "def".into(),
            event_index: 2,
            name: "exception".into(),
            timestamp_unix_nano: 1_700_000_000_000_000_001,
            attributes: [(
                "exception.type".to_string(),
                serde_json::json!("NullPointerException"),
            )]
            .into_iter()
            .collect(),
        };
        let row = SpanEventRow::from(ev.clone());
        let recovered = SpanEvent::from(row);
        assert_eq!(recovered.tenant_id, ev.tenant_id);
        assert_eq!(recovered.trace_id, ev.trace_id);
        assert_eq!(recovered.span_id, ev.span_id);
        assert_eq!(recovered.event_index, ev.event_index);
        assert_eq!(recovered.name, ev.name);
        assert_eq!(recovered.timestamp_unix_nano, ev.timestamp_unix_nano);
        assert_eq!(recovered.attributes, ev.attributes);
    }

    #[test]
    fn span_events_default_to_empty_in_span() {
        let span = Span::default();
        assert!(span.events.is_empty());
    }

    #[test]
    fn span_roundtrips_json() {
        let span = Span {
            tenant_id: Uuid::new_v4(),
            trace_id: "4bf92f3577b34da6a3ce929d0e0e4736".into(),
            span_id: "00f067aa0ba902b7".into(),
            parent_span_id: None,
            service_name: "checkout".into(),
            operation_name: "POST /order".into(),
            span_kind: SpanKind::Server,
            start_time_unix_nano: 1_700_000_000_000_000_000,
            end_time_unix_nano: 1_700_000_000_005_000_000,
            duration_ns: 5_000_000,
            status_code: StatusCode::Ok,
            ..Default::default()
        };
        let json = serde_json::to_string(&span).unwrap();
        let back: Span = serde_json::from_str(&json).unwrap();
        assert_eq!(back.trace_id, span.trace_id);
        assert_eq!(back.duration_ns, 5_000_000);
    }
}
