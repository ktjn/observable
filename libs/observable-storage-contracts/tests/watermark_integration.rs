//! Ingest watermark (phase 2 E5) against real ClickHouse: per tenant, per signal, and never
//! readable across tenants.

use std::time::{SystemTime, UNIX_EPOCH};

use observable_storage_contracts::watermark::{
    IngestWatermark, SKEW_ALLOWANCE_NS, WatermarkStatus, ingest_watermark,
};
use uuid::Uuid;

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

const S: u64 = 1_000_000_000;

#[tokio::test]
async fn the_watermark_is_per_tenant_and_ignores_future_dated_rows() {
    let ch = test_support::clickhouse::shared_client().await;
    let (tenant, other, empty) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let now = now_ns();
    let (span_t, log_t, metric_t) = (now - 120 * S, now - 30 * S, now - 600 * S);
    let future = now + SKEW_ALLOWANCE_NS + 3600 * S;
    let exec = |sql: String| {
        let ch = ch.clone();
        async move { ch.query(&sql).execute().await.expect("insert") }
    };
    exec(format!(
        "INSERT INTO observable.spans (tenant_id, trace_id, span_id, service_name, operation_name, \
         start_time_unix_nano, end_time_unix_nano, duration_ns, environment) VALUES \
         ('{tenant}', 't1', 's1', 'svc', 'op', {span_t}, {span_t}, 1, 'prod'), \
         ('{tenant}', 't2', 's2', 'svc', 'op', {future}, {future}, 1, 'prod'), \
         ('{other}', 't3', 's3', 'svc', 'op', {now}, {now}, 1, 'prod')"
    ))
    .await;
    exec(format!(
        "INSERT INTO observable.logs (tenant_id, log_id, timestamp_unix_nano, observed_timestamp_unix_nano, \
         severity_number, body, service_name, environment) VALUES \
         ('{tenant}', generateUUIDv4(), {log_t}, {log_t}, 9, 'b', 'svc', 'prod'), \
         ('{other}', generateUUIDv4(), {now}, {now}, 9, 'b', 'svc', 'prod')"
    ))
    .await;
    let series = Uuid::new_v4();
    exec(format!(
        "INSERT INTO observable.metric_points (tenant_id, metric_series_id, metric_name, service_name, \
         time_unix_nano, value_double) VALUES ('{tenant}', '{series}', 'm', 'svc', {metric_t}, 1)"
    ))
    .await;

    let since = now - 3600 * S;
    let w = ingest_watermark(&ch, tenant, since, now).await.unwrap();
    assert_eq!(
        w,
        IngestWatermark {
            traces: Some(span_t),
            logs: Some(log_t),
            metrics: Some(metric_t),
        }
    );
    assert_eq!(w.status(now, 300 * S), WatermarkStatus::Current);
    // An earlier read time still keeps observations within the skew allowance.
    let older = ingest_watermark(&ch, tenant, since, now - 100 * S)
        .await
        .unwrap();
    assert_eq!(
        older.logs,
        Some(log_t),
        "skew allowance keeps the log in range"
    );

    // The other tenant's newer data never shows up, and a tenant without data is missing.
    let o = ingest_watermark(&ch, other, since, now).await.unwrap();
    assert_eq!((o.traces, o.logs, o.metrics), (Some(now), Some(now), None));
    let e = ingest_watermark(&ch, empty, since, now).await.unwrap();
    assert_eq!(e, IngestWatermark::default());
    assert_eq!(e.status(now, 300 * S), WatermarkStatus::Missing);

    // Data only before the bound: delayed, not current.
    let late = ingest_watermark(&ch, tenant, since, now).await.unwrap();
    assert_eq!(
        late.status(now + 900 * S, 300 * S),
        WatermarkStatus::Delayed
    );

    // A read-only session can read it.
    let ro = ch.clone().with_setting("readonly", "2");
    assert_eq!(ingest_watermark(&ro, tenant, since, now).await.unwrap(), w);
}
