use alert_evaluator::evaluator::eval_change_detection_rules;
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

/// Inserts a metric point `age_secs` seconds in the past, so tests can place
/// points precisely inside the current window (age_secs small) or the
/// baseline window (age_secs ~= baseline_offset_secs).
async fn insert_metric_point_aged(
    ch: &clickhouse::Client,
    tenant_id: Uuid,
    metric_name: &str,
    value: f64,
    age_secs: i64,
) {
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let age_ns = (age_secs.max(0) as u64) * 1_000_000_000;
    let time_unix_nano = now_ns.saturating_sub(age_ns);
    let row = MetricPointRow {
        tenant_id,
        metric_series_id: Uuid::new_v4(),
        metric_name: metric_name.into(),
        service_name: "checkout".into(),
        time_unix_nano,
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

async fn create_change_detection_rule(
    pool: &PgPool,
    tenant_id: Uuid,
    metric_name: &str,
    window_secs: i64,
    baseline_offset_secs: i64,
    threshold_percent: f64,
) -> Uuid {
    let rule_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO alert_rules \
         (rule_id, tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, $2, 'test change detection rule', 'change_detection', 'warning', $3)",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .bind(serde_json::json!({
        "metric_name": metric_name,
        "window_secs": window_secs,
        "baseline_offset_secs": baseline_offset_secs,
        "threshold_percent": threshold_percent,
    }))
    .execute(pool)
    .await
    .expect("change detection rule inserted");
    rule_id
}

#[tokio::test]
async fn change_detection_fires_when_current_diverges_from_baseline() {
    let (pool, _pg) = start_postgres().await;
    let (ch, _ch) = start_clickhouse().await;
    let tenant_id = Uuid::new_v4();
    let metric_name = "error_rate";
    let window_secs = 300;
    let baseline_offset_secs = 3600;

    let rule_id = create_change_detection_rule(
        &pool,
        tenant_id,
        metric_name,
        window_secs,
        baseline_offset_secs,
        50.0,
    )
    .await;

    // Baseline window point: well inside [now - offset - window, now - offset].
    insert_metric_point_aged(
        &ch,
        tenant_id,
        metric_name,
        100.0,
        baseline_offset_secs + (window_secs / 2),
    )
    .await;

    // Current window point: well inside [now - window, now]. 150 vs 100 baseline
    // is a 50% increase, which fires at the 50% threshold.
    insert_metric_point_aged(&ch, tenant_id, metric_name, 150.0, window_secs / 2).await;

    eval_change_detection_rules(&pool, &ch).await.unwrap();

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

#[tokio::test]
async fn change_detection_stays_ok_when_within_threshold() {
    let (pool, _pg) = start_postgres().await;
    let (ch, _ch) = start_clickhouse().await;
    let tenant_id = Uuid::new_v4();
    let metric_name = "error_rate";
    let window_secs = 300;
    let baseline_offset_secs = 3600;

    let rule_id = create_change_detection_rule(
        &pool,
        tenant_id,
        metric_name,
        window_secs,
        baseline_offset_secs,
        50.0,
    )
    .await;

    // Baseline window point.
    insert_metric_point_aged(
        &ch,
        tenant_id,
        metric_name,
        100.0,
        baseline_offset_secs + (window_secs / 2),
    )
    .await;

    // Current window point: 110 vs 100 baseline is a 10% increase, under the
    // 50% threshold, so the rule must stay ok (no active firing).
    insert_metric_point_aged(&ch, tenant_id, metric_name, 110.0, window_secs / 2).await;

    eval_change_detection_rules(&pool, &ch).await.unwrap();

    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM alert_firings \
         WHERE rule_id = $1 AND tenant_id = $2 AND state = 'active'",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(active_count, 0);
}

#[tokio::test]
async fn change_detection_skips_rule_when_baseline_window_has_no_data() {
    let (pool, _pg) = start_postgres().await;
    let (ch, _ch) = start_clickhouse().await;
    let tenant_id = Uuid::new_v4();
    let metric_name = "error_rate";
    let window_secs = 300;
    let baseline_offset_secs = 3600;

    let rule_id = create_change_detection_rule(
        &pool,
        tenant_id,
        metric_name,
        window_secs,
        baseline_offset_secs,
        50.0,
    )
    .await;

    // Only a current-window point; no baseline data exists at all.
    insert_metric_point_aged(&ch, tenant_id, metric_name, 150.0, window_secs / 2).await;

    eval_change_detection_rules(&pool, &ch).await.unwrap();

    let firing_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM alert_firings WHERE rule_id = $1 AND tenant_id = $2",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(firing_count, 0, "rule must be skipped, not resolved/fired");
}
