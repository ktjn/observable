use crate::{EnvelopePayload, LogRecord, MetricPoint, MetricSeries, Span, TelemetryEnvelope};
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct MergedTelemetry {
    pub spans: Vec<Span>,
    pub logs: Vec<LogRecord>,
    pub series: Vec<MetricSeries>,
    pub points: Vec<MetricPoint>,
}

/// Applies the stream-processor's tenant and derived-field normalization to
/// one span before storage or downstream evaluation.
pub fn normalise_span(mut span: Span, tenant_id: Uuid) -> Span {
    span.tenant_id = tenant_id;
    if span.duration_ns == 0 {
        span.duration_ns = span
            .end_time_unix_nano
            .saturating_sub(span.start_time_unix_nano);
    }
    span
}

pub fn normalise_log(mut log: LogRecord, tenant_id: Uuid) -> LogRecord {
    log.tenant_id = tenant_id;
    if log.log_id == Uuid::nil() {
        log.log_id = Uuid::new_v4();
    }
    log
}

pub fn normalise_metric_series(mut series: MetricSeries, tenant_id: Uuid) -> MetricSeries {
    series.tenant_id = tenant_id;
    series
}

pub fn normalise_metric_point(mut point: MetricPoint, tenant_id: Uuid) -> MetricPoint {
    point.tenant_id = tenant_id;
    point
}

/// Merges one bounded stream-processor batch and normalizes every payload
/// using the envelope tenant before downstream consumers see it.
pub fn merge_telemetry(batch: impl IntoIterator<Item = TelemetryEnvelope>) -> MergedTelemetry {
    let mut merged = MergedTelemetry::default();
    for envelope in batch {
        let tenant_id = envelope.tenant_id;
        match envelope.payload {
            EnvelopePayload::Spans(spans) => merged.spans.extend(
                spans
                    .into_iter()
                    .map(|span| normalise_span(span, tenant_id)),
            ),
            EnvelopePayload::Logs(logs) => merged
                .logs
                .extend(logs.into_iter().map(|log| normalise_log(log, tenant_id))),
            EnvelopePayload::Metrics { series, points } => {
                merged.series.extend(
                    series
                        .into_iter()
                        .map(|item| normalise_metric_series(item, tenant_id)),
                );
                merged.points.extend(
                    points
                        .into_iter()
                        .map(|item| normalise_metric_point(item, tenant_id)),
                );
            }
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_stamps_tenant_and_derives_span_duration() {
        let tenant_id = Uuid::new_v4();
        let merged = merge_telemetry([TelemetryEnvelope {
            envelope_id: Uuid::new_v4(),
            tenant_id,
            environment: "production".into(),
            received_at_unix_nano: 1,
            payload: EnvelopePayload::Spans(vec![Span {
                start_time_unix_nano: 10,
                end_time_unix_nano: 25,
                ..Default::default()
            }]),
        }]);

        assert_eq!(merged.spans[0].tenant_id, tenant_id);
        assert_eq!(merged.spans[0].duration_ns, 15);
    }
}
