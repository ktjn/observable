use clickhouse::{Client, Row};
use domain::{Span, SpanKind, StatusCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Row, Serialize, Deserialize)]
pub struct SpanRow {
    pub tenant_id: Uuid,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub service_name: String,
    pub service_namespace: String,
    pub service_version: String,
    pub operation_name: String,
    pub span_kind: String,
    pub start_time_unix_nano: u64,
    pub end_time_unix_nano: u64,
    pub duration_ns: u64,
    pub status_code: String,
    pub status_message: String,
    pub attributes: String,
    pub resource_attributes: String,
    pub environment: String,
    pub host_id: String,
    pub workload: String,
    pub deployment_id: String,
}

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
            resource_attributes: serde_json::to_string(&s.resource_attributes)
                .unwrap_or_default(),
            environment: s.environment,
            host_id: s.host_id,
            workload: s.workload,
            deployment_id: s.deployment_id,
        }
    }
}

pub async fn insert_spans(ch: &Client, spans: Vec<Span>) -> anyhow::Result<()> {
    let mut insert = ch.insert("spans")?;
    for span in spans {
        insert.write(&SpanRow::from(span)).await?;
    }
    insert.end().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{Span, StatusCode};
    use uuid::Uuid;

    #[test]
    fn ch_row_from_span_maps_status() {
        let span = Span {
            tenant_id: Uuid::new_v4(),
            trace_id: "abc".into(),
            span_id: "def".into(),
            status_code: StatusCode::Error,
            start_time_unix_nano: 1_000,
            end_time_unix_nano: 2_000,
            duration_ns: 1_000,
            ..Default::default()
        };
        let row = SpanRow::from(span);
        assert_eq!(row.status_code, "ERROR");
    }
}
