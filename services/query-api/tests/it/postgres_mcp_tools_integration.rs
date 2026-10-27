use query_api::mcp_tools::{
    ResolveLabelResult, get_metric_schema, list_signal_fields, resolve_label_to_column,
};
use uuid::Uuid;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TENANT_A: Uuid = Uuid::from_u128(0xAAAA_0000_0000_0000_0000_0000_0000_0001);
const TEST_TOKEN: &str = "test-internal-token";

#[tokio::test]
async fn get_metric_schema_returns_none_for_unknown_metric_null() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/internal/schemas/metrics/schema/nonexistent_metric"))
        .and(query_param("tenant_id", TENANT_A.to_string()))
        .and(header("x-internal-token", TEST_TOKEN))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::Value::Null))
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let result = get_metric_schema(
        &client,
        &mock.uri(),
        TEST_TOKEN,
        TENANT_A,
        "nonexistent_metric",
    )
    .await
    .unwrap();

    assert!(
        result.is_none(),
        "null body from admin-service must return None"
    );
}

#[tokio::test]
async fn get_metric_schema_returns_parsed_schema_on_200() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/internal/schemas/metrics/schema/cpu_usage"))
        .and(query_param("tenant_id", TENANT_A.to_string()))
        .and(header("x-internal-token", TEST_TOKEN))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "field_name": "cpu_usage",
            "field_type": "Float64",
            "display_name": "CPU Usage Percent",
            "metric_type": "gauge",
            "timestamp_column": "time_unix_nano",
            "unit": "%",
            "recommended_downsampling": "1m",
            "interpretation_rule": "higher_is_worse",
            "not_for_billing": true,
            "schema_complete": true
        })))
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let result = get_metric_schema(&client, &mock.uri(), TEST_TOKEN, TENANT_A, "cpu_usage")
        .await
        .unwrap();

    let schema = result.expect("cpu_usage schema must be present");
    assert_eq!(schema.field_name, "cpu_usage");
    assert_eq!(schema.display_name.as_deref(), Some("CPU Usage Percent"));
    assert_eq!(schema.metric_type.as_deref(), Some("gauge"));
    assert_eq!(schema.unit.as_deref(), Some("%"));
    assert!(schema.schema_complete);
}

#[tokio::test]
async fn get_metric_schema_returns_err_on_500() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/internal/schemas/metrics/schema/error_metric"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let result =
        get_metric_schema(&client, &mock.uri(), TEST_TOKEN, TENANT_A, "error_metric").await;

    assert!(result.is_err(), "500 from admin-service must return Err");
}

#[tokio::test]
async fn list_signal_fields_returns_parsed_fields_on_200() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/internal/schemas/metrics/fields"))
        .and(query_param("tenant_id", TENANT_A.to_string()))
        .and(header("x-internal-token", TEST_TOKEN))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "field_name": "latency_ms",
                "field_type": "Float64",
                "otel_spec_version": null,
                "display_name": "Latency (ms)",
                "business_description": "Request latency",
                "interpretation_rule": null,
                "effective_sample_rate": null,
                "metric_type": "gauge",
                "timestamp_column": "time_unix_nano",
                "unit": "ms",
                "recommended_downsampling": null,
                "schema_complete": true
            }
        ])))
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let fields = list_signal_fields(&client, &mock.uri(), TEST_TOKEN, TENANT_A, "metrics")
        .await
        .unwrap();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].field_name, "latency_ms");
    assert_eq!(fields[0].display_name.as_deref(), Some("Latency (ms)"));
    assert_eq!(fields[0].unit.as_deref(), Some("ms"));
}

#[tokio::test]
async fn list_signal_fields_returns_err_on_500() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/internal/schemas/metrics/fields"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let result = list_signal_fields(&client, &mock.uri(), TEST_TOKEN, TENANT_A, "metrics").await;

    assert!(result.is_err(), "500 from admin-service must return Err");
}

#[tokio::test]
async fn resolve_label_to_column_returns_resolved_column_on_200() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/internal/schemas/metrics/resolve-label"))
        .and(query_param("tenant_id", TENANT_A.to_string()))
        .and(query_param("label", "CPU Usage"))
        .and(header("x-internal-token", TEST_TOKEN))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "field_name": "cpu_usage",
            "ambiguous": false,
            "candidates": []
        })))
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let result = resolve_label_to_column(
        &client,
        &mock.uri(),
        TEST_TOKEN,
        TENANT_A,
        "metrics",
        "CPU Usage",
    )
    .await
    .unwrap();

    match result {
        ResolveLabelResult::Found(col) => assert_eq!(col, "cpu_usage"),
        other => panic!("expected Found, got {other:?}"),
    }
}

#[tokio::test]
async fn resolve_label_to_column_returns_not_found_when_unmatched() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/internal/schemas/metrics/resolve-label"))
        .and(query_param("tenant_id", TENANT_A.to_string()))
        .and(query_param("label", "Unknown Label"))
        .and(header("x-internal-token", TEST_TOKEN))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "field_name": null,
            "ambiguous": false,
            "candidates": []
        })))
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let result = resolve_label_to_column(
        &client,
        &mock.uri(),
        TEST_TOKEN,
        TENANT_A,
        "metrics",
        "Unknown Label",
    )
    .await
    .unwrap();

    assert!(
        matches!(result, ResolveLabelResult::NotFound),
        "unmatched label must map to ResolveLabelResult::NotFound"
    );
}

#[tokio::test]
async fn resolve_label_to_column_returns_err_on_500() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/internal/schemas/metrics/resolve-label"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let result = resolve_label_to_column(
        &client,
        &mock.uri(),
        TEST_TOKEN,
        TENANT_A,
        "metrics",
        "Some Label",
    )
    .await;

    assert!(result.is_err(), "500 must return Err");
}
