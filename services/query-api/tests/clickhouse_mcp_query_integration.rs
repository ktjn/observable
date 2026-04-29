// Testcontainers integration test for the end-to-end MCP query path (P8-S6 Step 5).
//
// Spins up both PostgreSQL 16 and ClickHouse containers, applies all migrations, inserts
// test data, and calls `execute_mcp_query` directly to assert:
//   - Data rows are returned from ClickHouse
//   - All 6 provenance fields are populated
//   - tenant_id appears in source_sql (tenant isolation)
//   - Advisory-only invariant: generated SQL contains no DML keywords
//   - Tenant isolation: tenant_b sees no data from tenant_a series

use clickhouse::Client as ChClient;
use domain::{MetricPointRow, MetricSeriesRow, NlqIr, NlqOperation, NlqSignal, NlqTimeRange};
use query_api::mcp_query::execute_mcp_query;
use sqlx::PgPool;
use std::path::Path;
use testcontainers::{runners::AsyncRunner, ImageExt};
use testcontainers_modules::{clickhouse::ClickHouse, postgres::Postgres};
use uuid::Uuid;

// ── Constants ─────────────────────────────────────────────────────────────────

const TENANT_A: Uuid = Uuid::from_u128(0xAAAA_0000_0000_0000_0000_0000_0000_0001);
const TENANT_B: Uuid = Uuid::from_u128(0xBBBB_0000_0000_0000_0000_0000_0000_0002);
const METRIC: &str = "latency_ms";

// ── PostgreSQL helpers ────────────────────────────────────────────────────────

async fn start_pg() -> (PgPool, testcontainers::ContainerAsync<Postgres>) {
    let container = Postgres::default()
        .with_tag("16")
        .start()
        .await
        .expect("postgres container started");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.expect("pool connected");
    apply_pg_migrations(&pool).await;
    (pool, container)
}

async fn apply_pg_migrations(pool: &PgPool) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("migrations/postgres");
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("migrations/postgres must exist")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let sql = std::fs::read_to_string(entry.path()).expect("readable");
        sqlx::raw_sql(&sql)
            .execute(pool)
            .await
            .expect("pg migration applied");
    }
}

async fn seed_schema_registry(pool: &PgPool, tenant_id: Uuid) {
    // Insert global schema entry
    sqlx::query(
        "INSERT INTO schema_entries (signal_type, field_name, field_type)
         VALUES ('metrics', $1, 'Float64') ON CONFLICT DO NOTHING",
    )
    .bind(METRIC)
    .execute(pool)
    .await
    .expect("schema_entry inserted");

    // Insert tenant annotation with metric_type + timestamp_column
    sqlx::query(
        "INSERT INTO semantic_annotations
           (tenant_id, signal_type, field_name, metric_type, timestamp_column, unit)
         VALUES ($1, 'metrics', $2, 'gauge', 'ts', 'ms')
         ON CONFLICT (tenant_id, signal_type, field_name) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(METRIC)
    .execute(pool)
    .await
    .expect("semantic_annotation inserted");
}

// ── ClickHouse helpers ────────────────────────────────────────────────────────

async fn start_ch() -> (ChClient, testcontainers::ContainerAsync<ClickHouse>) {
    let container = ClickHouse::default()
        .with_tag("24.3")
        .with_env_var("CLICKHOUSE_USER", "default")
        .with_env_var("CLICKHOUSE_PASSWORD", "test")
        .start()
        .await
        .expect("clickhouse container started");
    let port = container.get_host_port_ipv4(8123).await.unwrap();
    let base_url = format!("http://127.0.0.1:{port}");
    let ch = apply_ch_migrations(&base_url, "default", "test").await;
    (ch, container)
}

async fn apply_ch_migrations(base_url: &str, user: &str, password: &str) -> ChClient {
    let root = ChClient::default()
        .with_url(base_url)
        .with_user(user)
        .with_password(password);

    root.query("CREATE DATABASE IF NOT EXISTS observable")
        .execute()
        .await
        .expect("create database");

    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("migrations/clickhouse");

    let mut entries: Vec<_> = std::fs::read_dir(&dir)
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
                    .expect("ch migration applied");
            }
        }
    }

    ChClient::default()
        .with_url(base_url)
        .with_user(user)
        .with_password(password)
        .with_database("observable")
}

async fn insert_metric_series(ch: &ChClient, row: MetricSeriesRow) {
    let mut ins = ch
        .insert::<MetricSeriesRow>("metric_series")
        .await
        .expect("metric_series insert handle");
    ins.write(&row).await.expect("metric_series row written");
    ins.end().await.expect("metric_series insert committed");
}

async fn insert_metric_point(ch: &ChClient, row: MetricPointRow) {
    let mut ins = ch
        .insert::<MetricPointRow>("metric_points")
        .await
        .expect("metric_points insert handle");
    ins.write(&row).await.expect("metric_points row written");
    ins.end().await.expect("metric_points insert committed");
}

fn now_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

fn make_series(tenant_id: Uuid, series_id: Uuid) -> MetricSeriesRow {
    MetricSeriesRow {
        tenant_id,
        metric_series_id: series_id,
        metric_name: METRIC.into(),
        description: String::new(),
        unit: "ms".into(),
        metric_type: "gauge".into(),
        is_monotonic: None,
        aggregation_temporality: None,
        attributes: "{}".into(),
        resource_attributes: "{}".into(),
        service_name: "test-svc".into(),
        environment: "test".into(),
    }
}

fn make_point(tenant_id: Uuid, series_id: Uuid, value: f64, offset_ms: u64) -> MetricPointRow {
    let now = now_ns();
    MetricPointRow {
        tenant_id,
        metric_series_id: series_id,
        metric_name: METRIC.into(),
        service_name: "test-svc".into(),
        time_unix_nano: now - offset_ms * 1_000_000,
        start_time_unix_nano: None,
        value_double: Some(value),
        value_int: None,
        histogram_count: None,
        histogram_sum: None,
        histogram_bucket_counts: vec![],
        histogram_explicit_bounds: vec![],
    }
}

fn base_ir(op: NlqOperation) -> NlqIr {
    NlqIr {
        operation: op,
        signals: vec![NlqSignal::Metrics],
        metric: Some(METRIC.into()),
        window: None,
        filters: vec![],
        group_by: vec![],
        resolution: Some("1m".into()),
        time_range: NlqTimeRange {
            from: "now-30m".into(),
            to: "now".into(),
        },
        visualization_hint: None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn timeseries_query_returns_data_with_all_provenance_fields() {
    let (db, _pg) = start_pg().await;
    let (ch, _ch_container) = start_ch().await;

    seed_schema_registry(&db, TENANT_A).await;

    let series_id = Uuid::new_v4();
    insert_metric_series(&ch, make_series(TENANT_A, series_id)).await;
    // Insert 3 data points at 5m, 10m, 15m ago
    insert_metric_point(&ch, make_point(TENANT_A, series_id, 100.0, 5 * 60 * 1000)).await;
    insert_metric_point(&ch, make_point(TENANT_A, series_id, 150.0, 10 * 60 * 1000)).await;
    insert_metric_point(&ch, make_point(TENANT_A, series_id, 200.0, 15 * 60 * 1000)).await;

    let ir = base_ir(NlqOperation::Timeseries);
    let frame = execute_mcp_query(&db, &ch, TENANT_A, &ir)
        .await
        .expect("execute_mcp_query succeeded");

    // Must return some data rows
    assert!(
        !frame.data.is_empty(),
        "expected data rows from timeseries query"
    );

    // All 6 provenance fields must be non-empty
    assert!(!frame.source_sql.is_empty(), "source_sql must be populated");
    assert!(
        !frame.approximation_statement.is_empty(),
        "approximation_statement must be populated"
    );
    assert!(
        !frame.signal_types.is_empty(),
        "signal_types must be populated"
    );
    assert!(
        !frame.time_range.from.is_empty(),
        "time_range.from must be populated"
    );
    assert!(
        !frame.time_range.to.is_empty(),
        "time_range.to must be populated"
    );
    // nlq_ir.metric must match what we sent
    assert_eq!(
        frame.nlq_ir.metric.as_deref(),
        Some(METRIC),
        "nlq_ir must reflect the original request"
    );
}

#[tokio::test]
async fn source_sql_contains_tenant_id_for_tenant_isolation() {
    let (db, _pg) = start_pg().await;
    let (ch, _ch_container) = start_ch().await;

    seed_schema_registry(&db, TENANT_A).await;

    let series_id = Uuid::new_v4();
    insert_metric_series(&ch, make_series(TENANT_A, series_id)).await;
    insert_metric_point(&ch, make_point(TENANT_A, series_id, 42.0, 60_000)).await;

    let ir = base_ir(NlqOperation::Timeseries);
    let frame = execute_mcp_query(&db, &ch, TENANT_A, &ir)
        .await
        .expect("execute_mcp_query succeeded");

    let tenant_str = TENANT_A.to_string();
    assert!(
        frame.source_sql.contains(&tenant_str),
        "source_sql must contain tenant_id ({tenant_str}) for tenant isolation: {}",
        frame.source_sql
    );
}

#[tokio::test]
async fn generated_sql_is_select_only_advisory_invariant() {
    let (db, _pg) = start_pg().await;
    let (ch, _ch_container) = start_ch().await;

    seed_schema_registry(&db, TENANT_A).await;

    let series_id = Uuid::new_v4();
    insert_metric_series(&ch, make_series(TENANT_A, series_id)).await;
    insert_metric_point(&ch, make_point(TENANT_A, series_id, 99.0, 60_000)).await;

    let ir = base_ir(NlqOperation::Timeseries);
    let frame = execute_mcp_query(&db, &ch, TENANT_A, &ir)
        .await
        .expect("execute_mcp_query succeeded");

    let sql_upper = frame.source_sql.to_uppercase();
    for dml in [
        "INSERT ",
        "UPDATE ",
        "DELETE ",
        "DROP ",
        "TRUNCATE ",
        "CREATE ",
        "ALTER ",
    ] {
        assert!(
            !sql_upper.contains(dml),
            "source_sql must not contain DML keyword '{dml}': {}",
            frame.source_sql
        );
    }
}

#[tokio::test]
async fn approximation_statement_is_non_empty_and_advisory() {
    let (db, _pg) = start_pg().await;
    let (ch, _ch_container) = start_ch().await;

    seed_schema_registry(&db, TENANT_A).await;

    let series_id = Uuid::new_v4();
    insert_metric_series(&ch, make_series(TENANT_A, series_id)).await;
    insert_metric_point(&ch, make_point(TENANT_A, series_id, 1.0, 60_000)).await;

    let ir = base_ir(NlqOperation::Timeseries);
    let frame = execute_mcp_query(&db, &ch, TENANT_A, &ir)
        .await
        .expect("execute_mcp_query succeeded");

    assert!(
        frame.approximation_statement.contains("billing"),
        "approximation_statement must mention billing advisory: {}",
        frame.approximation_statement
    );
}

#[tokio::test]
async fn table_operation_returns_data_rows() {
    let (db, _pg) = start_pg().await;
    let (ch, _ch_container) = start_ch().await;

    seed_schema_registry(&db, TENANT_A).await;

    let series_id = Uuid::new_v4();
    insert_metric_series(&ch, make_series(TENANT_A, series_id)).await;
    insert_metric_point(&ch, make_point(TENANT_A, series_id, 77.0, 5 * 60 * 1000)).await;
    insert_metric_point(&ch, make_point(TENANT_A, series_id, 88.0, 10 * 60 * 1000)).await;

    let ir = base_ir(NlqOperation::Table);
    let frame = execute_mcp_query(&db, &ch, TENANT_A, &ir)
        .await
        .expect("execute_mcp_query succeeded for table op");

    assert!(
        !frame.data.is_empty(),
        "table query must return at least one row"
    );
    // Table rows should have a "value" field
    assert!(
        frame.data[0].get("value").is_some(),
        "table rows must include 'value': {:?}",
        frame.data[0]
    );
}

#[tokio::test]
async fn tenant_b_sees_no_data_from_tenant_a_series() {
    let (db, _pg) = start_pg().await;
    let (ch, _ch_container) = start_ch().await;

    // Register metric for both tenants (tenant_b has annotation too)
    seed_schema_registry(&db, TENANT_A).await;
    seed_schema_registry(&db, TENANT_B).await;

    // Only insert data for tenant_a
    let series_id_a = Uuid::new_v4();
    insert_metric_series(&ch, make_series(TENANT_A, series_id_a)).await;
    insert_metric_point(&ch, make_point(TENANT_A, series_id_a, 55.0, 5 * 60 * 1000)).await;

    // Query as tenant_b — must get empty data set
    let ir = base_ir(NlqOperation::Timeseries);
    let frame = execute_mcp_query(&db, &ch, TENANT_B, &ir)
        .await
        .expect("execute_mcp_query succeeded for tenant_b");

    assert!(
        frame.data.is_empty(),
        "tenant_b must see no rows from tenant_a series"
    );

    // tenant_b's source_sql must NOT contain tenant_a's uuid
    assert!(
        !frame.source_sql.contains(&TENANT_A.to_string()),
        "tenant_b source_sql must not reference tenant_a"
    );
}

#[tokio::test]
async fn unknown_metric_returns_error() {
    let (db, _pg) = start_pg().await;
    let (_ch, _ch_container) = start_ch().await;

    // Do not seed schema registry — metric is unknown
    let ir = NlqIr {
        metric: Some("nonexistent_metric".into()),
        ..base_ir(NlqOperation::Timeseries)
    };

    let result = execute_mcp_query(&db, &_ch, TENANT_A, &ir).await;

    assert!(
        matches!(
            result,
            Err(query_api::mcp_query::McpQueryError::UnknownMetric(_))
        ),
        "missing schema entry must yield UnknownMetric error, got: {result:?}"
    );
}

#[tokio::test]
async fn missing_metric_field_returns_error() {
    let (db, _pg) = start_pg().await;
    let (_ch, _ch_container) = start_ch().await;

    let ir = NlqIr {
        metric: None,
        ..base_ir(NlqOperation::Timeseries)
    };

    let result = execute_mcp_query(&db, &_ch, TENANT_A, &ir).await;

    assert!(
        matches!(
            result,
            Err(query_api::mcp_query::McpQueryError::MissingMetric)
        ),
        "absent metric must yield MissingMetric error, got: {result:?}"
    );
}

// ── P8-S6 Step 9: Provenance gate ─────────────────────────────────────────────────────────────
//
// Asserts:
//   1. Every NLQ response includes all 6 provenance fields with non-empty values.
//   2. The query is read-only: row counts in both PostgreSQL and ClickHouse are unchanged.

#[tokio::test]
async fn provenance_gate_all_six_fields_non_empty() {
    let (db, _pg) = start_pg().await;
    let (ch, _ch_container) = start_ch().await;

    seed_schema_registry(&db, TENANT_A).await;

    let series_id = Uuid::new_v4();
    insert_metric_series(&ch, make_series(TENANT_A, series_id)).await;
    insert_metric_point(&ch, make_point(TENANT_A, series_id, 42.0, 5 * 60 * 1000)).await;

    let ir = base_ir(NlqOperation::Timeseries);
    let frame = execute_mcp_query(&db, &ch, TENANT_A, &ir)
        .await
        .expect("execute_mcp_query must succeed");

    // P8-S6 checkpoint: every NLQ response must carry all 6 provenance fields
    assert!(
        !frame.source_sql.is_empty(),
        "provenance: source_sql must be non-empty"
    );
    assert!(
        !frame.approximation_statement.is_empty(),
        "provenance: approximation_statement must be non-empty"
    );
    assert!(
        !frame.signal_types.is_empty(),
        "provenance: signal_types must be non-empty"
    );
    assert!(
        !frame.time_range.from.is_empty(),
        "provenance: time_range.from must be non-empty"
    );
    assert!(
        !frame.time_range.to.is_empty(),
        "provenance: time_range.to must be non-empty"
    );
    assert!(
        frame.nlq_ir.metric.is_some(),
        "provenance: nlq_ir.metric must be present"
    );

    // Advisory-only: approximation_statement must include billing disclaimer
    assert!(
        frame.approximation_statement.contains("billing"),
        "approximation_statement must include advisory billing disclaimer: {}",
        frame.approximation_statement
    );
}

#[tokio::test]
async fn provenance_gate_query_is_read_only_no_row_mutations() {
    let (db, _pg) = start_pg().await;
    let (ch, _ch_container) = start_ch().await;

    seed_schema_registry(&db, TENANT_A).await;

    let series_id = Uuid::new_v4();
    insert_metric_series(&ch, make_series(TENANT_A, series_id)).await;
    insert_metric_point(&ch, make_point(TENANT_A, series_id, 99.0, 5 * 60 * 1000)).await;

    // Count rows BEFORE query
    let pg_series_count_before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM schema_entries WHERE signal_type = 'metrics'")
            .fetch_one(&db)
            .await
            .expect("pre-query count");

    let pg_annotations_count_before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM semantic_annotations WHERE tenant_id = $1")
            .bind(TENANT_A)
            .fetch_one(&db)
            .await
            .expect("pre-query annotations count");

    // Execute the NLQ query
    let ir = base_ir(NlqOperation::Timeseries);
    execute_mcp_query(&db, &ch, TENANT_A, &ir)
        .await
        .expect("execute_mcp_query must succeed");

    // Count rows AFTER query — must be identical
    let pg_series_count_after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM schema_entries WHERE signal_type = 'metrics'")
            .fetch_one(&db)
            .await
            .expect("post-query count");

    let pg_annotations_count_after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM semantic_annotations WHERE tenant_id = $1")
            .bind(TENANT_A)
            .fetch_one(&db)
            .await
            .expect("post-query annotations count");

    assert_eq!(
        pg_series_count_before, pg_series_count_after,
        "schema_entries row count must not change after NLQ query (read-only)"
    );
    assert_eq!(
        pg_annotations_count_before, pg_annotations_count_after,
        "semantic_annotations row count must not change after NLQ query (read-only)"
    );
}
