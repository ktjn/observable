use domain::{LogRecord, Span, StatusCode, TelemetryEnvelope, processing::MergedTelemetry};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Portable stream-processor core used by the browser composition layer.
/// Native stream-processor hosts use the same domain processing functions;
/// this wrapper keeps the WASM binding focused on composition and transport.
#[derive(Debug, Default)]
pub struct EmbeddedStreamProcessor;

#[derive(Debug, Deserialize)]
struct GeneratedSpan {
    trace_id: String,
    span_id: String,
    parent_span_id: String,
    service_name: String,
    operation_name: String,
    duration_ns: String,
    status_code: String,
    environment: String,
    start_time_unix_nano: String,
}

#[derive(Debug, Deserialize)]
struct GeneratedLog {
    timestamp_unix_nano: String,
    observed_timestamp_unix_nano: String,
    severity_number: i32,
    severity_text: String,
    body: String,
    trace_id: String,
    span_id: String,
    service_name: String,
    environment: String,
    host_id: String,
}

#[derive(Debug, Serialize)]
pub struct ProcessedLog {
    pub tenant_id: Uuid,
    pub log_id: Uuid,
    pub timestamp_unix_nano: u64,
    pub observed_timestamp_unix_nano: u64,
    pub severity_number: i32,
    pub severity_text: String,
    pub body: String,
    pub trace_id: String,
    pub span_id: String,
    pub service_name: String,
    pub environment: String,
    pub host_id: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ProcessedSpan {
    pub tenant_id: Uuid,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub service_name: String,
    pub operation_name: String,
    pub duration_ns: u64,
    pub status_code: String,
    pub environment: String,
    pub start_time_unix_nano: u64,
}

impl EmbeddedStreamProcessor {
    pub fn process_batch(
        &self,
        batch: impl IntoIterator<Item = TelemetryEnvelope>,
    ) -> MergedTelemetry {
        domain::processing::merge_telemetry(batch)
    }

    /// Converts the playground producer's compact span payload into the
    /// canonical domain contract before it reaches the browser-local store.
    pub fn process_generated_spans(
        json: &str,
        tenant_id: Uuid,
    ) -> Result<Vec<ProcessedSpan>, String> {
        let generated: Vec<GeneratedSpan> =
            serde_json::from_str(json).map_err(|e| e.to_string())?;
        generated
            .into_iter()
            .map(|span| {
                let start_time_unix_nano = span
                    .start_time_unix_nano
                    .parse::<u64>()
                    .map_err(|e| e.to_string())?;
                let duration_ns = span.duration_ns.parse::<u64>().map_err(|e| e.to_string())?;
                let status_code = match span.status_code.as_str() {
                    "ERROR" => StatusCode::Error,
                    "OK" => StatusCode::Ok,
                    _ => StatusCode::Unset,
                };
                let span = Span {
                    tenant_id,
                    trace_id: span.trace_id,
                    span_id: span.span_id,
                    parent_span_id: (!span.parent_span_id.is_empty())
                        .then_some(span.parent_span_id),
                    service_name: span.service_name,
                    operation_name: span.operation_name,
                    start_time_unix_nano,
                    end_time_unix_nano: start_time_unix_nano.saturating_add(duration_ns),
                    duration_ns,
                    status_code,
                    environment: span.environment,
                    ..Default::default()
                };
                let span = domain::processing::normalise_span(span, tenant_id);
                Ok(ProcessedSpan {
                    tenant_id: span.tenant_id,
                    trace_id: span.trace_id,
                    span_id: span.span_id,
                    parent_span_id: span.parent_span_id,
                    service_name: span.service_name,
                    operation_name: span.operation_name,
                    duration_ns: span.duration_ns,
                    status_code: match span.status_code {
                        StatusCode::Error => "ERROR",
                        StatusCode::Ok => "OK",
                        StatusCode::Unset => "UNSET",
                    }
                    .to_string(),
                    environment: span.environment,
                    start_time_unix_nano: span.start_time_unix_nano,
                })
            })
            .collect()
    }

    /// Converts generated logs into canonical records before local storage.
    pub fn process_generated_logs(
        json: &str,
        tenant_id: Uuid,
    ) -> Result<Vec<ProcessedLog>, String> {
        let generated: Vec<GeneratedLog> = serde_json::from_str(json).map_err(|e| e.to_string())?;
        generated
            .into_iter()
            .map(|log| {
                let timestamp_unix_nano = log
                    .timestamp_unix_nano
                    .parse::<u64>()
                    .map_err(|e| e.to_string())?;
                let observed_timestamp_unix_nano = log
                    .observed_timestamp_unix_nano
                    .parse::<u64>()
                    .map_err(|e| e.to_string())?;
                let record = domain::processing::normalise_log(
                    LogRecord {
                        tenant_id,
                        log_id: Uuid::nil(),
                        timestamp_unix_nano,
                        observed_timestamp_unix_nano,
                        severity_number: log.severity_number,
                        severity_text: log.severity_text,
                        body: serde_json::Value::String(log.body),
                        trace_id: Some(log.trace_id),
                        span_id: Some(log.span_id),
                        service_name: log.service_name,
                        environment: log.environment,
                        host_id: log.host_id,
                        ..Default::default()
                    },
                    tenant_id,
                );
                Ok(ProcessedLog {
                    tenant_id: record.tenant_id,
                    log_id: record.log_id,
                    timestamp_unix_nano: record.timestamp_unix_nano,
                    observed_timestamp_unix_nano: record.observed_timestamp_unix_nano,
                    severity_number: record.severity_number,
                    severity_text: record.severity_text,
                    body: record.body.to_string(),
                    trace_id: record.trace_id.unwrap_or_default(),
                    span_id: record.span_id.unwrap_or_default(),
                    service_name: record.service_name,
                    environment: record.environment,
                    host_id: record.host_id,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{EnvelopePayload, Span};
    use uuid::Uuid;

    #[test]
    fn browser_processor_uses_shared_domain_normalization() {
        let tenant_id = Uuid::new_v4();
        let result = EmbeddedStreamProcessor.process_batch([TelemetryEnvelope {
            envelope_id: Uuid::new_v4(),
            tenant_id,
            environment: "production".into(),
            received_at_unix_nano: 1,
            payload: EnvelopePayload::Spans(vec![Span {
                start_time_unix_nano: 100,
                end_time_unix_nano: 125,
                ..Default::default()
            }]),
        }]);

        assert_eq!(result.spans[0].tenant_id, tenant_id);
        assert_eq!(result.spans[0].duration_ns, 25);
    }

    #[test]
    fn generated_spans_cross_ingest_boundary_before_storage() {
        let tenant_id = Uuid::new_v4();
        let json = r#"[{"trace_id":"trace-1","span_id":"span-1","parent_span_id":"","service_name":"api","operation_name":"GET /","duration_ns":"10","status_code":"OK","environment":"production","start_time_unix_nano":"100"}]"#;

        let processed = EmbeddedStreamProcessor::process_generated_spans(json, tenant_id).unwrap();

        assert_eq!(processed[0].tenant_id, tenant_id);
        assert_eq!(processed[0].parent_span_id, None);
        assert_eq!(processed[0].duration_ns, 10);
    }

    #[test]
    fn generated_logs_cross_ingest_boundary_with_tenant_stamp() {
        let tenant_id = Uuid::new_v4();
        let json = r#"[{"log_id":"ignored","timestamp_unix_nano":"100","observed_timestamp_unix_nano":"101","severity_number":9,"severity_text":"INFO","body":"ready","trace_id":"trace-1","span_id":"span-1","service_name":"api","environment":"production","host_id":"host"}]"#;

        let processed = EmbeddedStreamProcessor::process_generated_logs(json, tenant_id).unwrap();

        assert_eq!(processed[0].tenant_id, tenant_id);
        assert_eq!(processed[0].timestamp_unix_nano, 100);
        assert_eq!(processed[0].body, "\"ready\"");
    }
}
