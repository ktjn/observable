// Testcontainers integration test for `fetch_label_keys` (Slice B).
//
// Verifies that:
//   - Native columns (service_name, environment, metric_name) are always returned first.
//   - Attribute keys extracted from metric_series JSON are appended.
//   - The combined result is deduplicated (native keys not repeated).
//
// Per ADR-025, all ClickHouse-touching functions require a Testcontainers test.

use domain::MetricSeriesRow;
use query_api::mcp_tools::fetch_label_keys;
use uuid::Uuid;

// ── ClickHouse helpers ────────────────────────────────────────────────────────

async fn insert_metric_series(ch: &clickhouse::Client, row: MetricSeriesRow) {
    let mut ins = ch
        .insert::<MetricSeriesRow>("metric_series")
        .await
        .expect("metric_series insert handle");
    ins.write(&row).await.expect("metric_series row written");
    ins.end().await.expect("metric_series insert committed");
}

fn make_series_with_attrs(tenant_id: Uuid, attrs: &str) -> MetricSeriesRow {
    MetricSeriesRow {
        tenant_id,
        metric_series_id: Uuid::new_v4(),
        metric_name: "http_request_duration_ms".into(),
        description: String::new(),
        unit: "ms".into(),
        metric_type: "gauge".into(),
        is_monotonic: None,
        aggregation_temporality: None,
        attributes: attrs.into(),
        resource_attributes: "{}".into(),
        service_name: "web-frontend".into(),
        environment: "production".into(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn fetch_label_keys_returns_native_columns() {
    let ch = test_support::clickhouse::shared_client().await;
    let tenant = Uuid::new_v4();

    // Insert a row with JSON attributes containing pod and region.
    let row = make_series_with_attrs(tenant, r#"{"pod":"web-1","region":"eu"}"#);
    insert_metric_series(&ch, row).await;

    let keys = fetch_label_keys(&ch, tenant, 20)
        .await
        .expect("fetch_label_keys must succeed");

    // Native columns must always be present.
    assert!(
        keys.contains(&"service_name".to_string()),
        "service_name must be in label_keys; got: {keys:?}"
    );
    assert!(
        keys.contains(&"environment".to_string()),
        "environment must be in label_keys; got: {keys:?}"
    );
    assert!(
        keys.contains(&"metric_name".to_string()),
        "metric_name must be in label_keys; got: {keys:?}"
    );

    // ClickHouse-discovered attribute keys must also be present.
    assert!(
        keys.contains(&"pod".to_string()),
        "pod attribute key must be in label_keys; got: {keys:?}"
    );
    assert!(
        keys.contains(&"region".to_string()),
        "region attribute key must be in label_keys; got: {keys:?}"
    );
}

#[tokio::test]
async fn fetch_label_keys_no_attributes_returns_only_native() {
    let ch = test_support::clickhouse::shared_client().await;
    let tenant = Uuid::new_v4();

    // Insert a row with empty attributes — ClickHouse query should return nothing.
    let row = make_series_with_attrs(tenant, "{}");
    insert_metric_series(&ch, row).await;

    let keys = fetch_label_keys(&ch, tenant, 20)
        .await
        .expect("fetch_label_keys must succeed");

    // Must still include native columns.
    assert!(keys.contains(&"service_name".to_string()));
    assert!(keys.contains(&"environment".to_string()));
    assert!(keys.contains(&"metric_name".to_string()));
}

#[tokio::test]
async fn fetch_label_keys_deduplicates_native_columns() {
    let ch = test_support::clickhouse::shared_client().await;
    let tenant = Uuid::new_v4();

    // Attributes that contain a key matching a native column name.
    let row = make_series_with_attrs(
        tenant,
        r#"{"service_name":"should-not-duplicate","pod":"x"}"#,
    );
    insert_metric_series(&ch, row).await;

    let keys = fetch_label_keys(&ch, tenant, 20)
        .await
        .expect("fetch_label_keys must succeed");

    // service_name should appear exactly once.
    let count = keys.iter().filter(|k| k.as_str() == "service_name").count();
    assert_eq!(
        count, 1,
        "service_name must appear exactly once; got: {keys:?}"
    );
}
