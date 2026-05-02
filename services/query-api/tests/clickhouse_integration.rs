use clickhouse::Client;
use domain::{LogRow, SpanRow};
use query_api::logs::{fetch_log_rows, fetch_log_rows_since, LogSearchParams};
use query_api::planner::QueryPlanner;
use query_api::traces::fetch_trace_spans;
use std::collections::HashSet;
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

/// Executes the histogram query against a real ClickHouse client and returns
/// `(bucket_idx, severity_number, count)` tuples for all matched rows.
///
/// `bucket_idx` is `i64` because ClickHouse's `intDiv(UInt64, UInt64)` returns `Int64`.
async fn run_histogram(
    ch: &Client,
    tenant_id: Uuid,
    from_ns: u64,
    to_ns: u64,
    service: Option<&str>,
    bucket_count: u32,
) -> Vec<(i64, i32, u64)> {
    let planner = QueryPlanner;
    let plan = planner.plan_log_histogram(from_ns, to_ns, service, bucket_count);
    let mut query = ch
        .query(&plan.sql)
        .bind(plan.from_ns)
        .bind(plan.interval_ns)
        .bind(tenant_id)
        .bind(from_ns)
        .bind(to_ns);
    if let Some(svc) = service {
        query = query.bind(svc);
    }
    query
        .fetch_all::<(i64, i32, u64)>()
        .await
        .expect("histogram query succeeded")
}

/// Executes the trace histogram query using the same tuple shape as the handler.
async fn run_trace_histogram(
    ch: &Client,
    tenant_id: Uuid,
    from_ns: u64,
    to_ns: u64,
    service: Option<&str>,
    bucket_count: u32,
) -> Vec<(i64, i32, u64)> {
    let planner = QueryPlanner;
    let plan = planner.plan_trace_histogram(from_ns, to_ns, service, bucket_count);
    let mut query = ch
        .query(&plan.sql)
        .bind(plan.from_ns)
        .bind(plan.interval_ns)
        .bind(tenant_id)
        .bind(from_ns)
        .bind(to_ns);
    if let Some(svc) = service {
        query = query.bind(svc);
    }
    query
        .fetch_all::<(i64, i32, u64)>()
        .await
        .expect("trace histogram query must decode using handler tuple shape")
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

// ─── Histogram integration tests ────────────────────────────────────────────

#[tokio::test]
async fn log_histogram_empty_range_returns_no_rows() {
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
    let rows = run_histogram(&ch, tenant, now_ns - 3_600_000_000_000, now_ns, None, 30).await;

    assert!(
        rows.is_empty(),
        "no logs inserted — histogram must be empty"
    );
}

#[tokio::test]
async fn log_histogram_counts_logs_in_correct_buckets() {
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
    // Use recent timestamps to avoid ClickHouse TTL expiry (TTL = 60 days).
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let base = now_ns - 3_000_000_000; // 3 seconds ago
    insert_log(&ch, make_log_row_at(tenant, "svc", base)).await;
    insert_log(&ch, make_log_row_at(tenant, "svc", base + 1_000_000_000)).await;
    insert_log(&ch, make_log_row_at(tenant, "svc", base + 2_000_000_000)).await;

    let from_ns = base - 1;
    let to_ns = base + 3_000_000_001;
    let rows = run_histogram(&ch, tenant, from_ns, to_ns, None, 3).await;

    let total: u64 = rows.iter().map(|(_, _, cnt)| cnt).sum();
    assert_eq!(total, 3, "all 3 inserted logs must be counted");
}

#[tokio::test]
async fn log_histogram_service_filter_excludes_other_services() {
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
    insert_log(
        &ch,
        make_log_row_at(tenant, "svc-a", now_ns - 2_000_000_000),
    )
    .await;
    insert_log(
        &ch,
        make_log_row_at(tenant, "svc-b", now_ns - 1_000_000_000),
    )
    .await;

    let from_ns = now_ns - 3_000_000_000;
    let to_ns = now_ns + 1_000_000_000;
    let rows = run_histogram(&ch, tenant, from_ns, to_ns, Some("svc-a"), 30).await;

    let total: u64 = rows.iter().map(|(_, _, cnt)| cnt).sum();
    assert_eq!(total, 1, "only svc-a log must be counted");
}

#[tokio::test]
async fn log_histogram_tenant_isolation() {
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
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    insert_log(
        &ch,
        make_log_row_at(tenant_a, "svc", now_ns - 2_000_000_000),
    )
    .await;
    insert_log(
        &ch,
        make_log_row_at(tenant_a, "svc", now_ns - 1_000_000_000),
    )
    .await;
    insert_log(
        &ch,
        make_log_row_at(tenant_b, "svc", now_ns - 1_500_000_000),
    )
    .await;

    let from_ns = now_ns - 3_000_000_000;
    let to_ns = now_ns + 1_000_000_000;
    let rows = run_histogram(&ch, tenant_a, from_ns, to_ns, None, 30).await;

    let total: u64 = rows.iter().map(|(_, _, cnt)| cnt).sum();
    assert_eq!(total, 2, "only tenant_a logs must be counted");
}

#[tokio::test]
async fn log_histogram_bucket_count_param_changes_granularity() {
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
    // Use recent timestamps spaced 1 second apart to avoid TTL expiry.
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let base = now_ns - 7_000_000_000;
    for i in 1u64..=6 {
        insert_log(
            &ch,
            make_log_row_at(tenant, "svc", base + i * 1_000_000_000),
        )
        .await;
    }

    let from_ns = base;
    let to_ns = base + 7_000_000_001;

    let rows6 = run_histogram(&ch, tenant, from_ns, to_ns, None, 6).await;
    let distinct6: HashSet<i64> = rows6.iter().map(|(idx, _, _)| *idx).collect();
    assert_eq!(
        distinct6.len(),
        6,
        "6 buckets should produce 6 distinct bucket indices"
    );

    let rows3 = run_histogram(&ch, tenant, from_ns, to_ns, None, 3).await;
    let distinct3: HashSet<i64> = rows3.iter().map(|(idx, _, _)| *idx).collect();
    assert!(
        distinct3.len() <= 3,
        "3 buckets should produce at most 3 distinct bucket indices, got {}",
        distinct3.len()
    );
}

#[tokio::test]
async fn log_histogram_severity_distribution() {
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

    // INFO (9)
    let mut info = make_log_row_at(tenant, "svc", now_ns - 3_000_000_000);
    info.severity_number = 9;
    info.severity_text = "INFO".into();
    insert_log(&ch, info).await;

    // WARN (13)
    let mut warn = make_log_row_at(tenant, "svc", now_ns - 2_000_000_000);
    warn.severity_number = 13;
    warn.severity_text = "WARN".into();
    insert_log(&ch, warn).await;

    // ERROR (17)
    let mut error = make_log_row_at(tenant, "svc", now_ns - 1_000_000_000);
    error.severity_number = 17;
    error.severity_text = "ERROR".into();
    insert_log(&ch, error).await;

    let from_ns = now_ns - 4_000_000_000;
    let to_ns = now_ns + 1_000_000_000;
    let rows = run_histogram(&ch, tenant, from_ns, to_ns, None, 30).await;

    assert!(
        rows.iter().any(|(_, sev, _)| *sev == 9),
        "INFO severity (9) must appear"
    );
    assert!(
        rows.iter().any(|(_, sev, _)| *sev == 13),
        "WARN severity (13) must appear"
    );
    assert!(
        rows.iter().any(|(_, sev, _)| *sev == 17),
        "ERROR severity (17) must appear"
    );
}

#[tokio::test]
async fn trace_histogram_dummy_column_decodes_as_i32() {
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
    let mut span = make_span(tenant, "trace-histogram-decode", "span-1");
    span.service_name = "svc".into();
    span.start_time_unix_nano = now_ns - 1_000_000_000;
    span.end_time_unix_nano = now_ns;
    insert_span(&ch, span).await;

    let rows = run_trace_histogram(
        &ch,
        tenant,
        now_ns - 2_000_000_000,
        now_ns + 1_000_000_000,
        Some("svc"),
        30,
    )
    .await;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1, 1);
    assert_eq!(rows[0].2, 1);
}

// ─── Log search / context integration tests ─────────────────────────────────

#[tokio::test]
async fn log_search_service_filter() {
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
    insert_log(
        &ch,
        make_log_row_at(tenant, "svc-a", now_ns - 2_000_000_000),
    )
    .await;
    insert_log(
        &ch,
        make_log_row_at(tenant, "svc-a", now_ns - 1_000_000_000),
    )
    .await;
    insert_log(&ch, make_log_row_at(tenant, "svc-b", now_ns)).await;

    let planner = QueryPlanner;
    let plan = planner.plan_log_search(&LogSearchParams {
        service: Some("svc-a".into()),
        severity: None,
        trace_id: None,
        span_id: None,
        limit: None,
        facets: None,
        from: None,
        to: None,
    });

    let rows: Vec<LogRow> = ch
        .query(&plan.logs_sql)
        .bind(tenant)
        .bind("svc-a")
        .bind(plan.limit)
        .fetch_all()
        .await
        .expect("log search with service filter must succeed");

    assert_eq!(rows.len(), 2, "only svc-a logs should be returned");
    assert!(
        rows.iter().all(|r| r.service_name == "svc-a"),
        "all rows must belong to svc-a"
    );
}

#[tokio::test]
async fn log_search_severity_filter() {
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

    // INFO log — severity 9, below the threshold of 13
    let mut info = make_log_row_at(tenant, "svc", now_ns - 2_000_000_000);
    info.severity_number = 9;
    info.severity_text = "INFO".into();
    insert_log(&ch, info).await;

    // ERROR log — severity 17, above threshold
    let mut error = make_log_row_at(tenant, "svc", now_ns - 1_000_000_000);
    error.severity_number = 17;
    error.severity_text = "ERROR".into();
    insert_log(&ch, error).await;

    let planner = QueryPlanner;
    let plan = planner.plan_log_search(&LogSearchParams {
        service: None,
        severity: Some(13),
        trace_id: None,
        span_id: None,
        limit: None,
        facets: None,
        from: None,
        to: None,
    });

    let rows: Vec<LogRow> = ch
        .query(&plan.logs_sql)
        .bind(tenant)
        .bind(13i32)
        .bind(plan.limit)
        .fetch_all()
        .await
        .expect("log search with severity filter must succeed");

    assert_eq!(rows.len(), 1, "only the ERROR log must be returned");
    assert_eq!(rows[0].severity_number, 17);
}

#[tokio::test]
async fn log_context_returns_surrounding_logs() {
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
    // Use recent timestamps spaced 1 second apart to avoid ClickHouse TTL expiry.
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let ts: Vec<u64> = (0..5)
        .map(|i| now_ns - 5_000_000_000 + i * 1_000_000_000)
        .collect();
    // ts[0] = now-5s, ts[1] = now-4s, ts[2] = now-3s (pivot), ts[3] = now-2s, ts[4] = now-1s
    // Insert non-pivot rows first
    for &t in ts.iter().filter(|&&t| t != ts[2]) {
        insert_log(&ch, make_log_row_at(tenant, "svc", t)).await;
    }
    let pivot_ts = ts[2];

    // Insert pivot row separately so we can capture its log_id
    let pivot_row_data = make_log_row_at(tenant, "svc", pivot_ts);
    let pivot_log_id = pivot_row_data.log_id;
    insert_log(&ch, pivot_row_data).await;

    // We need service and host from the actual inserted row; use make_log_row defaults.
    let svc = "svc".to_string();
    let host = "host-1".to_string(); // default from make_log_row

    // Fetch logs BEFORE the pivot
    let before_rows: Vec<LogRow> = ch
        .query(
            "SELECT ?fields FROM observable.logs \
             WHERE tenant_id = ? AND service_name = ? AND host_id = ? \
             AND timestamp_unix_nano < ? \
             ORDER BY timestamp_unix_nano DESC LIMIT ?",
        )
        .bind(tenant)
        .bind(&svc)
        .bind(&host)
        .bind(pivot_ts)
        .bind(10u32)
        .fetch_all()
        .await
        .expect("before query succeeded");

    // Fetch logs AFTER the pivot
    let after_rows: Vec<LogRow> = ch
        .query(
            "SELECT ?fields FROM observable.logs \
             WHERE tenant_id = ? AND service_name = ? AND host_id = ? \
             AND timestamp_unix_nano > ? \
             ORDER BY timestamp_unix_nano ASC LIMIT ?",
        )
        .bind(tenant)
        .bind(&svc)
        .bind(&host)
        .bind(pivot_ts)
        .bind(10u32)
        .fetch_all()
        .await
        .expect("after query succeeded");

    assert_eq!(
        before_rows.len(),
        2,
        "2 logs before pivot (ts[0] and ts[1])"
    );
    assert_eq!(after_rows.len(), 2, "2 logs after pivot (ts[3] and ts[4])");

    // before is DESC — first element is ts[1], second is ts[0]
    assert_eq!(before_rows[0].timestamp_unix_nano, ts[1]);
    assert_eq!(before_rows[1].timestamp_unix_nano, ts[0]);

    // after is ASC — ts[3] then ts[4]
    assert_eq!(after_rows[0].timestamp_unix_nano, ts[3]);
    assert_eq!(after_rows[1].timestamp_unix_nano, ts[4]);

    // Fetch pivot row by log_id and assemble full context
    let fetched_pivot: LogRow = ch
        .query("SELECT ?fields FROM observable.logs WHERE tenant_id = ? AND log_id = ?")
        .bind(tenant)
        .bind(pivot_log_id)
        .fetch_optional::<LogRow>()
        .await
        .expect("pivot fetch succeeded")
        .expect("pivot row found");

    let mut reversed_before = before_rows.clone();
    reversed_before.reverse();
    let mut all = reversed_before;
    all.push(fetched_pivot.clone());
    all.extend(after_rows.clone());

    assert_eq!(all.len(), 5, "full context should be 5 rows");
    assert_eq!(
        all[2].timestamp_unix_nano, pivot_ts,
        "pivot is in the middle"
    );
    assert!(
        all[0].timestamp_unix_nano < all[1].timestamp_unix_nano,
        "before rows in ascending order after reverse"
    );
    assert!(
        all[3].timestamp_unix_nano < all[4].timestamp_unix_nano,
        "after rows in ascending order"
    );
}
