use clickhouse::Client;
use domain::{LogRow, SpanRow};
use query_api::logs::{fetch_log_rows, fetch_log_rows_since, LogSearchParams};
use query_api::planner::QueryPlanner;
use query_api::traces::fetch_trace_spans;
use std::path::Path;
use testcontainers::{runners::AsyncRunner, ImageExt};
use testcontainers_modules::clickhouse::ClickHouse;
use uuid::Uuid;

/// Applies ClickHouse migrations and returns a client scoped to the `observable` database.
async fn apply_migrations(base_url: &str, user: &str, password: &str) -> Client {
    let root = Client::default()
        .with_url(base_url)
        .with_user(user)
        .with_password(password);

    root.query("CREATE DATABASE IF NOT EXISTS observable")
        .execute()
        .await
        .expect("create database");

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
                root.query(stmt)
                    .execute()
                    .await
                    .expect("migration statement applied");
            }
        }
    }

    // Return a client scoped to `observable` for DML operations.
    Client::default()
        .with_url(base_url)
        .with_user(user)
        .with_password(password)
        .with_database("observable")
}

fn make_span(tenant_id: Uuid, trace_id: &str, span_id: &str) -> SpanRow {
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    SpanRow {
        tenant_id,
        trace_id: trace_id.into(),
        span_id: span_id.into(),
        parent_span_id: None,
        service_name: "test-svc".into(),
        service_namespace: String::new(),
        service_version: String::new(),
        operation_name: "op".into(),
        span_kind: "INTERNAL".into(),
        start_time_unix_nano: now_ns,
        end_time_unix_nano: now_ns + 1_000_000,
        duration_ns: 1_000_000,
        status_code: "OK".into(),
        status_message: String::new(),
        attributes: "{}".into(),
        resource_attributes: "{}".into(),
        environment: "test".into(),
        host_id: "host-1".into(),
        workload: String::new(),
        deployment_id: String::new(),
    }
}

/// Insert a span row using a client already scoped to the `observable` database.
async fn insert_span(ch: &Client, row: SpanRow) {
    let mut ins = ch
        .insert::<SpanRow>("spans")
        .await
        .expect("insert handle created");
    ins.write(&row).await.expect("row written");
    ins.end().await.expect("insert committed");
}

#[tokio::test]
async fn clickhouse_container_applies_migrations_and_enforces_tenant_filter() {
    let container = ClickHouse::default()
        .with_tag("24.3")
        .with_env_var("CLICKHOUSE_USER", "default")
        .with_env_var("CLICKHOUSE_PASSWORD", "test")
        .start()
        .await
        .expect("clickhouse container started");

    let port = container.get_host_port_ipv4(8123).await.unwrap();
    let base_url = format!("http://127.0.0.1:{port}");
    let ch = apply_migrations(&base_url, "default", "test").await;

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let trace_id = "trace-tenant-isolation-test";

    insert_span(&ch, make_span(tenant_a, trace_id, "span-a")).await;
    insert_span(&ch, make_span(tenant_b, trace_id, "span-b")).await;

    let result = fetch_trace_spans(&ch, tenant_a, trace_id)
        .await
        .expect("query succeeded");

    assert!(!result.is_empty(), "tenant_a must see their own span");
    assert!(
        result.iter().all(|span| span.tenant_id == tenant_a),
        "all returned spans must belong to tenant_a"
    );
    assert!(
        !result.iter().any(|span| span.tenant_id == tenant_b),
        "no spans from tenant_b must leak into tenant_a results"
    );
}

fn make_log_row(tenant_id: Uuid, service: &str) -> LogRow {
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    LogRow {
        tenant_id,
        log_id: Uuid::new_v4(),
        timestamp_unix_nano: now_ns,
        observed_timestamp_unix_nano: now_ns,
        severity_number: 9,
        severity_text: "INFO".into(),
        body: "{}".into(),
        trace_id: None,
        span_id: None,
        attributes: "{}".into(),
        resource_attributes: "{}".into(),
        service_name: service.into(),
        environment: "test".into(),
        host_id: "host-1".into(),
        fingerprint: None,
    }
}

fn make_log_row_at(tenant_id: Uuid, service: &str, timestamp_unix_nano: u64) -> LogRow {
    let mut row = make_log_row(tenant_id, service);
    row.timestamp_unix_nano = timestamp_unix_nano;
    row.observed_timestamp_unix_nano = timestamp_unix_nano;
    row
}

async fn insert_log(ch: &Client, row: LogRow) {
    let mut ins = ch
        .insert::<LogRow>("logs")
        .await
        .expect("log insert handle created");
    ins.write(&row).await.expect("log row written");
    ins.end().await.expect("log insert committed");
}

#[tokio::test]
async fn clickhouse_container_enforces_tenant_filter_on_logs() {
    let container = ClickHouse::default()
        .with_tag("24.3")
        .with_env_var("CLICKHOUSE_USER", "default")
        .with_env_var("CLICKHOUSE_PASSWORD", "test")
        .start()
        .await
        .expect("clickhouse container started");

    let port = container.get_host_port_ipv4(8123).await.unwrap();
    let base_url = format!("http://127.0.0.1:{port}");
    let ch = apply_migrations(&base_url, "default", "test").await;

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    insert_log(&ch, make_log_row(tenant_a, "svc-a")).await;
    insert_log(&ch, make_log_row(tenant_b, "svc-b")).await;

    let result = fetch_log_rows(&ch, tenant_a)
        .await
        .expect("log query succeeded");

    assert!(!result.is_empty(), "tenant_a must see their own log");
    assert!(
        result.iter().all(|row| row.tenant_id == tenant_a),
        "all returned log rows must belong to tenant_a"
    );
    assert!(
        !result.iter().any(|row| row.tenant_id == tenant_b),
        "no log rows from tenant_b must leak into tenant_a results"
    );
}

#[tokio::test]
async fn clickhouse_container_filters_logs_by_timestamp_cutoff() {
    let container = ClickHouse::default()
        .with_tag("24.3")
        .with_env_var("CLICKHOUSE_USER", "default")
        .with_env_var("CLICKHOUSE_PASSWORD", "test")
        .start()
        .await
        .expect("clickhouse container started");

    let port = container.get_host_port_ipv4(8123).await.unwrap();
    let base_url = format!("http://127.0.0.1:{port}");
    let ch = apply_migrations(&base_url, "default", "test").await;

    let tenant = Uuid::new_v4();

    insert_log(&ch, make_log_row_at(tenant, "old-svc", 1_000)).await;
    insert_log(&ch, make_log_row_at(tenant, "new-svc", 10_000)).await;

    let result = fetch_log_rows_since(&ch, tenant, 5_000)
        .await
        .expect("log query succeeded");

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].service_name, "new-svc");
}

/// Regression test for: Code 60 UNKNOWN_TABLE 'logs'
///
/// The query planner generates `SELECT count() FROM logs WHERE ...` using an unqualified
/// table name that relies on the ClickHouse client's `with_database("observable")` setting.
/// This test verifies that the planner-generated SQL executes without UNKNOWN_TABLE errors
/// against a real ClickHouse instance, covering both the no-filter and timestamp-filter
/// paths (the latter matches the exact query shape from the production error).
#[tokio::test]
async fn planner_count_query_resolves_unqualified_logs_via_database_context() {
    let container = ClickHouse::default()
        .with_tag("24.3")
        .with_env_var("CLICKHOUSE_USER", "default")
        .with_env_var("CLICKHOUSE_PASSWORD", "test")
        .start()
        .await
        .expect("clickhouse container started");

    let port = container.get_host_port_ipv4(8123).await.unwrap();
    let base_url = format!("http://127.0.0.1:{port}");
    let ch = apply_migrations(&base_url, "default", "test").await;

    let tenant = Uuid::new_v4();
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    insert_log(&ch, make_log_row_at(tenant, "svc", now_ns - 2_000_000_000)).await;
    insert_log(&ch, make_log_row_at(tenant, "svc", now_ns - 1_000_000_000)).await;
    insert_log(&ch, make_log_row_at(tenant, "svc", now_ns)).await;

    let planner = QueryPlanner;

    // Path 1: no time filter → "SELECT count() FROM logs WHERE tenant_id = ?"
    let plan_all = planner.plan_log_search(&LogSearchParams {
        service: None,
        severity: None,
        trace_id: None,
        span_id: None,
        limit: None,
        facets: None,
        from: None,
        to: None,
    });
    let total = ch
        .query(&plan_all.count_sql)
        .bind(tenant)
        .fetch_one::<u64>()
        .await
        .expect("SELECT count() FROM logs (unqualified) must resolve via database context");
    assert_eq!(total, 3);

    // Path 2: with timestamp cutoff → "SELECT count() FROM logs WHERE tenant_id = ? AND timestamp_unix_nano >= ?"
    // This mirrors the exact production error shape.
    let cutoff_ns = now_ns - 1_500_000_000; // 1.5 s ago: 2 logs are newer than this
    let cutoff_dt = chrono::DateTime::UNIX_EPOCH + chrono::Duration::nanoseconds(cutoff_ns as i64);
    let plan_from = planner.plan_log_search(&LogSearchParams {
        service: None,
        severity: None,
        trace_id: None,
        span_id: None,
        limit: None,
        facets: None,
        from: Some(cutoff_dt),
        to: None,
    });
    let filtered = ch
        .query(&plan_from.count_sql)
        .bind(tenant)
        .bind(cutoff_ns)
        .fetch_one::<u64>()
        .await
        .expect("SELECT count() FROM logs with timestamp filter must not return UNKNOWN_TABLE");
    assert_eq!(filtered, 2, "only logs after cutoff should be counted");
}
