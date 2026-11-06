use storage_writer::spans::insert_spans;
use uuid::Uuid;

#[tokio::test]
async fn inserts_all_telemetry_tables_and_deduplicates_retries() {
    use std::path::Path;
    use testcontainers::{ImageExt, runners::AsyncRunner};
    use testcontainers_modules::clickhouse::ClickHouse;

    let container = ClickHouse::default()
        .with_tag("26.9")
        .with_env_var("CLICKHOUSE_USER", "default")
        .with_env_var("CLICKHOUSE_PASSWORD", "test")
        .start()
        .await
        .expect("clickhouse container started");
    let port = container.get_host_port_ipv4(8123).await.unwrap();
    let base_url = format!("http://127.0.0.1:{port}");

    // Apply migrations
    let root = clickhouse::Client::default()
        .with_url(&base_url)
        .with_user("default")
        .with_password("test");
    root.query("CREATE DATABASE IF NOT EXISTS observable")
        .execute()
        .await
        .unwrap();

    let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("migrations/clickhouse");
    let mut entries: Vec<_> = std::fs::read_dir(&migrations_dir)
        .expect("migrations/clickhouse must exist")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let sql = std::fs::read_to_string(entry.path()).expect("readable migration");
        for stmt in sql.split(';') {
            let stmt = stmt.trim();
            if !stmt.is_empty() {
                root.query(stmt).execute().await.unwrap();
            }
        }
    }

    let ch = clickhouse::Client::default()
        .with_url(&base_url)
        .with_user("default")
        .with_password("test")
        .with_database("observable")
        .with_compression(clickhouse::Compression::None);

    let tenant_id = Uuid::new_v4();
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    let span = domain::Span {
        tenant_id,
        trace_id: "test-trace-events".into(),
        span_id: "span-with-events".into(),
        start_time_unix_nano: now_ns,
        end_time_unix_nano: now_ns + 1_000_000,
        duration_ns: 1_000_000,
        events: vec![
            domain::SpanEvent {
                tenant_id,
                trace_id: "test-trace-events".into(),
                span_id: "span-with-events".into(),
                event_index: 0,
                name: "exception".into(),
                timestamp_unix_nano: now_ns + 500_000,
                attributes: std::collections::HashMap::new(),
            },
            domain::SpanEvent {
                tenant_id,
                trace_id: "test-trace-events".into(),
                span_id: "span-with-events".into(),
                event_index: 1,
                name: "retry".into(),
                timestamp_unix_nano: now_ns + 750_000,
                attributes: std::collections::HashMap::new(),
            },
        ],
        ..Default::default()
    };

    insert_spans(&ch, vec![span.clone()])
        .await
        .expect("first insert succeeded");
    // Simulate crash-window Kafka redelivery of the identical batch
    insert_spans(&ch, vec![span])
        .await
        .expect("redelivered insert succeeded");

    let span_count: u64 = ch
        .query("SELECT count() FROM spans WHERE tenant_id = ? AND trace_id = 'test-trace-events'")
        .bind(tenant_id)
        .fetch_one()
        .await
        .expect("span count query");
    assert_eq!(span_count, 1, "expected deduplicated single span row");

    // Query span_events to verify 2 rows (not 4)
    let count: u64 = ch
        .query(
            "SELECT count() FROM span_events WHERE tenant_id = ? AND trace_id = 'test-trace-events'",
        )
        .bind(tenant_id)
        .fetch_one()
        .await
        .expect("count query");

    assert_eq!(
        count, 2,
        "expected 2 deduplicated span events to be written"
    );

    // Verify event names
    let names: Vec<String> = ch
        .query(
            "SELECT name FROM span_events WHERE tenant_id = ? AND trace_id = 'test-trace-events' ORDER BY event_index",
        )
        .bind(tenant_id)
        .fetch_all()
        .await
        .expect("names query");

    assert_eq!(names, vec!["exception".to_string(), "retry".to_string()]);

    let log = domain::LogRecord {
        tenant_id,
        log_id: Uuid::new_v4(),
        timestamp_unix_nano: now_ns,
        observed_timestamp_unix_nano: now_ns,
        body: serde_json::json!("deduplication regression"),
        ..Default::default()
    };
    let series = domain::MetricSeries {
        tenant_id,
        metric_series_id: Uuid::new_v4(),
        metric_name: "deduplication_regression".into(),
        service_name: "test".into(),
        ..Default::default()
    };
    let point = domain::MetricPoint {
        tenant_id,
        metric_series_id: series.metric_series_id,
        metric_name: series.metric_name.clone(),
        service_name: series.service_name.clone(),
        time_unix_nano: now_ns,
        value_double: Some(1.0),
        ..Default::default()
    };
    for _ in 0..2 {
        storage_writer::logs::insert_logs(&ch, vec![log.clone()])
            .await
            .expect("log insert succeeded");
        storage_writer::metrics::insert_metric_series(&ch, vec![series.clone()])
            .await
            .expect("series insert succeeded");
        storage_writer::metrics::insert_metric_points(&ch, vec![point.clone()])
            .await
            .expect("point insert succeeded");
    }
    // Startup reapplies migrations: changing settings again must preserve
    // existing rows and the deduplication history.
    let migration =
        std::fs::read_to_string(migrations_dir.join("005_enable_insert_deduplication.sql"))
            .expect("deduplication migration");
    for _ in 0..2 {
        for statement in migration
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            root.query(statement).execute().await.unwrap();
        }
    }
    // Retry after reapplying the settings, too.
    storage_writer::logs::insert_logs(&ch, vec![log])
        .await
        .unwrap();
    storage_writer::metrics::insert_metric_series(&ch, vec![series])
        .await
        .unwrap();
    storage_writer::metrics::insert_metric_points(&ch, vec![point])
        .await
        .unwrap();
    for table in [
        "spans",
        "span_events",
        "logs",
        "metric_series",
        "metric_points",
    ] {
        let count: u64 = ch
            .query(&format!("SELECT count() FROM {table} WHERE tenant_id = ?"))
            .bind(tenant_id)
            .fetch_one()
            .await
            .expect("row count query");
        let expected = if table == "span_events" { 2 } else { 1 };
        assert_eq!(count, expected, "unexpected rows in {table}");
    }
}
