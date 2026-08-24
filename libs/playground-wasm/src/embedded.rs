use domain::{TelemetryEnvelope, processing::MergedTelemetry};

/// Portable stream-processor core used by the browser composition layer.
/// Native stream-processor hosts use the same domain processing functions;
/// this wrapper keeps the WASM binding focused on composition and transport.
#[derive(Debug, Default)]
pub struct EmbeddedStreamProcessor;

impl EmbeddedStreamProcessor {
    pub fn process_batch(
        &self,
        batch: impl IntoIterator<Item = TelemetryEnvelope>,
    ) -> MergedTelemetry {
        domain::processing::merge_telemetry(batch)
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
}
