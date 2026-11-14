use std::collections::HashMap;
use std::time::Instant;

use domain::{EnvelopePayload, Span, StatusCode, TelemetryEnvelope};
use observable_process::merge_telemetry;
use uuid::Uuid;

#[test]
fn ingest_and_process_sustains_at_least_10000_spans_per_sec_per_replica() {
    let tenant_id = Uuid::new_v4();
    let total_spans = 20_000usize;
    let batch_size = 100usize;
    let batches: Vec<Vec<TelemetryEnvelope>> = (0..(total_spans / batch_size))
        .map(|b| {
            let spans = (0..batch_size)
                .map(|i| Span {
                    trace_id: format!("{:032x}", b * batch_size + i + 1),
                    span_id: format!("{:016x}", i + 1),
                    service_name: "throughput-bench".into(),
                    operation_name: "GET /health".into(),
                    status_code: StatusCode::Ok,
                    start_time_unix_nano: 1_000_000_000,
                    end_time_unix_nano: 1_002_000_000,
                    attributes: HashMap::from([("http.method".into(), serde_json::json!("GET"))]),
                    ..Default::default()
                })
                .collect();
            vec![TelemetryEnvelope {
                envelope_id: Uuid::new_v4(),
                tenant_id,
                environment: "production".into(),
                received_at_unix_nano: 1_002_000_000,
                payload: EnvelopePayload::Spans(spans),
            }]
        })
        .collect();

    let start = Instant::now();
    let mut processed = 0usize;
    for batch in batches {
        let merged = merge_telemetry(batch);
        processed += merged.spans.len();
    }
    let elapsed = start.elapsed().as_secs_f64();
    let spans_per_sec = processed as f64 / elapsed.max(1e-6);

    assert_eq!(processed, total_spans);
    assert!(
        spans_per_sec >= 10_000.0,
        "sustained throughput {spans_per_sec:.0} spans/sec < 10,000 spans/sec"
    );
}
