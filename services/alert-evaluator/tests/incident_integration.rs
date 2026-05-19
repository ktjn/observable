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

async fn create_threshold_rule_with_auto_trigger(
    pool: &PgPool,
    tenant_id: Uuid,
    metric_name: &str,
    threshold: f64,
    auto_trigger: bool,
) -> Uuid {
    let rule_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO alert_rules \
         (rule_id, tenant_id, name, alert_type, severity, condition, auto_trigger_incident) \
         VALUES ($1, $2, 'test incident rule', 'threshold', 'warning', $3, $4)",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .bind(serde_json::json!({
        "metric_name": metric_name,
        "operator": "gt",
        "threshold": threshold,
    }))
    .bind(auto_trigger)
    .execute(pool)
    .await
    .expect("threshold rule inserted");
    rule_id
}

async fn create_threshold_rule_with_runbook(
    pool: &PgPool,
    tenant_id: Uuid,
    metric_name: &str,
    threshold: f64,
    runbook_url: &str,
) -> Uuid {
    let rule_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO alert_rules \
         (rule_id, tenant_id, name, alert_type, severity, condition, auto_trigger_incident, runbook_url) \
         VALUES ($1, $2, 'runbook test rule', 'threshold', 'warning', $3, true, $4)",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .bind(serde_json::json!({
        "metric_name": metric_name,
        "operator": "gt",
        "threshold": threshold,
    }))
    .bind(runbook_url)
    .execute(pool)
    .await
    .expect("threshold rule with runbook inserted");
    rule_id
}

#[tokio::test]
async fn auto_trigger_incident_created_on_active_firing() {
    let (pool, _pg) = start_postgres().await;
    let (ch, _ch) = start_clickhouse().await;
    let tenant_id = Uuid::new_v4();
    let metric_name = "incident_test_metric";
    let rule_id =
        create_threshold_rule_with_auto_trigger(&pool, tenant_id, metric_name, 0.05, true).await;

    insert_metric_point(&ch, tenant_id, metric_name, 0.10).await;
    eval_threshold_rules(&pool, &ch).await.unwrap();

    let incident: Option<(Uuid, String, String)> = sqlx::query_as(
        "SELECT incident_id, status, title FROM incidents WHERE tenant_id = $1 AND triggered_by_rule_id = $2",
    )
    .bind(tenant_id)
    .bind(rule_id)
    .fetch_optional(&pool)
    .await
    .unwrap();

    let (incident_id, status, title) = incident.expect("incident must be created");
    assert_eq!(status, "triggered");
    assert_eq!(title, "test incident rule");

    let events: Vec<(String, String)> = sqlx::query_as(
        "SELECT event_type, actor FROM incident_events WHERE incident_id = $1 ORDER BY event_time ASC",
    )
    .bind(incident_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].0, "triggered");
    assert_eq!(events[0].1, "system");
    assert_eq!(events[1].0, "alert_fired");
    assert_eq!(events[1].1, "system");
}

#[tokio::test]
async fn auto_trigger_incident_resolved_when_alert_resolves() {
    let (pool, _pg) = start_postgres().await;
    let (ch, _ch) = start_clickhouse().await;
    let tenant_id = Uuid::new_v4();
    let metric_name = "incident_resolve_metric";
    let rule_id =
        create_threshold_rule_with_auto_trigger(&pool, tenant_id, metric_name, 0.05, true).await;

    insert_metric_point(&ch, tenant_id, metric_name, 0.10).await;
    eval_threshold_rules(&pool, &ch).await.unwrap();

    let incident_before: (Uuid, String) = sqlx::query_as(
        "SELECT incident_id, status FROM incidents WHERE tenant_id = $1 AND triggered_by_rule_id = $2",
    )
    .bind(tenant_id)
    .bind(rule_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(incident_before.1, "triggered");

    insert_metric_point(&ch, tenant_id, metric_name, 0.01).await;
    eval_threshold_rules(&pool, &ch).await.unwrap();

    let incident_after: (Uuid, String) = sqlx::query_as(
        "SELECT incident_id, status FROM incidents WHERE tenant_id = $1 AND triggered_by_rule_id = $2",
    )
    .bind(tenant_id)
    .bind(rule_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(incident_after.1, "resolved");
    assert_eq!(incident_after.0, incident_before.0);

    let events: Vec<(String, String)> = sqlx::query_as(
        "SELECT event_type, actor FROM incident_events WHERE incident_id = $1 ORDER BY event_time ASC",
    )
    .bind(incident_after.0)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(events.len(), 3);
    assert_eq!(events[0].0, "triggered");
    assert_eq!(events[1].0, "alert_fired");
    assert_eq!(events[2].0, "alert_resolved");
}

#[tokio::test]
async fn no_incident_when_auto_trigger_is_false() {
    let (pool, _pg) = start_postgres().await;
    let (ch, _ch) = start_clickhouse().await;
    let tenant_id = Uuid::new_v4();
    let metric_name = "no_incident_metric";
    let rule_id =
        create_threshold_rule_with_auto_trigger(&pool, tenant_id, metric_name, 0.05, false).await;

    insert_metric_point(&ch, tenant_id, metric_name, 0.10).await;
    eval_threshold_rules(&pool, &ch).await.unwrap();

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM incidents WHERE tenant_id = $1 AND triggered_by_rule_id = $2",
    )
    .bind(tenant_id)
    .bind(rule_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        count, 0,
        "incident must not be created when auto_trigger_incident is false"
    );
}

#[tokio::test]
async fn runbook_url_copied_from_rule_to_incident() {
    let (pool, _pg) = start_postgres().await;
    let (ch, _ch) = start_clickhouse().await;
    let tenant_id = Uuid::new_v4();
    let metric_name = "runbook_propagation_metric";
    let runbook = "https://runbooks.example.com/high-error-rate";
    let rule_id =
        create_threshold_rule_with_runbook(&pool, tenant_id, metric_name, 0.05, runbook).await;

    insert_metric_point(&ch, tenant_id, metric_name, 0.10).await;
    eval_threshold_rules(&pool, &ch).await.unwrap();

    let fetched_runbook: Option<String> = sqlx::query_scalar(
        "SELECT runbook_url FROM incidents \
         WHERE tenant_id = $1 AND triggered_by_rule_id = $2",
    )
    .bind(tenant_id)
    .bind(rule_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(fetched_runbook.as_deref(), Some(runbook));
}
