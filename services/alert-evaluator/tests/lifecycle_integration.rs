use alert_evaluator::evaluator::eval_threshold_rules;
use domain::MetricPointRow;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::path::Path;
use testcontainers::{ImageExt, runners::AsyncRunner};
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

async fn insert_metric_point(
    ch: &clickhouse::Client,
    tenant_id: Uuid,
    metric_name: &str,
    value: f64,
) {
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let row = MetricPointRow {
        tenant_id,
        metric_series_id: Uuid::new_v4(),
        metric_name: metric_name.into(),
        service_name: "checkout".into(),
        time_unix_nano: now_ns,
        start_time_unix_nano: None,
        value_double: Some(value),
        value_int: None,
        histogram_count: None,
        histogram_sum: None,
        histogram_bucket_counts: vec![],
        histogram_explicit_bounds: vec![],
    };
    let mut insert = ch
        .insert::<MetricPointRow>("metric_points")
        .await
        .expect("metric point insert handle");
    insert.write(&row).await.expect("metric point written");
    insert.end().await.expect("metric point insert committed");
}

async fn create_threshold_rule(
    pool: &PgPool,
    tenant_id: Uuid,
    metric_name: &str,
    threshold: f64,
    for_duration_secs: Option<i64>,
) -> Uuid {
    let rule_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO alert_rules \
         (rule_id, tenant_id, name, alert_type, severity, condition, for_duration_secs) \
         VALUES ($1, $2, 'test lifecycle rule', 'threshold', 'warning', $3, $4)",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .bind(serde_json::json!({
        "metric_name": metric_name,
        "operator": "gt",
        "threshold": threshold,
    }))
    .bind(for_duration_secs)
    .execute(pool)
    .await
    .expect("threshold rule inserted");
    rule_id
}

#[tokio::test]
async fn threshold_lifecycle_pending_active_dedupe_and_resolve() {
    let (pool, _pg) = start_postgres().await;
    let (ch, _ch) = start_clickhouse().await;
    let tenant_id = Uuid::new_v4();
    let metric_name = "rf4_error_rate";
    let rule_id = create_threshold_rule(&pool, tenant_id, metric_name, 0.05, Some(60)).await;

    insert_metric_point(&ch, tenant_id, metric_name, 0.10).await;
    eval_threshold_rules(&pool, &ch).await.unwrap();

    let first_state: String =
        sqlx::query_scalar("SELECT state FROM alert_firings WHERE rule_id = $1 AND tenant_id = $2")
            .bind(rule_id)
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(first_state, "pending");

    sqlx::query(
        "UPDATE alert_firings \
         SET firing_start = NOW() - INTERVAL '61 seconds', \
             occurred_at = NOW() - INTERVAL '61 seconds' \
         WHERE rule_id = $1 AND tenant_id = $2",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    insert_metric_point(&ch, tenant_id, metric_name, 0.11).await;
    eval_threshold_rules(&pool, &ch).await.unwrap();
    insert_metric_point(&ch, tenant_id, metric_name, 0.12).await;
    eval_threshold_rules(&pool, &ch).await.unwrap();

    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM alert_firings \
         WHERE rule_id = $1 AND tenant_id = $2 AND state = 'active'",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active_count, 1, "active firing must be reused");

    insert_metric_point(&ch, tenant_id, metric_name, 0.01).await;
    eval_threshold_rules(&pool, &ch).await.unwrap();

    let (resolved_count, unresolved_count): (i64, i64) = sqlx::query_as(
        "SELECT \
             COUNT(*) FILTER (WHERE state = 'resolved' AND resolved_at IS NOT NULL), \
             COUNT(*) FILTER (WHERE state IN ('pending', 'active')) \
         FROM alert_firings \
         WHERE rule_id = $1 AND tenant_id = $2",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(resolved_count, 1);
    assert_eq!(unresolved_count, 0);
}
