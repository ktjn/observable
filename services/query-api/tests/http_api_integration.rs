use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware as axum_middleware,
    routing::{get, post},
    Router,
};
use clickhouse::Client as ChClient;
use domain::{LogRow, MetricPointRow, MetricSeriesRow, SpanRow};
use http_body_util::BodyExt;
use query_api::{
    config, logs, metrics, middleware::auth::require_tenant, planner::QueryPlanner, traces,
};
use serde_json::Value;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::{path::Path, sync::Arc};
use testcontainers::{runners::AsyncRunner, ImageExt};
use testcontainers_modules::{clickhouse::ClickHouse, postgres::Postgres};
use tower::ServiceExt;
use uuid::Uuid;

// ── Dev credentials (must match seed data in migrations) ────────────────────
// Migration 017 moves dev-key to the dev-tenant at ...0002.
// Tenant ...0001 is the 'observable' self-ingestion tenant.

const DEV_TENANT_ID: &str = "00000000-0000-0000-0000-000000000002";
const DEV_API_KEY: &str = "dev-api-key-0000";

// ── Container helpers ────────────────────────────────────────────────────────

async fn start_clickhouse() -> (ChClient, testcontainers::ContainerAsync<ClickHouse>) {
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

async fn start_postgres() -> (PgPool, testcontainers::ContainerAsync<Postgres>) {
    let container = Postgres::default()
        .with_tag("16")
        .start()
        .await
        .expect("postgres container started");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("postgres pool connected");
    apply_pg_migrations(&pool).await;
    (pool, container)
}

async fn apply_pg_migrations(pool: &PgPool) {
    let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("migrations/postgres");

    let mut entries: Vec<_> = std::fs::read_dir(&migrations_dir)
        .expect("migrations/postgres must exist")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let sql = std::fs::read_to_string(entry.path()).expect("readable migration");
        sqlx::raw_sql(&sql)
            .execute(pool)
            .await
            .expect("pg migration applied");
    }
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

fn build_app_with_pg(ch: ChClient, db: PgPool) -> Router {
    let state = traces::AppState {
        ch,
        db: db.clone(),
        planner: Arc::new(QueryPlanner),
        llm: None,
        auth_service_url: "http://auth-service:4319".into(),
    };
    Router::new()
        .route("/v1/traces/histogram", get(traces::trace_histogram))
        .route("/v1/logs/histogram", get(logs::log_histogram))
        .route("/v1/metrics", get(metrics::list_metrics))
        .route("/v1/metrics/points", get(metrics::get_metric_group_points))
        .layer(axum_middleware::from_fn(require_tenant))
        .layer(axum::Extension(db))
        .with_state(state)
}

/// App used for tests where auth rejection happens before the handler is
/// reached (i.e. missing Authorization header → immediate 401). The lazy
/// Postgres pool is never actually queried in these cases.
fn fake_app_no_db() -> Router {
    let db =
        PgPool::connect_lazy("postgres://user:pass@127.0.0.1:5432/db").expect("valid postgres url");
    let ch = ChClient::default().with_url("http://127.0.0.1:19999");
    let state = traces::AppState {
        ch,
        db: db.clone(),
        planner: Arc::new(QueryPlanner),
        llm: None,
        auth_service_url: "http://auth-service:4319".into(),
    };
    Router::new()
        .route("/v1/traces/histogram", get(traces::trace_histogram))
        .route("/v1/logs/histogram", get(logs::log_histogram))
        .route("/v1/metrics", get(metrics::list_metrics))
        .route("/v1/metrics/points", get(metrics::get_metric_group_points))
        .layer(axum_middleware::from_fn(require_tenant))
        .layer(axum::Extension(db))
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
    let app = fake_app_no_db();
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
    let (db, _pg) = start_postgres().await;
    let ch = ChClient::default().with_url("http://127.0.0.1:19999");
    let app = build_app_with_pg(ch, db);

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
    let (db, _pg) = start_postgres().await;
    let (ch, _ch_container) = start_clickhouse().await;
    let app = build_app_with_pg(ch, db);

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
    let app = fake_app_no_db();
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
    let app = fake_app_no_db();
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

// ── Histogram query-string parsing (regression for nanosecond timestamps) ────

#[tokio::test]
async fn trace_histogram_accepts_nanosecond_u64_timestamps() {
    let (ch, _container) = start_clickhouse().await;
    let (db, _pg) = start_postgres().await;
    let app = build_app_with_pg(ch, db);

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
    let (db, _pg) = start_postgres().await;
    let app = build_app_with_pg(ch, db);

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
    let (db, _pg) = start_postgres().await;
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

    let app = build_app_with_pg(ch, db);
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
    let (db, _pg) = start_postgres().await;
    let dev_tenant: Uuid = DEV_TENANT_ID.parse().unwrap();
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    let base = now_ns - 3_000_000_000;
    insert_log(&ch, make_log(dev_tenant, "svc", base)).await;
    insert_log(&ch, make_log(dev_tenant, "svc", base + 1_000_000_000)).await;
    insert_log(&ch, make_log(dev_tenant, "svc", base + 2_000_000_000)).await;

    let app = build_app_with_pg(ch, db);
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
    let (db, _pg) = start_postgres().await;
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

    let app = build_app_with_pg(ch, db);
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
    let (db, _pg) = start_postgres().await;
    let dev_tenant: Uuid = DEV_TENANT_ID.parse().unwrap();
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    let base = now_ns - 2_000_000_000;
    insert_log(&ch, make_log(dev_tenant, "svc-a", base)).await;
    insert_log(&ch, make_log(dev_tenant, "svc-b", base + 500_000_000)).await;

    let app = build_app_with_pg(ch, db);
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
    let (db, _pg) = start_postgres().await;
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

    let app = build_app_with_pg(ch, db);
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
    let (db, _pg) = start_postgres().await;
    let dev_tenant: Uuid = DEV_TENANT_ID.parse().unwrap();
    let first_series = Uuid::new_v4();
    let second_series = Uuid::new_v4();
    let ts_ns = 1_777_819_009_493_000_000;

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

    let app = build_app_with_pg(ch, db);
    let uri = "/v1/metrics/points?metric_name=span.calls_total&service=checkout&environment=prod&metric_type=sum&unit=1";
    let resp = app
        .oneshot(dev_request("GET", uri))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let json = response_body_json(resp.into_body()).await;
    let points = json["points"].as_array().expect("points array");
    assert_eq!(points.len(), 1, "same timestamp should be aggregated");
    assert_eq!(points[0]["metric_name"], "span.calls_total");
    assert_eq!(points[0]["value_double"], 5.0);
}

// ── LLM models endpoint ──────────────────────────────────────────────────────

#[tokio::test]
async fn llm_models_returns_ok_false_when_unreachable() {
    let (db, _pg) = start_postgres().await;
    let ch = ChClient::default().with_url("http://127.0.0.1:19999");
    let state = traces::AppState {
        ch,
        db: db.clone(),
        planner: Arc::new(QueryPlanner),
        llm: None,
    };
    let app = Router::new()
        .route("/v1/config/llm/models", post(config::list_llm_models))
        .layer(axum_middleware::from_fn(require_tenant))
        .layer(axum::Extension(db))
        .with_state(state);

    // Port 1 is reserved/refused on all standard OS configurations.
    let body = r#"{"url":"http://127.0.0.1:1","api_key":""}"#;
    let req = Request::builder()
        .method("POST")
        .uri("/v1/config/llm/models")
        .header("Authorization", format!("Bearer {DEV_API_KEY}"))
        .header("X-Tenant-ID", DEV_TENANT_ID)
        .header("Content-Type", "application/json")
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = response_body_json(resp.into_body()).await;
    assert_eq!(
        json["ok"], false,
        "unreachable endpoint must return ok:false"
    );
    assert_eq!(
        json["models"].as_array().expect("models array").len(),
        0,
        "models must be empty when connection fails"
    );
}
