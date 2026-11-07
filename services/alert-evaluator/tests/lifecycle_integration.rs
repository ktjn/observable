use alert_evaluator::evaluator::{eval_alert_rules, eval_threshold_rules};
use observable_storage_contracts::MetricPointRow;
use sqlx::postgres::PgPool;
use uuid::Uuid;

async fn start_postgres() -> (PgPool, test_support::postgres::TestDatabase) {
    test_support::postgres::isolated_pool().await
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
    let ch = test_support::clickhouse::shared_client().await;
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

#[tokio::test]
async fn composite_rule_fires_only_when_both_source_rules_are_active() {
    let (pool, _pg) = start_postgres().await;
    let ch = test_support::clickhouse::shared_client().await;
    let tenant_id = Uuid::new_v4();
    let left_rule_id = create_threshold_rule(&pool, tenant_id, "left_metric", 0.05, None).await;
    let right_rule_id = create_threshold_rule(&pool, tenant_id, "right_metric", 0.10, None).await;
    let composite_rule_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO alert_rules \
         (rule_id, tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, $2, 'left AND right', 'composite', 'critical', $3)",
    )
    .bind(composite_rule_id)
    .bind(tenant_id)
    .bind(serde_json::json!({
        "left_rule_id": left_rule_id,
        "right_rule_id": right_rule_id,
    }))
    .execute(&pool)
    .await
    .expect("composite rule inserted");

    insert_metric_point(&ch, tenant_id, "left_metric", 0.10).await;
    eval_threshold_rules(&pool, &ch).await.unwrap();
    eval_alert_rules(&pool, &ch).await.unwrap();

    let first_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM alert_firings \
         WHERE rule_id = $1 AND tenant_id = $2 AND state = 'active'",
    )
    .bind(composite_rule_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(first_count, 0);

    insert_metric_point(&ch, tenant_id, "right_metric", 0.20).await;
    eval_threshold_rules(&pool, &ch).await.unwrap();
    eval_alert_rules(&pool, &ch).await.unwrap();

    let second_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM alert_firings \
         WHERE rule_id = $1 AND tenant_id = $2 AND state = 'active'",
    )
    .bind(composite_rule_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(second_count, 1);
}
