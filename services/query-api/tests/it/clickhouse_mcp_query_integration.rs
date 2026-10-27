// Testcontainers integration test for the end-to-end MCP query path (P8-S6 Step 5).
//
// Spins up a ClickHouse container, applies all migrations, inserts
// test data, and calls `execute_mcp_query` directly with a schema mock to assert:
//   - Data rows are returned from ClickHouse
//   - All 6 provenance fields are populated
//   - tenant_id appears in source_sql (tenant isolation)
//   - Advisory-only invariant: generated SQL contains no DML keywords
//   - Tenant isolation: tenant_b sees no data from tenant_a series

use clickhouse::Client as ChClient;
use domain::{NlqIr, NlqOperation, NlqSignal, NlqTimeRange};
use observable_storage_contracts::{MetricPointRow, MetricSeriesRow};
use query_api::mcp_query::execute_mcp_query;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ── Constants ─────────────────────────────────────────────────────────────────

const METRIC: &str = "latency_ms";
const TEST_TOKEN: &str = "test-internal-token";

// ── Mock helpers ──────────────────────────────────────────────────────────────

async fn start_schema_mock() -> MockServer {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(format!("/internal/schemas/metrics/schema/{METRIC}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "field_name": METRIC,
            "field_type": "Float64",
            "display_name": null,
            "metric_type": "gauge",
            "timestamp_column": "ts",
            "unit": "ms",
            "recommended_downsampling": null,
            "interpretation_rule": null,
            "not_for_billing": false,
            "schema_complete": true
        })))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/internal/schemas/metrics/schema/nonexistent_metric"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::Value::Null))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/internal/schemas/metrics/fields"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "field_name": METRIC,
                "field_type": "Float64",
                "otel_spec_version": null,
                "display_name": null,
                "business_description": null,
                "interpretation_rule": null,
                "effective_sample_rate": null,
                "metric_type": "gauge",
                "timestamp_column": "ts",
                "unit": "ms",
                "recommended_downsampling": null,
                "schema_complete": true
            }
        ])))
        .mount(&mock)
        .await;

    mock
}

// ── ClickHouse helpers ────────────────────────────────────────────────────────

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
        percentiles: None,
        catalog_field: None,
        limit: None,
        query: None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn timeseries_query_returns_data_with_all_provenance_fields() {
    let mock = start_schema_mock().await;
    let client = reqwest::Client::new();
    let ch = test_support::clickhouse::shared_client().await;

    let tenant_a = Uuid::new_v4();

    let series_id = Uuid::new_v4();
    insert_metric_series(&ch, make_series(tenant_a, series_id)).await;
    // Insert 3 data points at 5m, 10m, 15m ago
    insert_metric_point(&ch, make_point(tenant_a, series_id, 100.0, 5 * 60 * 1000)).await;
    insert_metric_point(&ch, make_point(tenant_a, series_id, 150.0, 10 * 60 * 1000)).await;
    insert_metric_point(&ch, make_point(tenant_a, series_id, 200.0, 15 * 60 * 1000)).await;

    let ir = base_ir(NlqOperation::Timeseries);
    let frame = execute_mcp_query(&client, &mock.uri(), TEST_TOKEN, &ch, tenant_a, &ir)
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
    let mock = start_schema_mock().await;
    let client = reqwest::Client::new();
    let ch = test_support::clickhouse::shared_client().await;

    let tenant_a = Uuid::new_v4();

    let series_id = Uuid::new_v4();
    insert_metric_series(&ch, make_series(tenant_a, series_id)).await;
    insert_metric_point(&ch, make_point(tenant_a, series_id, 42.0, 60_000)).await;

    let ir = base_ir(NlqOperation::Timeseries);
    let frame = execute_mcp_query(&client, &mock.uri(), TEST_TOKEN, &ch, tenant_a, &ir)
        .await
        .expect("execute_mcp_query succeeded");

    let tenant_str = tenant_a.to_string();
    assert!(
        frame.source_sql.contains(&tenant_str),
        "source_sql must contain tenant_id ({tenant_str}) for tenant isolation: {}",
        frame.source_sql
    );
}

#[tokio::test]
async fn generated_sql_is_select_only_advisory_invariant() {
    let mock = start_schema_mock().await;
    let client = reqwest::Client::new();
    let ch = test_support::clickhouse::shared_client().await;

    let tenant_a = Uuid::new_v4();

    let series_id = Uuid::new_v4();
    insert_metric_series(&ch, make_series(tenant_a, series_id)).await;
    insert_metric_point(&ch, make_point(tenant_a, series_id, 99.0, 60_000)).await;

    let ir = base_ir(NlqOperation::Timeseries);
    let frame = execute_mcp_query(&client, &mock.uri(), TEST_TOKEN, &ch, tenant_a, &ir)
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
    let mock = start_schema_mock().await;
    let client = reqwest::Client::new();
    let ch = test_support::clickhouse::shared_client().await;

    let tenant_a = Uuid::new_v4();

    let series_id = Uuid::new_v4();
    insert_metric_series(&ch, make_series(tenant_a, series_id)).await;
    insert_metric_point(&ch, make_point(tenant_a, series_id, 1.0, 60_000)).await;

    let ir = base_ir(NlqOperation::Timeseries);
    let frame = execute_mcp_query(&client, &mock.uri(), TEST_TOKEN, &ch, tenant_a, &ir)
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
    let mock = start_schema_mock().await;
    let client = reqwest::Client::new();
    let ch = test_support::clickhouse::shared_client().await;

    let tenant_a = Uuid::new_v4();

    let series_id = Uuid::new_v4();
    insert_metric_series(&ch, make_series(tenant_a, series_id)).await;
    insert_metric_point(&ch, make_point(tenant_a, series_id, 77.0, 5 * 60 * 1000)).await;
    insert_metric_point(&ch, make_point(tenant_a, series_id, 88.0, 10 * 60 * 1000)).await;

    let ir = base_ir(NlqOperation::Table);
    let frame = execute_mcp_query(&client, &mock.uri(), TEST_TOKEN, &ch, tenant_a, &ir)
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
    let mock = start_schema_mock().await;
    let client = reqwest::Client::new();
    let ch = test_support::clickhouse::shared_client().await;

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    // Only insert data for tenant_a
    let series_id_a = Uuid::new_v4();
    insert_metric_series(&ch, make_series(tenant_a, series_id_a)).await;
    insert_metric_point(&ch, make_point(tenant_a, series_id_a, 55.0, 5 * 60 * 1000)).await;

    // Query as tenant_b — must get empty data set
    let ir = base_ir(NlqOperation::Timeseries);
    let frame = execute_mcp_query(&client, &mock.uri(), TEST_TOKEN, &ch, tenant_b, &ir)
        .await
        .expect("execute_mcp_query succeeded for tenant_b");

    assert!(
        frame.data.is_empty(),
        "tenant_b must see no rows from tenant_a series"
    );

    // tenant_b's source_sql must NOT contain tenant_a's uuid
    assert!(
        !frame.source_sql.contains(&tenant_a.to_string()),
        "tenant_b source_sql must not reference tenant_a"
    );
}

#[tokio::test]
async fn unknown_metric_returns_error() {
    let mock = start_schema_mock().await;
    let client = reqwest::Client::new();
    let ch = test_support::clickhouse::shared_client().await;

    let tenant_a = Uuid::new_v4();

    // Query nonexistent_metric — mock returns 404
    let ir = NlqIr {
        metric: Some("nonexistent_metric".into()),
        ..base_ir(NlqOperation::Timeseries)
    };

    let result = execute_mcp_query(&client, &mock.uri(), TEST_TOKEN, &ch, tenant_a, &ir).await;

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
    let mock = start_schema_mock().await;
    let client = reqwest::Client::new();
    let ch = test_support::clickhouse::shared_client().await;

    let tenant_a = Uuid::new_v4();

    let ir = NlqIr {
        metric: None,
        ..base_ir(NlqOperation::Timeseries)
    };

    let result = execute_mcp_query(&client, &mock.uri(), TEST_TOKEN, &ch, tenant_a, &ir).await;

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
//   2. The query is read-only: row counts in ClickHouse are unchanged.

#[tokio::test]
async fn provenance_gate_all_six_fields_non_empty() {
    let mock = start_schema_mock().await;
    let client = reqwest::Client::new();
    let ch = test_support::clickhouse::shared_client().await;

    let tenant_a = Uuid::new_v4();

    let series_id = Uuid::new_v4();
    insert_metric_series(&ch, make_series(tenant_a, series_id)).await;
    insert_metric_point(&ch, make_point(tenant_a, series_id, 42.0, 5 * 60 * 1000)).await;

    let ir = base_ir(NlqOperation::Timeseries);
    let frame = execute_mcp_query(&client, &mock.uri(), TEST_TOKEN, &ch, tenant_a, &ir)
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
    let mock = start_schema_mock().await;
    let client = reqwest::Client::new();
    let ch = test_support::clickhouse::shared_client().await;

    let tenant_a = Uuid::new_v4();

    let series_id = Uuid::new_v4();
    insert_metric_series(&ch, make_series(tenant_a, series_id)).await;
    insert_metric_point(&ch, make_point(tenant_a, series_id, 99.0, 5 * 60 * 1000)).await;

    // Count rows BEFORE query
    let ch_series_count_before: u64 = ch
        .query("SELECT count() FROM metric_series WHERE tenant_id = ?")
        .bind(tenant_a)
        .fetch_one()
        .await
        .expect("pre-query count");

    let ch_points_count_before: u64 = ch
        .query("SELECT count() FROM metric_points WHERE tenant_id = ?")
        .bind(tenant_a)
        .fetch_one()
        .await
        .expect("pre-query points count");

    // Execute the NLQ query
    let ir = base_ir(NlqOperation::Timeseries);
    execute_mcp_query(&client, &mock.uri(), TEST_TOKEN, &ch, tenant_a, &ir)
        .await
        .expect("execute_mcp_query must succeed");

    // Count rows AFTER query — must be identical
    let ch_series_count_after: u64 = ch
        .query("SELECT count() FROM metric_series WHERE tenant_id = ?")
        .bind(tenant_a)
        .fetch_one()
        .await
        .expect("post-query count");

    let ch_points_count_after: u64 = ch
        .query("SELECT count() FROM metric_points WHERE tenant_id = ?")
        .bind(tenant_a)
        .fetch_one()
        .await
        .expect("post-query points count");

    assert_eq!(
        ch_series_count_before, ch_series_count_after,
        "metric_series row count must not change after NLQ query (read-only)"
    );
    assert_eq!(
        ch_points_count_before, ch_points_count_after,
        "metric_points row count must not change after NLQ query (read-only)"
    );
}
