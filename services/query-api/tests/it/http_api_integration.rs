use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    middleware as axum_middleware,
    routing::{get, post, put},
};
use clickhouse::Client as ChClient;
use domain::{LogRow, MetricPointRow, MetricSeriesRow, SpanRow};
use http_body_util::BodyExt;
use query_api::{
    alerts, dashboards, discovery, incidents, llm_adapter, logs, metrics,
    middleware::auth::TenantContext, middleware::auth::require_tenant, observability,
    planner::QueryPlanner, reliability, slos, traces,
};
use serde_json::Value;
use sqlx::postgres::PgPool;
use std::{path::Path, sync::Arc};
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::clickhouse::ClickHouse;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

// ── Dev credentials (must match seed data in migrations) ────────────────────
// Migration 017 moves dev-key to the dev-tenant at ...0002.
// Tenant ...0001 is the 'observable' self-ingestion tenant.

const DEV_TENANT_ID: &str = "00000000-0000-0000-0000-000000000002";
const DEV_API_KEY: &str = "dev-api-key-0000";

// ── Container helpers ────────────────────────────────────────────────────────

async fn start_clickhouse() -> (ChClient, testcontainers::ContainerAsync<ClickHouse>) {
    let container = ClickHouse::default()
        .with_tag("25.3")
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
                root.query(stmt).execute().await.expect("migration applied");
            }
        }
    }

    ChClient::default()
        .with_url(base_url)
        .with_user(user)
        .with_password(password)
        .with_database("observable")
}

// ── App builder ──────────────────────────────────────────────────────────────

/// Starts a wiremock server that stubs auth-service's `/internal/validate`
/// endpoint to accept `DEV_API_KEY` and resolve it to `DEV_TENANT_ID` with the
/// `tenant_admin` role. Most tests in this file authenticate with the dev API
/// key via `require_tenant`, which (since the audit-gap fix) now calls out to
/// auth-service over HTTP rather than querying Postgres directly — this mock
/// stands in for that call. The returned `MockServer` must be kept alive for
/// as long as the app built against its URI is used.
async fn start_dev_auth_mock() -> MockServer {
    let mock_server = MockServer::start().await;
    let tenant_id = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    Mock::given(method("POST"))
        .and(path("/internal/validate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "tenant_id": tenant_id,
            "role": "tenant_admin",
            "environment": "prod",
        })))
        .mount(&mock_server)
        .await;

    mock_server
}

/// Most callers in this file don't hold on to the returned `MockServer`, so
/// leak it for the (short) lifetime of the test process rather than
/// threading an extra return value through every call site.
async fn build_app_with_pg(ch: ChClient, db: PgPool) -> Router {
    let mock_server = Box::leak(Box::new(start_dev_auth_mock().await));
    build_app_with_pg_at(ch, db, mock_server.uri())
}

fn build_app_with_pg_at(ch: ChClient, db: PgPool, auth_service_url: String) -> Router {
    let state = traces::AppState {
        ch,
        db: db.clone(),
        planner: Arc::new(QueryPlanner),
        llm: None,
        auth_service_url,
        metrics: Arc::new(observability::QueryApiMetrics::new()),
    };
    let auth_service_url = Arc::new(state.auth_service_url.clone());
    Router::new()
        .route("/v1/traces/histogram", get(traces::trace_histogram))
        .route("/v1/logs/histogram", get(logs::log_histogram))
        .route("/v1/metrics", get(metrics::list_metrics))
        .route("/v1/metrics/points", get(metrics::get_metric_group_points))
        .route("/v1/nlq", post(llm_adapter::handle_nlq_query))
        .route("/v1/dashboards/{id}", get(dashboards::handle_get_dashboard))
        .route(
            "/v1/dashboards/{id}",
            put(dashboards::handle_update_dashboard),
        )
        .route("/v1/alerts/rules", get(alerts::handle_list_rules))
        .route("/v1/alerts/rules/{rule_id}", get(alerts::handle_get_rule))
        .route("/v1/slos", get(slos::handle_list_slos))
        .route("/v1/slos", post(slos::handle_create_slo))
        .route(
            "/v1/services/summary",
            get(discovery::list_service_summaries),
        )
        .route("/v1/incidents", get(incidents::handle_list_incidents))
        .route(
            "/v1/incidents/{incident_id}",
            get(incidents::handle_get_incident),
        )
        .route(
            "/v1/services/{service_name}/reliability-report",
            get(reliability::handle_get_service_reliability_report),
        )
        .layer(axum_middleware::from_fn(require_tenant))
        .layer(axum::Extension(db))
        .layer(axum::Extension(auth_service_url))
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/readyz", get(observability::readyz))
        .route("/metrics", get(observability::metrics))
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            observability::record_http_metrics,
        ))
        .with_state(state)
}

/// App used for tests where auth rejection happens before the handler is
/// reached (i.e. missing Authorization header → immediate 401). The lazy
/// Postgres pool is never actually queried in these cases.
fn fake_app_no_db(auth_url: Option<String>) -> Router {
    let db =
        PgPool::connect_lazy("postgres://user:pass@127.0.0.1:5432/db").expect("valid postgres url");
    let ch = ChClient::default().with_url("http://127.0.0.1:19999");
    let auth_service_url = auth_url.unwrap_or_else(|| "http://auth-service:4319".into());
    let state = traces::AppState {
        ch,
        db: db.clone(),
        planner: Arc::new(QueryPlanner),
        llm: None,
        auth_service_url: auth_service_url.clone(),
        metrics: Arc::new(observability::QueryApiMetrics::new()),
    };
    let auth_service_url_ext = Arc::new(auth_service_url);
    Router::new()
        .route("/v1/traces/histogram", get(traces::trace_histogram))
        .route("/v1/logs/histogram", get(logs::log_histogram))
        .route("/v1/metrics", get(metrics::list_metrics))
        .route("/v1/metrics/points", get(metrics::get_metric_group_points))
        .route("/v1/alerts/rules", get(alerts::handle_list_rules))
        .route("/v1/slos", get(slos::handle_list_slos))
        .route("/v1/incidents", get(incidents::handle_list_incidents))
        .route(
            "/v1/incidents/{incident_id}",
            get(incidents::handle_get_incident),
        )
        .route(
            "/v1/services/{service_name}/reliability-report",
            get(reliability::handle_get_service_reliability_report),
        )
        .layer(axum_middleware::from_fn(require_tenant))
        .layer(axum::Extension(db))
        .layer(axum::Extension(auth_service_url_ext))
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/readyz", get(observability::readyz))
        .route("/metrics", get(observability::metrics))
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            observability::record_http_metrics,
        ))
        .with_state(state)
}

fn fake_nlq_app_no_db() -> Router {
    let db =
        PgPool::connect_lazy("postgres://user:pass@127.0.0.1:5432/db").expect("valid postgres url");
    let ch = ChClient::default().with_url("http://127.0.0.1:19999");
    let state = traces::AppState {
        ch,
        db,
        planner: Arc::new(QueryPlanner),
        llm: None,
        auth_service_url: "http://auth-service:4319".into(),
        metrics: Arc::new(observability::QueryApiMetrics::new()),
    };
    let tenant_id = Uuid::parse_str(DEV_TENANT_ID).unwrap();
    Router::new()
        .route("/v1/nlq", post(llm_adapter::handle_nlq_query))
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/readyz", get(observability::readyz))
        .route("/metrics", get(observability::metrics))
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id: None,
            role: "admin".into(),
        }))
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            observability::record_http_metrics,
        ))
        .with_state(state)
}

fn dev_request(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", format!("Bearer {DEV_API_KEY}"))
        .header("X-Tenant-ID", DEV_TENANT_ID)
        .body(Body::empty())
        .unwrap()
}

async fn response_body_json(body: axum::body::Body) -> Value {
    let bytes = body.collect().await.expect("body collected").to_bytes();
    serde_json::from_slice(&bytes).expect("valid JSON")
}

// ── Insertion helpers ────────────────────────────────────────────────────────

async fn insert_span(ch: &ChClient, row: SpanRow) {
    let mut ins = ch.insert::<SpanRow>("spans").await.expect("insert handle");
    ins.write(&row).await.expect("row written");
    ins.end().await.expect("insert committed");
}

async fn insert_log(ch: &ChClient, row: LogRow) {
    let mut ins = ch.insert::<LogRow>("logs").await.expect("insert handle");
    ins.write(&row).await.expect("log written");
    ins.end().await.expect("insert committed");
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

fn make_span(tenant_id: Uuid, trace_id: &str, span_id: &str, start_ns: u64) -> SpanRow {
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
        start_time_unix_nano: start_ns,
        end_time_unix_nano: start_ns + 1_000_000,
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

fn make_log(tenant_id: Uuid, service: &str, ts_ns: u64) -> LogRow {
    LogRow {
        tenant_id,
        log_id: Uuid::new_v4(),
        timestamp_unix_nano: ts_ns,
        observed_timestamp_unix_nano: ts_ns,
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

fn make_metric_series(
    tenant_id: Uuid,
    series_id: Uuid,
    metric_name: &str,
    route: &str,
) -> MetricSeriesRow {
    MetricSeriesRow {
        tenant_id,
        metric_series_id: series_id,
        metric_name: metric_name.into(),
        description: String::new(),
        unit: "1".into(),
        metric_type: "sum".into(),
        is_monotonic: Some(1),
        aggregation_temporality: Some("delta".into()),
        attributes: format!(r#"{{"route":"{route}"}}"#),
        resource_attributes: "{}".into(),
        service_name: "checkout".into(),
        environment: "prod".into(),
    }
}

fn make_metric_point(
    tenant_id: Uuid,
    series_id: Uuid,
    metric_name: &str,
    ts_ns: u64,
    value: i64,
) -> MetricPointRow {
    MetricPointRow {
        tenant_id,
        metric_series_id: series_id,
        metric_name: metric_name.into(),
        service_name: "checkout".into(),
        time_unix_nano: ts_ns,
        start_time_unix_nano: Some(ts_ns - 1_000_000_000),
        value_double: None,
        value_int: Some(value),
        histogram_count: None,
        histogram_sum: None,
        histogram_bucket_counts: Vec::new(),
        histogram_explicit_bounds: Vec::new(),
    }
}

// ── New credential-bound auth tests ──────────────────────────────────────────

/// Missing Authorization header → 401, before any DB query.
#[tokio::test]
async fn query_api_rejects_missing_authorization_header() {
    let app = fake_app_no_db(None);
    let req = Request::builder()
        .method("GET")
        .uri("/v1/traces/histogram?buckets=10")
        .header("X-Tenant-ID", DEV_TENANT_ID)
        // No Authorization header
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Valid token for tenant 0001 but X-Tenant-ID is a different UUID → 403.
#[tokio::test]
async fn query_api_rejects_tenant_header_not_owned_by_token() {
    let db = test_support::postgres::shared_pool().await;
    let ch = ChClient::default().with_url("http://127.0.0.1:19999");
    let app = build_app_with_pg(ch, db).await;

    let other_tenant = Uuid::new_v4();
    let req = Request::builder()
        .method("GET")
        .uri("/v1/traces/histogram?buckets=10")
        .header("Authorization", format!("Bearer {DEV_API_KEY}"))
        .header("X-Tenant-ID", other_tenant.to_string())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

/// Matching token + tenant header → request passes auth (handler may return
/// any non-401/403 status).
#[tokio::test]
async fn query_api_accepts_matching_token_and_tenant_header() {
    let db = test_support::postgres::shared_pool().await;
    let (ch, _ch_container) = start_clickhouse().await;
    let app = build_app_with_pg(ch, db).await;

    let resp = app
        .oneshot(dev_request(
            "GET",
            "/v1/traces/histogram?from=1777819009493000000&to=1777822609493000000&buckets=10",
        ))
        .await
        .unwrap();

    let status = resp.status();
    assert!(
        status != StatusCode::UNAUTHORIZED && status != StatusCode::FORBIDDEN,
        "expected auth to pass, got {status}"
    );
}

// ── Legacy auth tests (early-rejection paths, no DB needed) ──────────────────

#[tokio::test]
async fn missing_tenant_id_header_returns_401() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&mock_server)
        .await;

    let app = fake_app_no_db(Some(mock_server.uri()));
    // Has Authorization but no X-Tenant-ID
    let req = Request::builder()
        .method("GET")
        .uri("/v1/traces/histogram?buckets=10")
        .header("Authorization", "Bearer some-token")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn invalid_tenant_id_header_returns_400() {
    let app = fake_app_no_db(None);
    let req = Request::builder()
        .method("GET")
        .uri("/v1/traces/histogram?buckets=10")
        .header("Authorization", "Bearer some-token")
        .header("X-Tenant-ID", "not-a-uuid")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ── Self-observability probes ───────────────────────────────────────────────

#[tokio::test]
async fn query_api_readyz_returns_200_when_dependencies_are_up() {
    let (ch, _ch_container) = start_clickhouse().await;
    let db = test_support::postgres::shared_pool().await;
    let app = build_app_with_pg(ch, db).await;

    let req = Request::builder()
        .method("GET")
        .uri("/readyz")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn query_api_readyz_returns_503_when_dependencies_are_unavailable() {
    let app = fake_app_no_db(None);

    let req = Request::builder()
        .method("GET")
        .uri("/readyz")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn query_api_metrics_endpoint_exposes_prometheus_text() {
    let app = fake_app_no_db(None);
    let metrics_app = app.clone();

    let request = Request::builder()
        .method("GET")
        .uri("/v1/traces/histogram?buckets=10")
        .body(Body::empty())
        .expect("request body");
    let resp = app.oneshot(request).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let metrics_resp = metrics_app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(metrics_resp.status(), StatusCode::OK);
    let content_type = metrics_resp
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .expect("content type header present")
        .to_str()
        .expect("content type is utf-8");
    assert!(
        content_type.starts_with("text/plain"),
        "expected Prometheus text format, got {content_type}"
    );

    let bytes = metrics_resp
        .into_body()
        .collect()
        .await
        .expect("metrics body collected")
        .to_bytes();
    let body = String::from_utf8(bytes.to_vec()).expect("metrics body is utf-8");
    assert!(
        body.contains("query_api_http_requests_total"),
        "expected HTTP request counter in Prometheus payload"
    );
    assert!(
        body.contains("query_api_http_request_duration_seconds"),
        "expected HTTP duration histogram in Prometheus payload"
    );
    assert!(
        body.contains("method=\"GET\""),
        "expected method label in Prometheus payload"
    );
    assert!(
        body.contains("status=\"401\""),
        "expected auth failure status label in Prometheus payload"
    );
}

// ── Histogram query-string parsing (regression for nanosecond timestamps) ────

#[tokio::test]
async fn trace_histogram_accepts_nanosecond_u64_timestamps() {
    let (ch, _container) = start_clickhouse().await;
    let db = test_support::postgres::shared_pool().await;
    let app = build_app_with_pg(ch, db).await;

    let resp = app
        .oneshot(dev_request(
            "GET",
            "/v1/traces/histogram?from=1777819009493000000&to=1777822609493000000&buckets=60",
        ))
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "nanosecond u64 timestamps must deserialize without error"
    );
    let json = response_body_json(resp.into_body()).await;
    assert!(json["buckets"].is_array());
}

#[tokio::test]
async fn log_histogram_accepts_nanosecond_u64_timestamps() {
    let (ch, _container) = start_clickhouse().await;
    let db = test_support::postgres::shared_pool().await;
    let app = build_app_with_pg(ch, db).await;

    let resp = app
        .oneshot(dev_request(
            "GET",
            "/v1/logs/histogram?from=1777819009493000000&to=1777822609493000000&buckets=60",
        ))
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "nanosecond u64 timestamps must deserialize without error"
    );
    let json = response_body_json(resp.into_body()).await;
    assert!(json["buckets"].is_array());
}

// ── Histogram correctness tests ───────────────────────────────────────────────

#[tokio::test]
async fn trace_histogram_counts_inserted_spans() {
    let (ch, _container) = start_clickhouse().await;
    let db = test_support::postgres::shared_pool().await;
    let dev_tenant: Uuid = DEV_TENANT_ID.parse().unwrap();
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    let base = now_ns - 3_000_000_000;
    insert_span(&ch, make_span(dev_tenant, "trace-1", "span-1", base)).await;
    insert_span(
        &ch,
        make_span(dev_tenant, "trace-2", "span-2", base + 1_000_000_000),
    )
    .await;
    insert_span(
        &ch,
        make_span(dev_tenant, "trace-3", "span-3", base + 2_000_000_000),
    )
    .await;

    let app = build_app_with_pg(ch, db).await;
    let from = base - 1;
    let to = base + 3_000_000_001;
    let uri = format!("/v1/traces/histogram?from={from}&to={to}&buckets=30");

    let resp = app.oneshot(dev_request("GET", &uri)).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let json = response_body_json(resp.into_body()).await;
    let buckets = json["buckets"].as_array().unwrap();
    let total: u64 = buckets
        .iter()
        .map(|b| b["count"].as_u64().unwrap_or(0))
        .sum();
    assert_eq!(total, 3, "all 3 inserted spans must be counted");
}

#[tokio::test]
async fn log_histogram_counts_inserted_logs() {
    let (ch, _container) = start_clickhouse().await;
    let db = test_support::postgres::shared_pool().await;
    let dev_tenant: Uuid = DEV_TENANT_ID.parse().unwrap();
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    let base = now_ns - 3_000_000_000;
    insert_log(&ch, make_log(dev_tenant, "svc", base)).await;
    insert_log(&ch, make_log(dev_tenant, "svc", base + 1_000_000_000)).await;
    insert_log(&ch, make_log(dev_tenant, "svc", base + 2_000_000_000)).await;

    let app = build_app_with_pg(ch, db).await;
    let from = base - 1;
    let to = base + 3_000_000_001;
    let uri = format!("/v1/logs/histogram?from={from}&to={to}&buckets=30");

    let resp = app.oneshot(dev_request("GET", &uri)).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let json = response_body_json(resp.into_body()).await;
    let buckets = json["buckets"].as_array().unwrap();
    let total: u64 = buckets
        .iter()
        .flat_map(|b| b["counts"].as_object())
        .flat_map(|m| m.values())
        .filter_map(|v| v.as_u64())
        .sum();
    assert_eq!(total, 3, "all 3 inserted logs must be counted");
}

#[tokio::test]
async fn trace_histogram_tenant_isolation() {
    let (ch, _container) = start_clickhouse().await;
    let db = test_support::postgres::shared_pool().await;
    let dev_tenant: Uuid = DEV_TENANT_ID.parse().unwrap();
    // tenant_b is a different UUID; we insert its spans but query as dev_tenant
    let tenant_b = Uuid::new_v4();
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    let base = now_ns - 2_000_000_000;
    insert_span(&ch, make_span(dev_tenant, "trace-a", "span-a", base)).await;
    insert_span(
        &ch,
        make_span(tenant_b, "trace-b", "span-b", base + 500_000_000),
    )
    .await;

    let app = build_app_with_pg(ch, db).await;
    let from = base - 1;
    let to = base + 2_000_000_001;
    let uri = format!("/v1/traces/histogram?from={from}&to={to}&buckets=30");

    let resp = app.oneshot(dev_request("GET", &uri)).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let json = response_body_json(resp.into_body()).await;
    let total: u64 = json["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["count"].as_u64().unwrap_or(0))
        .sum();
    assert_eq!(total, 1, "dev_tenant must see only their own span");
}

#[tokio::test]
async fn log_histogram_service_filter() {
    let (ch, _container) = start_clickhouse().await;
    let db = test_support::postgres::shared_pool().await;
    let dev_tenant: Uuid = DEV_TENANT_ID.parse().unwrap();
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    let base = now_ns - 2_000_000_000;
    insert_log(&ch, make_log(dev_tenant, "svc-a", base)).await;
    insert_log(&ch, make_log(dev_tenant, "svc-b", base + 500_000_000)).await;

    let app = build_app_with_pg(ch, db).await;
    let from = base - 1;
    let to = base + 2_000_000_001;
    let uri = format!("/v1/logs/histogram?service=svc-a&from={from}&to={to}&buckets=30");

    let resp = app.oneshot(dev_request("GET", &uri)).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let json = response_body_json(resp.into_body()).await;
    let total: u64 = json["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|b| b["counts"].as_object())
        .flat_map(|m| m.values())
        .filter_map(|v| v.as_u64())
        .sum();
    assert_eq!(total, 1, "service filter must exclude svc-b");
}

// ── Metric catalog grouping ─────────────────────────────────────────────────

#[tokio::test]
async fn metric_list_groups_label_specific_series_by_metric_identity() {
    let (ch, _container) = start_clickhouse().await;
    let db = test_support::postgres::shared_pool().await;
    let dev_tenant: Uuid = DEV_TENANT_ID.parse().unwrap();
    insert_metric_series(
        &ch,
        make_metric_series(dev_tenant, Uuid::new_v4(), "span.calls_total", "/checkout"),
    )
    .await;
    insert_metric_series(
        &ch,
        make_metric_series(dev_tenant, Uuid::new_v4(), "span.calls_total", "/cart"),
    )
    .await;

    let app = build_app_with_pg(ch, db).await;
    let resp = app
        .oneshot(dev_request("GET", "/v1/metrics?service=checkout"))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let json = response_body_json(resp.into_body()).await;
    let metrics = json["metrics"].as_array().expect("metrics array");
    assert_eq!(metrics.len(), 1, "one grouped metric should be returned");
    assert_eq!(metrics[0]["metric_name"], "span.calls_total");
    assert_eq!(metrics[0]["series_count"], 2);
}

#[tokio::test]
async fn metric_group_points_sum_label_specific_series_at_same_timestamp() {
    let (ch, _container) = start_clickhouse().await;
    let db = test_support::postgres::shared_pool().await;
    let dev_tenant: Uuid = DEV_TENANT_ID.parse().unwrap();
    let first_series = Uuid::new_v4();
    let second_series = Uuid::new_v4();
    // Use current time so the points are always within the 14-day TTL window.
    let ts_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    insert_metric_series(
        &ch,
        make_metric_series(dev_tenant, first_series, "span.calls_total", "/checkout"),
    )
    .await;
    insert_metric_series(
        &ch,
        make_metric_series(dev_tenant, second_series, "span.calls_total", "/cart"),
    )
    .await;
    insert_metric_point(
        &ch,
        make_metric_point(dev_tenant, first_series, "span.calls_total", ts_ns, 2),
    )
    .await;
    insert_metric_point(
        &ch,
        make_metric_point(dev_tenant, second_series, "span.calls_total", ts_ns, 3),
    )
    .await;

    let app = build_app_with_pg(ch, db).await;
    let uri = "/v1/metrics/points?metric_name=span.calls_total&service=checkout&environment=prod&metric_type=sum&unit=1";
    let resp = app.oneshot(dev_request("GET", uri)).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let json = response_body_json(resp.into_body()).await;
    let points = json["points"].as_array().expect("points array");
    assert_eq!(points.len(), 1, "same timestamp should be aggregated");
    assert_eq!(points[0]["metric_name"], "span.calls_total");
    assert_eq!(points[0]["value_double"], 5.0);
}

// ── Alert lifecycle API ─────────────────────────────────────────────────────

#[tokio::test]
async fn list_alert_rules_http_returns_lifecycle_state() {
    let (ch, _ch_container) = start_clickhouse().await;
    let pg = test_support::postgres::shared_pool().await;
    let app = build_app_with_pg(ch, pg.clone()).await;
    let tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();
    let rule_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO alert_rules \
         (rule_id, tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, $2, 'HTTP lifecycle rule', 'threshold', 'warning', $3)",
    )
    .bind(rule_id)
    .bind(tenant)
    .bind(serde_json::json!({
        "metric_name": "http_lifecycle_metric",
        "operator": "gt",
        "threshold": 0.05,
    }))
    .execute(&pg)
    .await
    .expect("alert rule inserted");
    sqlx::query(
        "INSERT INTO alert_firings (rule_id, tenant_id, state, value) \
         VALUES ($1, $2, 'pending', 0.10)",
    )
    .bind(rule_id)
    .bind(tenant)
    .execute(&pg)
    .await
    .expect("alert firing inserted");

    let response = app
        .oneshot(dev_request("GET", "/v1/alerts/rules"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    let item = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["rule_id"] == rule_id.to_string())
        .expect("inserted rule appears in HTTP response");
    assert_eq!(item["state"], "pending");
    assert_eq!(item["firing"], false);
}

// ── SLO API ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn post_slo_creates_tenant_scoped_definition() {
    let (ch, _ch_container) = start_clickhouse().await;
    let pg = test_support::postgres::shared_pool().await;
    let app = build_app_with_pg(ch, pg).await;

    let body = serde_json::json!({
        "service_name": "payments",
        "environment": "prod",
        "target": 0.999,
        "window_days": 30,
        "burn_rate_fast_threshold": 14.4,
        "burn_rate_slow_threshold": 1.0,
        "description": "Payments availability SLO"
    });
    let req = Request::builder()
        .method("POST")
        .uri("/v1/slos")
        .header("Authorization", format!("Bearer {DEV_API_KEY}"))
        .header("X-Tenant-ID", DEV_TENANT_ID)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(body["service_name"], "payments");
    assert_eq!(body["environment"], "prod");
    assert_eq!(body["sli_type"], "availability");
    assert_eq!(body["target"], 0.999);
    assert_eq!(body["firing"], false);
    assert!(body["last_fired_at"].is_null());
}

#[tokio::test]
async fn post_slo_rejects_invalid_target() {
    let (ch, _ch_container) = start_clickhouse().await;
    let pg = test_support::postgres::shared_pool().await;
    let app = build_app_with_pg(ch, pg).await;

    let body = serde_json::json!({
        "service_name": "payments",
        "environment": "prod",
        "target": 1.0,
        "window_days": 30,
        "burn_rate_fast_threshold": 14.4,
        "burn_rate_slow_threshold": 1.0
    });
    let req = Request::builder()
        .method("POST")
        .uri("/v1/slos")
        .header("Authorization", format!("Bearer {DEV_API_KEY}"))
        .header("X-Tenant-ID", DEV_TENANT_ID)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_slos_does_not_return_other_tenant_definitions() {
    let (ch, _ch_container) = start_clickhouse().await;
    let pg = test_support::postgres::shared_pool().await;
    let app = build_app_with_pg(ch, pg.clone()).await;
    let other_tenant = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO slo_definitions \
         (tenant_id, service_name, environment, sli_type, target, window_days, \
          burn_rate_fast_threshold, burn_rate_slow_threshold, description) \
         VALUES ($1, 'private-svc', 'prod', 'availability', 0.99, 30, 14.4, 1.0, 'Private SLO')",
    )
    .bind(other_tenant)
    .execute(&pg)
    .await
    .expect("other tenant SLO inserted");

    let response = app.oneshot(dev_request("GET", "/v1/slos")).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    let items = body["items"].as_array().expect("items array");
    assert!(
        items
            .iter()
            .all(|item| item["service_name"] != "private-svc"),
        "tenant-scoped list must not include other tenant SLOs"
    );
}

// ── Service Catalog Health Signals (P9-S5) ──────────────────────────────────

#[tokio::test]
async fn service_summary_reports_slo_breach_alert_count_and_latest_deploy() {
    let (ch, _ch_container) = start_clickhouse().await;
    let pg = test_support::postgres::shared_pool().await;
    let dev_tenant: Uuid = DEV_TENANT_ID.parse().unwrap();
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let base = now_ns - 3_000_000_000;

    // checkout: low error rate (error-rate health would be "healthy"), but has a firing
    // SLO burn-rate alert, so health_state must be overridden to "breach".
    insert_span(
        &ch,
        SpanRow {
            service_name: "checkout".into(),
            status_code: "OK".into(),
            ..make_span(dev_tenant, "trace-checkout", "span-checkout", base)
        },
    )
    .await;

    // billing: no SLO, no alerts, no deployment — must keep the existing placeholder
    // defaults and its error-rate-derived health state.
    insert_span(
        &ch,
        SpanRow {
            service_name: "billing".into(),
            status_code: "OK".into(),
            ..make_span(dev_tenant, "trace-billing", "span-billing", base)
        },
    )
    .await;

    let slo_id: Uuid = sqlx::query_scalar(
        "INSERT INTO slo_definitions \
         (tenant_id, service_name, environment, sli_type, target, window_days, \
          burn_rate_fast_threshold, burn_rate_slow_threshold, description) \
         VALUES ($1, 'checkout', 'prod', 'availability', 0.99, 30, 14.4, 1.0, 'Checkout SLO') \
         RETURNING slo_id",
    )
    .bind(dev_tenant)
    .fetch_one(&pg)
    .await
    .expect("slo inserted");

    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, 'Checkout SLO burn', 'slo_burn_rate', 'critical', $2) \
         RETURNING rule_id",
    )
    .bind(dev_tenant)
    .bind(serde_json::json!({ "slo_id": slo_id.to_string() }))
    .fetch_one(&pg)
    .await
    .expect("alert rule inserted");

    sqlx::query(
        "INSERT INTO alert_firings (rule_id, tenant_id, state, value) \
         VALUES ($1, $2, 'active', 5.0)",
    )
    .bind(rule_id)
    .bind(dev_tenant)
    .execute(&pg)
    .await
    .expect("alert firing inserted");

    sqlx::query(
        "INSERT INTO deployment_markers \
         (tenant_id, service_name, environment, service_version, status, started_at) \
         VALUES ($1, 'checkout', 'prod', 'v2.3.1', 'success', NOW())",
    )
    .bind(dev_tenant)
    .execute(&pg)
    .await
    .expect("deployment marker inserted");

    let app = build_app_with_pg(ch, pg).await;

    let response = app
        .oneshot(dev_request("GET", "/v1/services/summary"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    let items = body["items"].as_array().expect("items array");

    let checkout = items
        .iter()
        .find(|item| item["service_name"] == "checkout")
        .expect("checkout summary present");
    assert_eq!(checkout["health_state"], "breach");
    assert!(checkout["active_alert_count"].as_u64().unwrap() >= 1);
    assert_eq!(checkout["latest_deployment"], "v2.3.1");

    let billing = items
        .iter()
        .find(|item| item["service_name"] == "billing")
        .expect("billing summary present");
    assert_eq!(billing["active_alert_count"], 0);
    assert!(billing["latest_deployment"].is_null());
    assert_eq!(billing["health_state"], "healthy");
}

#[tokio::test]
async fn dashboard_get_http_returns_v2_panel_shape() {
    let (ch, _ch_container) = start_clickhouse().await;
    let pg = test_support::postgres::shared_pool().await;
    let app = build_app_with_pg(ch, pg.clone()).await;
    let tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    let created = dashboards::create_dashboard(
        &pg,
        tenant,
        &dashboards::CreateDashboardRequest {
            name: "HTTP dashboard".into(),
            panels: vec![dashboards::DashboardPanelRequest {
                title: "Notes".into(),
                panel_kind: Some("text".into()),
                query_kind: None,
                content: Some("HTTP text panel".into()),
                layout: Some(serde_json::json!({"x":0,"y":0,"w":12,"h":2})),
                ..Default::default()
            }],
        },
        None,
    )
    .await
    .expect("dashboard created");

    let response = app
        .oneshot(dev_request(
            "GET",
            &format!("/v1/dashboards/{}", created.dashboard_id),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(body["name"], "HTTP dashboard");
    assert_eq!(body["panels"][0]["panel_kind"], "text");
    assert_eq!(body["panels"][0]["content"], "HTTP text panel");
    assert_eq!(body["panels"][0]["layout"]["w"], 12);
}

#[tokio::test]
async fn dashboard_put_http_updates_panel_layout() {
    let (ch, _ch_container) = start_clickhouse().await;
    let pg = test_support::postgres::shared_pool().await;
    let app = build_app_with_pg(ch, pg.clone()).await;
    let tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    let created = dashboards::create_dashboard(
        &pg,
        tenant,
        &dashboards::CreateDashboardRequest {
            name: "HTTP dashboard".into(),
            panels: vec![dashboards::DashboardPanelRequest {
                title: "Notes".into(),
                panel_kind: Some("text".into()),
                query_kind: None,
                content: Some("Before".into()),
                layout: Some(serde_json::json!({"x":0,"y":0,"w":6,"h":2})),
                ..Default::default()
            }],
        },
        None,
    )
    .await
    .expect("dashboard created");

    let body = serde_json::json!({
        "name": "Updated dashboard",
        "panels": [{
            "title": "Notes",
            "panel_kind": "text",
            "query_kind": null,
            "preset": null,
            "filters": {},
            "content": "After",
            "layout": {"x":0,"y":0,"w":8,"h":3},
            "time_range": {"mode":"global"}
        }]
    });
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/dashboards/{}", created.dashboard_id))
        .header("Authorization", format!("Bearer {DEV_API_KEY}"))
        .header("X-Tenant-ID", DEV_TENANT_ID)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(body["name"], "Updated dashboard");
    assert_eq!(body["panels"][0]["content"], "After");
    assert_eq!(body["panels"][0]["layout"]["w"], 8);
}

#[tokio::test]
async fn nlq_base_ir_invalid_regex_filter_returns_400() {
    let app = fake_nlq_app_no_db();
    let long_pattern = "a".repeat(257);

    let body = serde_json::json!({
        "mode": "execute",
        "base_ir": {
            "signals": ["logs"],
            "operation": "table",
            "time_range": {
                "from": "now-1h",
                "to": "now"
            },
            "filters": [{
                "field": "service_name",
                "op": "=~",
                "value": long_pattern
            }]
        }
    });

    let req = Request::builder()
        .method("POST")
        .uri("/v1/nlq")
        .header("Authorization", format!("Bearer {DEV_API_KEY}"))
        .header("X-Tenant-ID", DEV_TENANT_ID)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(
        body["error"],
        "invalid filter value for field: service_name"
    );
}

#[tokio::test]
async fn nlq_metrics_base_ir_without_metric_returns_400() {
    let app = fake_nlq_app_no_db();

    let body = serde_json::json!({
        "mode": "execute",
        "base_ir": {
            "signals": ["metrics"],
            "operation": "table",
            "time_range": {
                "from": "now-1h",
                "to": "now"
            },
            "filters": []
        }
    });

    let req = Request::builder()
        .method("POST")
        .uri("/v1/nlq")
        .header("Authorization", format!("Bearer {DEV_API_KEY}"))
        .header("X-Tenant-ID", DEV_TENANT_ID)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(body["error"], "metric is required for this operation");
}

#[tokio::test]
async fn test_mcp_query_rejects_unknown_filter_field() {
    let db = test_support::postgres::shared_pool().await;
    let (ch_client, _ch_container) = start_clickhouse().await;
    let mock_server = start_dev_auth_mock().await;

    let state = query_api::traces::AppState {
        ch: ch_client,
        db: db.clone(),
        planner: Arc::new(QueryPlanner),
        llm: None,
        auth_service_url: mock_server.uri(),
        metrics: Arc::new(query_api::observability::QueryApiMetrics::new()),
    };

    let app = Router::new()
        .route(
            "/v1/mcp/query",
            post(query_api::mcp_query::handle_mcp_query),
        )
        .layer(axum_middleware::from_fn(require_tenant))
        .layer(axum::Extension(db.clone()))
        .layer(axum::Extension(Arc::new(state.auth_service_url.clone())))
        .with_state(state);

    let ir = serde_json::json!({
        "metric": "request_duration_ms",
        "signals": ["metrics"],
        "operation": "table",
        "time_range": {
            "from": "2024-01-01T00:00:00Z",
            "to": "2024-01-02T00:00:00Z"
        },
        "filters": [{
            "field": "invalid_foo",
            "op": "=",
            "value": "bar"
        }]
    });

    let req = Request::builder()
        .method("POST")
        .uri("/v1/mcp/query")
        .header("Authorization", format!("Bearer {}", DEV_API_KEY))
        .header("X-Tenant-ID", DEV_TENANT_ID)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&ir).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = std::str::from_utf8(&body_bytes).unwrap();
    assert!(
        body_str.contains("field 'invalid_foo' not in schema catalog"),
        "unexpected response body: {body_str}"
    );
}

// ── Incident API ────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_incidents_returns_tenant_scoped_incidents() {
    let (ch, _ch_container) = start_clickhouse().await;
    let pg = test_support::postgres::shared_pool().await;
    let app = build_app_with_pg(ch, pg.clone()).await;
    let tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    sqlx::query(
        "INSERT INTO incidents (incident_id, tenant_id, title, severity, status, dedup_key) \
         VALUES ($1, $2, 'HTTP test incident', 'critical', 'triggered', 'dedup-1')",
    )
    .bind(Uuid::new_v4())
    .bind(tenant)
    .execute(&pg)
    .await
    .expect("incident inserted");

    let response = app
        .oneshot(dev_request("GET", "/v1/incidents"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    let items = body["items"].as_array().unwrap();
    assert!(
        items.iter().any(|i| i["title"] == "HTTP test incident"),
        "incident must appear in list"
    );
}

#[tokio::test]
async fn get_incident_returns_detail_with_timeline() {
    let (ch, _ch_container) = start_clickhouse().await;
    let pg = test_support::postgres::shared_pool().await;
    let app = build_app_with_pg(ch, pg.clone()).await;
    let tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();
    let incident_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO incidents (incident_id, tenant_id, title, severity, status, dedup_key) \
         VALUES ($1, $2, 'Detail incident', 'warning', 'resolved', 'dedup-2')",
    )
    .bind(incident_id)
    .bind(tenant)
    .execute(&pg)
    .await
    .expect("incident inserted");

    sqlx::query(
        "INSERT INTO incident_events (incident_id, event_type, actor, message) \
         VALUES ($1, 'triggered', 'system', 'Alert fired')",
    )
    .bind(incident_id)
    .execute(&pg)
    .await
    .expect("event inserted");

    let response = app
        .oneshot(dev_request("GET", &format!("/v1/incidents/{incident_id}")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(body["title"], "Detail incident");
    assert_eq!(body["status"], "resolved");
    let timeline = body["timeline"].as_array().unwrap();
    assert_eq!(timeline.len(), 1);
    assert_eq!(timeline[0]["event_type"], "triggered");
    assert_eq!(timeline[0]["actor"], "system");
}

#[tokio::test]
async fn get_incident_returns_404_for_unknown_id() {
    let (ch, _ch_container) = start_clickhouse().await;
    let pg = test_support::postgres::shared_pool().await;
    let app = build_app_with_pg(ch, pg.clone()).await;
    let unknown_id = Uuid::new_v4();

    let response = app
        .oneshot(dev_request("GET", &format!("/v1/incidents/{unknown_id}")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_incident_detail_includes_rule_name() {
    let (ch, _ch_container) = start_clickhouse().await;
    let pg = test_support::postgres::shared_pool().await;
    let app = build_app_with_pg(ch, pg.clone()).await;
    let tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'CPU High', 'threshold', 'critical', \
                 '{\"metric_name\":\"cpu\",\"operator\":\"gt\",\"threshold\":90}', \
                 '{}', true) \
         RETURNING rule_id",
    )
    .bind(tenant)
    .fetch_one(&pg)
    .await
    .expect("rule inserted");

    let incident_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key, triggered_by_rule_id) \
         VALUES ($1, $2, 'CPU spike', 'critical', 'triggered', 'dedup-rule-1', $3)",
    )
    .bind(incident_id)
    .bind(tenant)
    .bind(rule_id)
    .execute(&pg)
    .await
    .expect("incident inserted");

    let response = app
        .oneshot(dev_request("GET", &format!("/v1/incidents/{incident_id}")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(body["rule_name"], "CPU High");
}

#[tokio::test]
async fn get_incident_detail_rule_name_null_when_no_rule() {
    let (ch, _ch_container) = start_clickhouse().await;
    let pg = test_support::postgres::shared_pool().await;
    let app = build_app_with_pg(ch, pg.clone()).await;
    let tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    let incident_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key) \
         VALUES ($1, $2, 'Manual incident', 'warning', 'triggered', 'dedup-norule')",
    )
    .bind(incident_id)
    .bind(tenant)
    .execute(&pg)
    .await
    .expect("incident inserted");

    let response = app
        .oneshot(dev_request("GET", &format!("/v1/incidents/{incident_id}")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert!(body["rule_name"].is_null());
}

#[tokio::test]
async fn get_alert_rule_returns_detail_with_firings() {
    let (ch, _ch_container) = start_clickhouse().await;
    let pg = test_support::postgres::shared_pool().await;
    let tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'High Error Rate', 'threshold', 'critical', \
                 '{\"metric_name\":\"error_rate\",\"operator\":\"gt\",\"threshold\":0.05}', \
                 '{}', false) \
         RETURNING rule_id",
    )
    .bind(tenant)
    .fetch_one(&pg)
    .await
    .expect("rule inserted");

    for state in ["active", "resolved"] {
        sqlx::query(
            "INSERT INTO alert_firings (rule_id, tenant_id, state, value) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(rule_id)
        .bind(tenant)
        .bind(state)
        .bind(0.08_f64)
        .execute(&pg)
        .await
        .expect("firing inserted");
    }

    let app = build_app_with_pg(ch, pg.clone()).await;
    let response = app
        .oneshot(dev_request("GET", &format!("/v1/alerts/rules/{rule_id}")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(body["name"], "High Error Rate");
    assert_eq!(body["severity"], "critical");
    assert_eq!(body["alert_type"], "threshold");
    let firings = body["firings"].as_array().unwrap();
    assert_eq!(firings.len(), 2);
}

#[tokio::test]
async fn get_alert_rule_returns_404_for_wrong_tenant() {
    let (ch, _ch_container) = start_clickhouse().await;
    let pg = test_support::postgres::shared_pool().await;
    let _tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'Other Tenant Rule', 'threshold', 'warning', \
                 '{\"metric_name\":\"m\",\"operator\":\"gt\",\"threshold\":1.0}', \
                 '{}', false) \
         RETURNING rule_id",
    )
    .bind(Uuid::new_v4()) // different tenant — NOT DEV_TENANT_ID
    .fetch_one(&pg)
    .await
    .expect("rule inserted");

    // Request is authenticated as DEV_TENANT_ID
    let app = build_app_with_pg(ch, pg.clone()).await;
    let response = app
        .oneshot(dev_request("GET", &format!("/v1/alerts/rules/{rule_id}")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_incident_detail_includes_impacted_service_for_slo_rule() {
    let (ch, _ch_container) = start_clickhouse().await;
    let pg = test_support::postgres::shared_pool().await;
    let app = build_app_with_pg(ch, pg.clone()).await;
    let tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    // Seed an SLO definition for "payments" service
    let slo_id: Uuid = sqlx::query_scalar(
        "INSERT INTO slo_definitions \
         (tenant_id, service_name, environment, sli_type, target, window_days, \
          burn_rate_fast_threshold, burn_rate_slow_threshold, description) \
         VALUES ($1, 'payments', 'prod', 'availability', 0.99, 30, 14.4, 1.0, 'Payments SLO') \
         RETURNING slo_id",
    )
    .bind(tenant)
    .fetch_one(&pg)
    .await
    .expect("slo inserted");

    // Seed an slo_burn_rate alert rule referencing the SLO
    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'Payments SLO burn', 'slo_burn_rate', 'critical', $2, '{}', true) \
         RETURNING rule_id",
    )
    .bind(tenant)
    .bind(serde_json::json!({
        "slo_id": slo_id,
        "fast_window_minutes": 60,
        "slow_window_minutes": 360,
    }))
    .fetch_one(&pg)
    .await
    .expect("slo_burn_rate rule inserted");

    // Seed an incident linked to that rule
    let incident_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key, triggered_by_rule_id) \
         VALUES ($1, $2, 'Payments SLO burn', 'critical', 'triggered', 'slo-dedup-1', $3)",
    )
    .bind(incident_id)
    .bind(tenant)
    .bind(rule_id)
    .execute(&pg)
    .await
    .expect("slo incident inserted");

    let response = app
        .oneshot(dev_request("GET", &format!("/v1/incidents/{incident_id}")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(
        body["impacted_service"], "payments",
        "slo_burn_rate incident must carry impacted_service from slo_definitions"
    );
}

#[tokio::test]
async fn get_incident_detail_impacted_service_null_for_threshold_rule() {
    let (ch, _ch_container) = start_clickhouse().await;
    let pg = test_support::postgres::shared_pool().await;
    let app = build_app_with_pg(ch, pg.clone()).await;
    let tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'High CPU', 'threshold', 'warning', \
                 '{\"metric_name\":\"cpu\",\"operator\":\"gt\",\"threshold\":80}', \
                 '{}', true) \
         RETURNING rule_id",
    )
    .bind(tenant)
    .fetch_one(&pg)
    .await
    .expect("threshold rule inserted");

    let incident_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key, triggered_by_rule_id) \
         VALUES ($1, $2, 'High CPU', 'warning', 'triggered', 'threshold-dedup-1', $3)",
    )
    .bind(incident_id)
    .bind(tenant)
    .bind(rule_id)
    .execute(&pg)
    .await
    .expect("threshold incident inserted");

    let response = app
        .oneshot(dev_request("GET", &format!("/v1/incidents/{incident_id}")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert!(
        body["impacted_service"].is_null(),
        "threshold incident must have null impacted_service"
    );
}

#[tokio::test]
async fn get_service_reliability_report_filters_service_environment_and_interval() {
    let pg = test_support::postgres::shared_pool().await;
    let ch = ChClient::default().with_url("http://127.0.0.1:19999");
    let app = build_app_with_pg(ch, pg.clone()).await;
    let tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();
    let service_name = "checkout";
    let from = chrono::Utc::now() - chrono::Duration::hours(6);
    let to = chrono::Utc::now();

    let checkout_prod_slo_id: Uuid = sqlx::query_scalar(
        "INSERT INTO slo_definitions \
         (tenant_id, service_name, environment, sli_type, target, window_days, \
          burn_rate_fast_threshold, burn_rate_slow_threshold, description) \
         VALUES ($1, $2, 'prod', 'availability', 0.99, 30, 14.4, 1.0, 'Checkout prod SLO') \
         RETURNING slo_id",
    )
    .bind(tenant)
    .bind(service_name)
    .fetch_one(&pg)
    .await
    .expect("slo inserted");

    let checkout_prod_rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'Checkout prod SLO burn', 'slo_burn_rate', 'critical', $2, '{}', true) \
         RETURNING rule_id",
    )
    .bind(tenant)
    .bind(serde_json::json!({
        "slo_id": checkout_prod_slo_id,
        "fast_window_minutes": 60,
        "slow_window_minutes": 360,
    }))
    .fetch_one(&pg)
    .await
    .expect("slo rule inserted");

    sqlx::query(
        "INSERT INTO alert_firings (rule_id, tenant_id, state, value, occurred_at) \
         VALUES ($1, $2, 'active', 0.42, NOW())",
    )
    .bind(checkout_prod_rule_id)
    .bind(tenant)
    .execute(&pg)
    .await
    .expect("slo firing inserted");

    let checkout_staging_slo_id: Uuid = sqlx::query_scalar(
        "INSERT INTO slo_definitions \
         (tenant_id, service_name, environment, sli_type, target, window_days, \
          burn_rate_fast_threshold, burn_rate_slow_threshold, description) \
         VALUES ($1, $2, 'staging', 'availability', 0.99, 30, 14.4, 1.0, 'Checkout staging SLO') \
         RETURNING slo_id",
    )
    .bind(tenant)
    .bind(service_name)
    .fetch_one(&pg)
    .await
    .expect("staging slo inserted");

    let checkout_staging_rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'Checkout staging SLO burn', 'slo_burn_rate', 'critical', $2, '{}', true) \
         RETURNING rule_id",
    )
    .bind(tenant)
    .bind(serde_json::json!({
        "slo_id": checkout_staging_slo_id,
        "fast_window_minutes": 60,
        "slow_window_minutes": 360,
    }))
    .fetch_one(&pg)
    .await
    .expect("staging slo rule inserted");

    let payments_prod_slo_id: Uuid = sqlx::query_scalar(
        "INSERT INTO slo_definitions \
         (tenant_id, service_name, environment, sli_type, target, window_days, \
          burn_rate_fast_threshold, burn_rate_slow_threshold, description) \
         VALUES ($1, 'payments', 'prod', 'availability', 0.99, 30, 14.4, 1.0, 'Payments prod SLO') \
         RETURNING slo_id",
    )
    .bind(tenant)
    .fetch_one(&pg)
    .await
    .expect("payments slo inserted");

    let payments_prod_rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'Payments prod SLO burn', 'slo_burn_rate', 'critical', $2, '{}', true) \
         RETURNING rule_id",
    )
    .bind(tenant)
    .bind(serde_json::json!({
        "slo_id": payments_prod_slo_id,
        "fast_window_minutes": 60,
        "slow_window_minutes": 360,
    }))
    .fetch_one(&pg)
    .await
    .expect("payments slo rule inserted");

    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key, triggered_by_rule_id, triggered_at, resolved_at) \
         VALUES ($1, $2, 'Checkout prod resolved', 'critical', 'resolved', 'checkout-prod-resolved', $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant)
    .bind(checkout_prod_rule_id)
    .bind(from + chrono::Duration::hours(1))
    .bind(from + chrono::Duration::hours(2))
    .execute(&pg)
    .await
    .expect("resolved incident inserted");

    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key, triggered_by_rule_id, triggered_at) \
         VALUES ($1, $2, 'Checkout prod open', 'warning', 'triggered', 'checkout-prod-open', $3, $4)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant)
    .bind(checkout_prod_rule_id)
    .bind(from + chrono::Duration::hours(3))
    .execute(&pg)
    .await
    .expect("open incident inserted");

    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key, triggered_by_rule_id, triggered_at, resolved_at) \
         VALUES ($1, $2, 'Checkout staging incident', 'warning', 'resolved', 'checkout-staging', $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant)
    .bind(checkout_staging_rule_id)
    .bind(from + chrono::Duration::hours(1))
    .bind(from + chrono::Duration::hours(2))
    .execute(&pg)
    .await
    .expect("staging incident inserted");

    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key, triggered_by_rule_id, triggered_at, resolved_at) \
         VALUES ($1, $2, 'Payments prod incident', 'critical', 'resolved', 'payments-prod', $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant)
    .bind(payments_prod_rule_id)
    .bind(from + chrono::Duration::hours(1))
    .bind(from + chrono::Duration::hours(2))
    .execute(&pg)
    .await
    .expect("other service incident inserted");

    sqlx::query(
        "INSERT INTO deployment_markers \
         (deployment_id, tenant_id, project_id, service_name, environment, service_version, status, started_at, finished_at, deployed_by, commit_sha, rollback_of, metadata) \
         VALUES ($1, $2, NULL, $3, 'prod', '2026.05.22', 'success', $4, $5, 'ci-bot', 'abc123', NULL, NULL)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant)
    .bind(service_name)
    .bind(from + chrono::Duration::hours(4))
    .bind(from + chrono::Duration::hours(5))
    .execute(&pg)
    .await
    .expect("prod deployment inserted");

    sqlx::query(
        "INSERT INTO deployment_markers \
         (deployment_id, tenant_id, project_id, service_name, environment, service_version, status, started_at, finished_at, deployed_by, commit_sha, rollback_of, metadata) \
         VALUES ($1, $2, NULL, $3, 'staging', '2026.05.21', 'success', $4, $5, 'ci-bot', 'def456', NULL, NULL)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant)
    .bind(service_name)
    .bind(from + chrono::Duration::hours(4))
    .bind(from + chrono::Duration::hours(5))
    .execute(&pg)
    .await
    .expect("staging deployment inserted");

    let uri = format!(
        "/v1/services/{service_name}/reliability-report?from={}&to={}&environment=prod",
        from.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        to.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    );
    let response = app.oneshot(dev_request("GET", &uri)).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(body["service_name"], service_name);
    assert_eq!(body["environment"], "prod");
    assert_eq!(body["incident_summary"]["total"], 2);
    assert_eq!(body["incident_summary"]["open"], 1);
    assert_eq!(body["incident_summary"]["resolved"], 1);
    assert_eq!(body["slo_summary"]["total"], 1);
    assert_eq!(body["slo_summary"]["firing"], 1);
    assert_eq!(body["deployments"].as_array().unwrap().len(), 1);

    let incidents = body["incidents"].as_array().unwrap();
    assert!(
        incidents.iter().all(|incident| {
            incident["title"] != "Checkout staging incident"
                && incident["title"] != "Payments prod incident"
        }),
        "report must only include the target service and environment"
    );

    let mean_time = body["incident_summary"]["mean_time_to_resolve_minutes"]
        .as_f64()
        .expect("MTTR value");
    assert!(
        mean_time >= 59.0,
        "expected MTTR to reflect the resolved incident duration, got {mean_time}"
    );
}
