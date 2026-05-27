use domain::{EnvelopePayload, LogRecord, MetricPoint, MetricSeries, Span, TelemetryEnvelope};

pub struct MergedBatch {
    pub spans: Vec<Span>,
    pub logs: Vec<LogRecord>,
    pub series: Vec<MetricSeries>,
    pub points: Vec<MetricPoint>,
}

pub fn merge_batch(batch: Vec<TelemetryEnvelope>) -> MergedBatch {
    let mut spans = Vec::new();
    let mut logs = Vec::new();
    let mut series = Vec::new();
    let mut points = Vec::new();
    for env in batch {
        let tid = env.tenant_id;
        match env.payload {
            EnvelopePayload::Spans(ss) => {
                spans.extend(
                    ss.into_iter()
                        .map(|s| crate::normalise::normalise_span(s, tid)),
                );
            }
            EnvelopePayload::Logs(ls) => {
                logs.extend(
                    ls.into_iter()
                        .map(|l| crate::normalise::normalise_log(l, tid)),
                );
            }
            EnvelopePayload::Metrics {
                series: sr,
                points: pt,
            } => {
                series.extend(
                    sr.into_iter()
                        .map(|s| crate::normalise::normalise_metric_series(s, tid)),
                );
                points.extend(
                    pt.into_iter()
                        .map(|p| crate::normalise::normalise_metric_point(p, tid)),
                );
            }
        }
    }
    MergedBatch {
        spans,
        logs,
        series,
        points,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{EnvelopePayload, LogRecord, MetricPoint, MetricSeries, Span, TelemetryEnvelope};
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
    fn empty_batch_produces_empty_merged() {
        let m = merge_batch(vec![]);
        assert!(m.spans.is_empty());
        assert!(m.logs.is_empty());
        assert!(m.series.is_empty());
        assert!(m.points.is_empty());
    }

    #[test]
    fn spans_from_multiple_envelopes_merged() {
        let t = Uuid::new_v4();
        let m = merge_batch(vec![
            env(
                t,
                EnvelopePayload::Spans(vec![Span::default(), Span::default()]),
            ),
            env(t, EnvelopePayload::Spans(vec![Span::default()])),
        ]);
        assert_eq!(m.spans.len(), 3);
        assert!(m.logs.is_empty());
        assert!(m.series.is_empty());
    }

    #[test]
    fn tenant_id_stamped_on_spans() {
        let t = Uuid::new_v4();
        let span = Span {
            tenant_id: Uuid::nil(),
            ..Default::default()
        };
        let m = merge_batch(vec![env(t, EnvelopePayload::Spans(vec![span]))]);
        assert_eq!(m.spans[0].tenant_id, t);
    }

    #[test]
    fn duration_filled_when_zero() {
        let t = Uuid::new_v4();
        let span = Span {
            start_time_unix_nano: 1_000_000_000,
            end_time_unix_nano: 1_005_000_000,
            duration_ns: 0,
            ..Default::default()
        };
        let m = merge_batch(vec![env(t, EnvelopePayload::Spans(vec![span]))]);
        assert_eq!(m.spans[0].duration_ns, 5_000_000);
    }

    #[test]
    fn log_id_assigned_when_nil() {
        let t = Uuid::new_v4();
        let log = LogRecord {
            tenant_id: Uuid::nil(),
            log_id: Uuid::nil(),
            ..Default::default()
        };
        let m = merge_batch(vec![env(t, EnvelopePayload::Logs(vec![log]))]);
        assert_eq!(m.logs[0].tenant_id, t);
        assert_ne!(m.logs[0].log_id, Uuid::nil());
    }

    #[test]
    fn mixed_batch_merged_by_type() {
        let t1 = Uuid::new_v4();
        let t2 = Uuid::new_v4();
        let m = merge_batch(vec![
            env(t1, EnvelopePayload::Spans(vec![Span::default(); 2])),
            env(t2, EnvelopePayload::Logs(vec![LogRecord::default()])),
            env(
                t1,
                EnvelopePayload::Metrics {
                    series: vec![MetricSeries::default()],
                    points: vec![MetricPoint::default(); 3],
                },
            ),
        ]);
        assert_eq!(m.spans.len(), 2);
        assert_eq!(m.logs.len(), 1);
        assert_eq!(m.series.len(), 1);
        assert_eq!(m.points.len(), 3);
        assert!(m.spans.iter().all(|s| s.tenant_id == t1));
        assert!(m.logs.iter().all(|l| l.tenant_id == t2));
        assert!(m.series.iter().all(|s| s.tenant_id == t1));
        assert!(m.points.iter().all(|p| p.tenant_id == t1));
    }
}
