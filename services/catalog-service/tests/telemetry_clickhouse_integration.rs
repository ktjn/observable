//! store-telemetry adapter of the coverage assessment against real ClickHouse (shared
//! testcontainer, repository migrations): latest data per signal for one telemetry identity.

use catalog_service::ports::Telemetry;
use chrono::{Duration, Utc};
use uuid::Uuid;

fn ns(minutes_ago: i64) -> u64 {
    (Utc::now() - Duration::minutes(minutes_ago))
        .timestamp_nanos_opt()
        .unwrap() as u64
}

#[tokio::test]
async fn clickhouse_adapter_reports_recent_signals_per_identity() {
    let ch = test_support::clickhouse::shared_client().await;
    let tenant = Uuid::new_v4();
    let recent = ns(5);
    let old = ns(300);
    let exec = |sql: String| {
        let ch = ch.clone();
        async move { ch.query(&sql).execute().await.expect("insert") }
    };
    exec(format!(
        "INSERT INTO observable.spans (tenant_id, trace_id, span_id, service_name, operation_name, \
         start_time_unix_nano, end_time_unix_nano, duration_ns, environment) VALUES \
         ('{tenant}', 't1', 's1', 'payments-api', 'GET /pay', {recent}, {recent}, 1, 'prod'), \
         ('{tenant}', 't2', 's2', 'payments-api', 'GET /pay', {recent}, {recent}, 1, 'staging')"
    ))
    .await;
    exec(format!(
        "INSERT INTO observable.logs (tenant_id, log_id, timestamp_unix_nano, observed_timestamp_unix_nano, \
         severity_number, body, service_name, environment) VALUES \
         ('{tenant}', generateUUIDv4(), {old}, {old}, 9, 'old', 'payments-api', 'prod')"
    ))
    .await;
    let series = Uuid::new_v4();
    exec(format!(
        "INSERT INTO observable.metric_series (tenant_id, metric_series_id, metric_name, metric_type, \
         service_name, environment) VALUES ('{tenant}', '{series}', 'http_requests', 'sum', 'payments-api', 'prod')"
    ))
    .await;
    exec(format!(
        "INSERT INTO observable.metric_points (tenant_id, metric_series_id, metric_name, service_name, \
         time_unix_nano, value_double) VALUES ('{tenant}', '{series}', 'http_requests', 'payments-api', {recent}, 1)"
    ))
    .await;

    let telemetry = Telemetry::ClickHouse(Box::new(ch.clone()));
    let since = ns(60);
    let prod = telemetry
        .seen_since(tenant, "payments-api", "prod", since)
        .await
        .expect("read");
    assert_eq!(prod.traces, Some(recent));
    assert_eq!(prod.logs, None, "data outside the window is not evidence");
    assert_eq!(prod.metrics, Some(recent));

    let qa = telemetry
        .seen_since(tenant, "payments-api", "qa", since)
        .await
        .expect("read");
    assert_eq!((qa.traces, qa.logs, qa.metrics), (None, None, None));

    let any = telemetry
        .seen_since(tenant, "payments-api", "", since)
        .await
        .expect("read");
    assert_eq!(any.traces, Some(recent));
    assert_eq!(any.metrics, Some(recent));

    let other = telemetry
        .seen_since(Uuid::new_v4(), "payments-api", "", since)
        .await
        .expect("read");
    assert_eq!(
        (other.traces, other.logs, other.metrics),
        (None, None, None)
    );
}
