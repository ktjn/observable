// HTTP integration tests for the tenant usage report endpoint:
//   GET /v1/tenants/usage-report
//
// This is the only admin-service endpoint that reads from ClickHouse (telemetry
// counts) in addition to Postgres (control-plane audit logs), so this is the
// first admin-service test file to stand up a ClickHouse testcontainer.

use admin_service::{AdminServiceAppState, middleware::auth::TenantContext, observability, usage};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use clickhouse::Client as ChClient;
use domain::{LogRow, MetricPointRow, MetricSeriesRow, SpanRow};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::{path::Path, sync::Arc};
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::{clickhouse::ClickHouse, postgres::Postgres};
use tower::ServiceExt;
use uuid::Uuid;

// ── Dev credentials (must match seed data in migrations) ────────────────────
// Migration 017 moves dev-key to the dev-tenant at ...0002.
// Tenant ...0001 is the 'observable' self-ingestion tenant.

const DEV_TENANT_ID: &str = "00000000-0000-0000-0000-000000000002";

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

async fn start_postgres() -> (PgPool, testcontainers::ContainerAsync<Postgres>) {
    let container = Postgres::default()
        .with_tag("17")
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
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
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
//
// Unlike query-api's http_api_integration.rs, this app builder injects
// `TenantContext` directly as an `Extension` rather than running it through
// `require_tenant` + a wiremock auth-service stub — same shortcut used by
// admin_members_integration.rs in this crate, since these tests only need to
// exercise the handler, not the auth middleware.

fn build_app(ch: ChClient, db: PgPool) -> Router {
    let state = AdminServiceAppState {
        ch,
        db: db.clone(),
        auth_service_url: "http://auth-service:4319".into(),
        metrics: Arc::new(observability::AdminServiceMetrics::new()),
    };
    let tenant_id = Uuid::parse_str(DEV_TENANT_ID).unwrap();
    Router::new()
        .route(
            "/v1/tenants/usage-report",
            get(usage::handle_get_tenant_usage_report),
        )
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id: None,
            role: "tenant_admin".into(),
        }))
        .with_state(state)
}

fn get_request(uri: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
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

// ── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_tenant_usage_report_scopes_to_tenant_and_interval() {
    let (ch, _ch_container) = start_clickhouse().await;
    let (pg, _pg_container) = start_postgres().await;
    let app = build_app(ch.clone(), pg.clone());
    let tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();
    let other_tenant = Uuid::new_v4();

    let from = chrono::Utc::now() - chrono::Duration::hours(2);
    let to = chrono::Utc::now() + chrono::Duration::minutes(5);

    let in_window_ns = (from + chrono::Duration::minutes(15))
        .timestamp_nanos_opt()
        .unwrap() as u64;
    let out_window_ns = (from - chrono::Duration::hours(4))
        .timestamp_nanos_opt()
        .unwrap() as u64;

    let in_window_series_id = Uuid::new_v4();
    let out_window_series_id = Uuid::new_v4();
    let other_tenant_series_id = Uuid::new_v4();

    insert_span(&ch, make_span(tenant, "trace-in", "span-in", in_window_ns)).await;
    insert_span(
        &ch,
        make_span(tenant, "trace-old", "span-old", out_window_ns),
    )
    .await;
    insert_span(
        &ch,
        make_span(other_tenant, "trace-other", "span-other", in_window_ns),
    )
    .await;

    insert_log(&ch, make_log(tenant, "checkout", in_window_ns)).await;
    insert_log(&ch, make_log(tenant, "checkout", out_window_ns)).await;
    insert_log(&ch, make_log(other_tenant, "checkout", in_window_ns)).await;

    insert_metric_series(
        &ch,
        MetricSeriesRow {
            tenant_id: tenant,
            metric_series_id: in_window_series_id,
            metric_name: "checkout.requests_total".into(),
            description: String::new(),
            unit: "1".into(),
            metric_type: "sum".into(),
            is_monotonic: Some(1),
            aggregation_temporality: Some("delta".into()),
            attributes: "{}".into(),
            resource_attributes: "{}".into(),
            service_name: "checkout".into(),
            environment: "prod".into(),
        },
    )
    .await;
    ch.query(
        "INSERT INTO observable.metric_series \
         (tenant_id, metric_series_id, metric_name, metric_type, service_name, created_at) \
         VALUES (?, ?, 'checkout.requests_total', 'sum', 'checkout', toDateTime(?))",
    )
    .bind(tenant)
    .bind(out_window_series_id)
    .bind((from - chrono::Duration::hours(4)).timestamp())
    .execute()
    .await
    .expect("out-of-window metric_series inserted");
    ch.query(
        "INSERT INTO observable.metric_series \
         (tenant_id, metric_series_id, metric_name, metric_type, service_name, created_at) \
         VALUES (?, ?, 'checkout.requests_total', 'sum', 'checkout', toDateTime(?))",
    )
    .bind(other_tenant)
    .bind(other_tenant_series_id)
    .bind((from + chrono::Duration::minutes(15)).timestamp())
    .execute()
    .await
    .expect("other-tenant metric_series inserted");

    insert_metric_point(
        &ch,
        make_metric_point(
            tenant,
            in_window_series_id,
            "checkout.requests_total",
            in_window_ns,
            4,
        ),
    )
    .await;
    insert_metric_point(
        &ch,
        make_metric_point(
            tenant,
            in_window_series_id,
            "checkout.requests_total",
            out_window_ns,
            9,
        ),
    )
    .await;
    insert_metric_point(
        &ch,
        make_metric_point(
            other_tenant,
            other_tenant_series_id,
            "checkout.requests_total",
            in_window_ns,
            7,
        ),
    )
    .await;

    sqlx::query(
        "INSERT INTO query_audit_log (occurred_at, action, tenant_id, result_count) \
         VALUES ($1, 'trace_get', $2, 4)",
    )
    .bind(from + chrono::Duration::minutes(20))
    .bind(tenant)
    .execute(&pg)
    .await
    .expect("query audit row inserted");
    sqlx::query(
        "INSERT INTO query_audit_log (occurred_at, action, tenant_id, result_count) \
         VALUES ($1, 'log_search', $2, 2)",
    )
    .bind(from + chrono::Duration::minutes(30))
    .bind(tenant)
    .execute(&pg)
    .await
    .expect("second query audit row inserted");
    sqlx::query(
        "INSERT INTO query_audit_log (occurred_at, action, tenant_id, result_count) \
         VALUES ($1, 'trace_get', $2, 8)",
    )
    .bind(from - chrono::Duration::hours(1))
    .bind(tenant)
    .execute(&pg)
    .await
    .expect("out-of-window query audit row inserted");
    sqlx::query(
        "INSERT INTO query_audit_log (occurred_at, action, tenant_id, result_count) \
         VALUES ($1, 'trace_get', $2, 6)",
    )
    .bind(from + chrono::Duration::minutes(25))
    .bind(other_tenant)
    .execute(&pg)
    .await
    .expect("other-tenant query audit row inserted");

    sqlx::query(
        "INSERT INTO credential_audit_log (occurred_at, action, outcome, credential_hash, tenant_id, denial_reason) \
         VALUES ($1, 'credential_validate', 'allow', 'hash-allow', $2, NULL)",
    )
    .bind(from + chrono::Duration::minutes(22))
    .bind(tenant)
    .execute(&pg)
    .await
    .expect("credential allow row inserted");
    sqlx::query(
        "INSERT INTO credential_audit_log (occurred_at, action, outcome, credential_hash, tenant_id, denial_reason) \
         VALUES ($1, 'credential_validate', 'deny', 'hash-deny', $2, 'revoked')",
    )
    .bind(from + chrono::Duration::minutes(23))
    .bind(tenant)
    .execute(&pg)
    .await
    .expect("credential deny row inserted");
    sqlx::query(
        "INSERT INTO credential_audit_log (occurred_at, action, outcome, credential_hash, tenant_id, denial_reason) \
         VALUES ($1, 'credential_validate', 'deny', 'hash-old', $2, 'expired')",
    )
    .bind(from - chrono::Duration::hours(1))
    .bind(tenant)
    .execute(&pg)
    .await
    .expect("out-of-window credential row inserted");

    let uri = format!(
        "/v1/tenants/usage-report?from={}&to={}",
        from.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        to.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    );
    let response = app.oneshot(get_request(&uri)).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(body["tenant_id"], DEV_TENANT_ID);
    assert_eq!(body["telemetry_summary"]["spans"], 1);
    assert_eq!(body["telemetry_summary"]["logs"], 1);
    assert_eq!(body["telemetry_summary"]["metric_points"], 1);
    assert_eq!(body["telemetry_summary"]["metric_series_created"], 1);
    assert_eq!(body["control_plane_summary"]["query_reads"], 2);
    assert_eq!(body["control_plane_summary"]["query_rows"], 6);
    assert_eq!(body["control_plane_summary"]["credential_checks"], 2);
    assert_eq!(body["control_plane_summary"]["credential_allows"], 1);
    assert_eq!(body["control_plane_summary"]["credential_denies"], 1);
    assert_eq!(body["estimated_cost_index"], 43);
}

#[tokio::test]
async fn get_tenant_usage_report_returns_zeroes_for_empty_interval() {
    let (ch, _ch_container) = start_clickhouse().await;
    let (pg, _pg_container) = start_postgres().await;
    let app = build_app(ch, pg.clone());

    let from = chrono::Utc::now() - chrono::Duration::days(3);
    let to = chrono::Utc::now() - chrono::Duration::days(2);
    let uri = format!(
        "/v1/tenants/usage-report?from={}&to={}",
        from.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        to.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    );
    let response = app.oneshot(get_request(&uri)).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(body["telemetry_summary"]["spans"], 0);
    assert_eq!(body["telemetry_summary"]["logs"], 0);
    assert_eq!(body["telemetry_summary"]["metric_points"], 0);
    assert_eq!(body["telemetry_summary"]["metric_series_created"], 0);
    assert_eq!(body["control_plane_summary"]["query_reads"], 0);
    assert_eq!(body["control_plane_summary"]["query_rows"], 0);
    assert_eq!(body["control_plane_summary"]["credential_checks"], 0);
    assert_eq!(body["control_plane_summary"]["credential_allows"], 0);
    assert_eq!(body["control_plane_summary"]["credential_denies"], 0);
    assert_eq!(body["estimated_cost_index"], 0);
}
