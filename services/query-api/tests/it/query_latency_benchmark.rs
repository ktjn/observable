use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use observable_storage_contracts::{LogRow, MetricPointRow, MetricSeriesRow, SpanRow};
use query_api::logs::fetch_log_rows_since;
use query_api::traces::fetch_trace_spans;
use uuid::Uuid;

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

fn p99_ms(samples: &mut [Duration]) -> f64 {
    samples.sort_unstable();
    let idx = ((samples.len() as f64) * 0.99).ceil() as usize;
    let clamped = idx.saturating_sub(1).min(samples.len().saturating_sub(1));
    samples[clamped].as_secs_f64() * 1000.0
}

#[tokio::test]
async fn tenant_scoped_1h_queries_complete_within_p99_500ms() {
    let ch = test_support::clickhouse::shared_client().await;
    let tenant_id = Uuid::new_v4();
    let base_ns = now_ns();
    let trace_id = "4bf92f3577b34da6a3ce929d0e0e4736";

    // Seed spans, logs, and metric series/points within a 1-hour window.
    let mut span_ins = ch.insert::<SpanRow>("spans").await.expect("span insert");
    for i in 0..50u64 {
        let row = SpanRow {
            tenant_id,
            trace_id: trace_id.into(),
            span_id: format!("{:016x}", i + 1),
            parent_span_id: None,
            service_name: "bench-checkout".into(),
            service_namespace: "prod".into(),
            service_version: "1.0.0".into(),
            operation_name: "POST /checkout".into(),
            span_kind: "SERVER".into(),
            start_time_unix_nano: base_ns - i * 1_000_000_000,
            end_time_unix_nano: base_ns - i * 1_000_000_000 + 5_000_000,
            duration_ns: 5_000_000,
            status_code: "OK".into(),
            status_message: String::new(),
            attributes: "{}".into(),
            resource_attributes: "{}".into(),
            environment: "production".into(),
            host_id: "bench-host-1".into(),
            workload: "checkout".into(),
            deployment_id: "dep-1".into(),
        };
        span_ins.write(&row).await.expect("write span");
    }
    span_ins.end().await.expect("end span insert");

    let mut log_ins = ch.insert::<LogRow>("logs").await.expect("log insert");
    for i in 0..50u64 {
        let row = LogRow {
            tenant_id,
            log_id: Uuid::new_v4(),
            timestamp_unix_nano: base_ns - i * 1_000_000_000,
            observed_timestamp_unix_nano: base_ns - i * 1_000_000_000,
            severity_number: 9,
            severity_text: "INFO".into(),
            body: format!("bench log line {i}"),
            trace_id: Some(trace_id.into()),
            span_id: Some("0000000000000001".into()),
            attributes: "{}".into(),
            resource_attributes: "{}".into(),
            service_name: "bench-checkout".into(),
            environment: "production".into(),
            host_id: "bench-host-1".into(),
            fingerprint: Some(42),
        };
        log_ins.write(&row).await.expect("write log");
    }
    log_ins.end().await.expect("end log insert");

    let metric_series_id = Uuid::new_v4();
    let mut series_ins = ch
        .insert::<MetricSeriesRow>("metric_series")
        .await
        .expect("series insert");
    series_ins
        .write(&MetricSeriesRow {
            tenant_id,
            metric_series_id,
            metric_name: "http_server_requests_total".into(),
            description: "Total requests".into(),
            unit: "1".into(),
            metric_type: "sum".into(),
            is_monotonic: Some(1),
            aggregation_temporality: Some("delta".into()),
            attributes: r#"{"service":"bench-checkout"}"#.into(),
            resource_attributes: "{}".into(),
            service_name: "bench-checkout".into(),
            environment: "production".into(),
        })
        .await
        .expect("write series");
    series_ins.end().await.expect("end series insert");

    let mut pt_ins = ch
        .insert::<MetricPointRow>("metric_points")
        .await
        .expect("point insert");
    for i in 0..50u64 {
        pt_ins
            .write(&MetricPointRow {
                tenant_id,
                metric_series_id,
                metric_name: "http_server_requests_total".into(),
                service_name: "bench-checkout".into(),
                time_unix_nano: base_ns - i * 60_000_000_000,
                start_time_unix_nano: Some(base_ns - (i + 1) * 60_000_000_000),
                value_double: Some(100.0 + i as f64),
                value_int: None,
                histogram_count: None,
                histogram_sum: None,
                histogram_bucket_counts: Vec::new(),
                histogram_explicit_bounds: Vec::new(),
            })
            .await
            .expect("write point");
    }
    pt_ins.end().await.expect("end point insert");

    let start_ns = base_ns.saturating_sub(3_600_000_000_000);
    let end_ns = base_ns + 1_000_000_000;

    let mut trace_latencies = Vec::with_capacity(20);
    let mut log_latencies = Vec::with_capacity(20);
    let mut metric_latencies = Vec::with_capacity(20);

    for _ in 0..20 {
        let t0 = Instant::now();
        let spans = fetch_trace_spans(&ch, tenant_id, trace_id)
            .await
            .expect("trace query");
        assert!(!spans.is_empty());
        trace_latencies.push(t0.elapsed());

        let t1 = Instant::now();
        let logs = fetch_log_rows_since(&ch, tenant_id, start_ns)
            .await
            .expect("log query");
        assert!(!logs.is_empty());
        log_latencies.push(t1.elapsed());

        let t2 = Instant::now();
        let count: u64 = ch
            .query(
                "SELECT count() FROM metric_points \
                 WHERE tenant_id = ? AND metric_series_id = ? AND time_unix_nano >= ? AND time_unix_nano <= ?",
            )
            .bind(tenant_id)
            .bind(metric_series_id)
            .bind(start_ns)
            .bind(end_ns)
            .fetch_one()
            .await
            .expect("metric query");
        assert!(count > 0);
        metric_latencies.push(t2.elapsed());
    }

    let trace_p99 = p99_ms(&mut trace_latencies);
    let log_p99 = p99_ms(&mut log_latencies);
    let metric_p99 = p99_ms(&mut metric_latencies);
    assert!(
        trace_p99 <= 500.0 && log_p99 <= 500.0 && metric_p99 <= 500.0,
        "1h query p99 exceeded 500ms: trace={trace_p99:.2}ms log={log_p99:.2}ms metric={metric_p99:.2}ms"
    );
}
