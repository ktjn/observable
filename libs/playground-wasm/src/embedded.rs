use domain::{Span, StatusCode, TelemetryEnvelope, processing::MergedTelemetry};
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
}
