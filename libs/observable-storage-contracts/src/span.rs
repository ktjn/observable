use domain::{Span, SpanEvent, SpanKind, StatusCode};

use crate::generated::tracing::{TracingSpanEventRowV1, TracingSpanRowV1};

pub type SpanRow = TracingSpanRowV1;

impl From<Span> for SpanRow {
    fn from(s: Span) -> Self {
        Self {
            tenant_id: s.tenant_id,
            trace_id: s.trace_id,
            span_id: s.span_id,
            parent_span_id: s.parent_span_id,
            service_name: s.service_name,
            service_namespace: s.service_namespace,
            service_version: s.service_version,
            operation_name: s.operation_name,
            span_kind: match s.span_kind {
                SpanKind::Internal => "INTERNAL".to_string(),
                SpanKind::Server => "SERVER".to_string(),
                SpanKind::Client => "CLIENT".to_string(),
                SpanKind::Producer => "PRODUCER".to_string(),
                SpanKind::Consumer => "CONSUMER".to_string(),
            },
            start_time_unix_nano: s.start_time_unix_nano,
            end_time_unix_nano: s.end_time_unix_nano,
            duration_ns: s.duration_ns,
            status_code: match s.status_code {
                StatusCode::Unset => "UNSET".to_string(),
                StatusCode::Ok => "OK".to_string(),
                StatusCode::Error => "ERROR".to_string(),
            },
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

impl From<SpanRow> for Span {
    fn from(row: SpanRow) -> Self {
        Self {
            tenant_id: row.tenant_id,
            trace_id: row.trace_id,
            span_id: row.span_id,
            parent_span_id: row.parent_span_id,
            service_name: row.service_name,
            service_namespace: row.service_namespace,
            service_version: row.service_version,
            operation_name: row.operation_name,
            span_kind: match row.span_kind.as_str() {
                "SERVER" => SpanKind::Server,
                "CLIENT" => SpanKind::Client,
                "PRODUCER" => SpanKind::Producer,
                "CONSUMER" => SpanKind::Consumer,
                _ => SpanKind::Internal,
            },
            start_time_unix_nano: row.start_time_unix_nano,
            end_time_unix_nano: row.end_time_unix_nano,
            duration_ns: row.duration_ns,
            status_code: match row.status_code.as_str() {
                "OK" => StatusCode::Ok,
                "ERROR" => StatusCode::Error,
                _ => StatusCode::Unset,
            },
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

pub type SpanEventRow = TracingSpanEventRowV1;

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
    use uuid::Uuid;

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
}
