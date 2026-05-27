use domain::{EnvelopePayload, LogRecord, MetricPoint, MetricSeries, Span, TelemetryEnvelope};
use stream_processor::batch::merge_batch;
use uuid::Uuid;

fn env(tenant_id: Uuid, payload: EnvelopePayload) -> TelemetryEnvelope {
    TelemetryEnvelope {
        envelope_id: Uuid::new_v4(),
        tenant_id,
        environment: "prod".into(),
        received_at_unix_nano: 0,
        payload,
    }
}

#[test]
fn spans_from_two_envelopes_merged() {
    let t = Uuid::new_v4();
    let batch = vec![
        env(
            t,
            EnvelopePayload::Spans(vec![Span::default(), Span::default()]),
        ),
        env(t, EnvelopePayload::Spans(vec![Span::default()])),
    ];
    let merged = merge_batch(batch);
    assert_eq!(merged.spans.len(), 3);
    assert!(merged.logs.is_empty());
    assert!(merged.series.is_empty());
    assert!(merged.points.is_empty());
    assert!(merged.spans.iter().all(|s| s.tenant_id == t));
}

#[test]
fn logs_from_two_envelopes_merged() {
    let t = Uuid::new_v4();
    let batch = vec![
        env(t, EnvelopePayload::Logs(vec![LogRecord::default()])),
        env(
            t,
            EnvelopePayload::Logs(vec![LogRecord::default(), LogRecord::default()]),
        ),
    ];
    let merged = merge_batch(batch);
    assert_eq!(merged.logs.len(), 3);
    assert!(merged.spans.is_empty());
    assert!(merged.logs.iter().all(|l| l.tenant_id == t));
}

#[test]
fn mixed_envelopes_merged_by_type_and_tenant() {
    let t1 = Uuid::new_v4();
    let t2 = Uuid::new_v4();
    let batch = vec![
        env(t1, EnvelopePayload::Spans(vec![Span::default(); 2])),
        env(t2, EnvelopePayload::Logs(vec![LogRecord::default()])),
        env(
            t1,
            EnvelopePayload::Metrics {
                series: vec![MetricSeries::default()],
                points: vec![MetricPoint::default(); 3],
            },
        ),
    ];
    let merged = merge_batch(batch);
    assert_eq!(merged.spans.len(), 2);
    assert_eq!(merged.logs.len(), 1);
    assert_eq!(merged.series.len(), 1);
    assert_eq!(merged.points.len(), 3);
    assert!(merged.spans.iter().all(|s| s.tenant_id == t1));
    assert!(merged.logs.iter().all(|l| l.tenant_id == t2));
    assert!(merged.series.iter().all(|s| s.tenant_id == t1));
    assert!(merged.points.iter().all(|p| p.tenant_id == t1));
}

#[test]
fn empty_batch_produces_empty_merged() {
    let merged = merge_batch(vec![]);
    assert!(merged.spans.is_empty());
    assert!(merged.logs.is_empty());
    assert!(merged.series.is_empty());
    assert!(merged.points.is_empty());
}
