use alert_evaluator::evaluator::eval_slo_burn_rate_rules;
use domain::SpanRow;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::path::Path;
use testcontainers::{runners::AsyncRunner, ImageExt};
use testcontainers_modules::{clickhouse::ClickHouse, postgres::Postgres};
use uuid::Uuid;

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

async fn apply_ch_migrations(base_url: &str, user: &str, password: &str) -> clickhouse::Client {
    let root = clickhouse::Client::default()
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
                    .expect("ch migration applied");
            }
        }
    }

    clickhouse::Client::default()
        .with_url(base_url)
        .with_user(user)
        .with_password(password)
        .with_database("observable")
}

async fn start_clickhouse() -> (
    clickhouse::Client,
    testcontainers::ContainerAsync<ClickHouse>,
) {
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

fn make_span(
    tenant_id: Uuid,
    service_name: &str,
    environment: &str,
    offset_minutes: u64,
    error: bool,
) -> SpanRow {
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let start_time_unix_nano = now_ns - offset_minutes * 60 * 1_000_000_000;
    SpanRow {
        tenant_id,
        trace_id: Uuid::new_v4().to_string(),
        span_id: Uuid::new_v4().to_string(),
        parent_span_id: None,
        service_name: service_name.into(),
        service_namespace: String::new(),
        service_version: String::new(),
        operation_name: "GET /checkout".into(),
        span_kind: "SERVER".into(),
        start_time_unix_nano,
        end_time_unix_nano: start_time_unix_nano + 1_000_000,
        duration_ns: 1_000_000,
        status_code: if error { "ERROR" } else { "OK" }.into(),
        status_message: String::new(),
        attributes: "{}".into(),
        resource_attributes: "{}".into(),
        environment: environment.into(),
        host_id: "host-1".into(),
        workload: String::new(),
        deployment_id: String::new(),
    }
}

async fn insert_span(ch: &clickhouse::Client, row: SpanRow) {
    let mut insert = ch
        .insert::<SpanRow>("spans")
        .await
        .expect("span insert handle");
    insert.write(&row).await.expect("span written");
    insert.end().await.expect("span insert committed");
}

async fn insert_spans(
    ch: &clickhouse::Client,
    tenant_id: Uuid,
    service_name: &str,
    environment: &str,
) {
    for idx in 0..100 {
        let error = idx < 20;
        insert_span(
            ch,
            make_span(tenant_id, service_name, environment, 30, error),
        )
        .await;
    }
    for idx in 0..100 {
        let error = idx < 10;
        insert_span(
            ch,
            make_span(tenant_id, service_name, environment, 180, error),
        )
        .await;
    }
}

#[tokio::test]
async fn slo_burn_rate_rule_fires_when_fast_and_slow_windows_burn() {
    let (pool, _pg) = start_postgres().await;
    let (ch, _ch) = start_clickhouse().await;
    let tenant_id = Uuid::new_v4();
    let slo_id = Uuid::new_v4();
    let rule_id = Uuid::new_v4();
    let service_name = "checkout";
    let environment = "prod";

    sqlx::query(
        "INSERT INTO slo_definitions \
         (slo_id, tenant_id, service_name, environment, sli_type, target, window_days, \
          burn_rate_fast_threshold, burn_rate_slow_threshold, description) \
         VALUES ($1, $2, $3, $4, 'availability', 0.99, 30, 5.0, 2.0, 'Checkout availability')",
    )
    .bind(slo_id)
    .bind(tenant_id)
    .bind(service_name)
    .bind(environment)
    .execute(&pool)
    .await
    .expect("SLO inserted");

    sqlx::query(
        "INSERT INTO alert_rules (rule_id, tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, $2, 'Checkout burn rate', 'slo_burn_rate', 'critical', $3)",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .bind(serde_json::json!({
        "slo_id": slo_id,
        "fast_window_minutes": 60,
        "slow_window_minutes": 360,
    }))
    .execute(&pool)
    .await
    .expect("SLO burn-rate rule inserted");

    insert_spans(&ch, tenant_id, service_name, environment).await;

    eval_slo_burn_rate_rules(&pool, &ch).await.unwrap();

    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM alert_firings \
         WHERE rule_id = $1 AND tenant_id = $2 AND state = 'active'",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(active_count, 1);
}
