use query_api::alerts::{create_alert_rule, list_alert_rules, silence_alert_rule, CreateRuleRequest};
use sqlx::PgPool;
use std::path::Path;
use testcontainers::{runners::AsyncRunner, ImageExt};
use testcontainers_modules::postgres::Postgres;
use uuid::Uuid;

async fn apply_migrations(pool: &PgPool) {
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
            .expect("migration applied");
    }
}

async fn start_pool() -> (
    PgPool,
    testcontainers::ContainerAsync<testcontainers_modules::postgres::Postgres>,
) {
    let container = Postgres::default()
        .with_tag("16")
        .start()
        .await
        .expect("postgres container started");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.expect("pool connected");
    apply_migrations(&pool).await;
    (pool, container)
}

#[tokio::test]
async fn list_rules_returns_seeded_dev_rule() {
    let (pool, _container) = start_pool().await;
    let dev_tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    let rules = list_alert_rules(&pool, dev_tenant).await.unwrap();

    assert!(!rules.is_empty(), "dev tenant must have at least one seeded rule");
    assert!(
        rules.iter().any(|r| r.name == "High error rate"),
        "seeded 'High error rate' rule must be present"
    );
    let seeded = rules.iter().find(|r| r.name == "High error rate").unwrap();
    assert_eq!(seeded.metric_name, "error_rate");
    assert_eq!(seeded.operator, "gt");
    assert!(!seeded.silenced);
    assert!(!seeded.firing);
}

#[tokio::test]
async fn create_rule_appears_in_list() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();

    let req = CreateRuleRequest {
        name: "Test latency rule".into(),
        metric_name: "p95_latency_ms".into(),
        operator: "gt".into(),
        threshold: 500.0,
    };
    let created = create_alert_rule(&pool, tenant, &req).await.unwrap();

    assert_eq!(created.name, "Test latency rule");
    assert_eq!(created.metric_name, "p95_latency_ms");
    assert_eq!(created.operator, "gt");
    assert!((created.threshold - 500.0).abs() < f64::EPSILON);
    assert_eq!(created.severity, "warning");
    assert!(!created.silenced);
    assert!(!created.firing);

    let rules = list_alert_rules(&pool, tenant).await.unwrap();
    assert!(
        rules.iter().any(|r| r.rule_id == created.rule_id),
        "created rule must appear in list"
    );
}

#[tokio::test]
async fn silence_toggle_updates_silenced_flag() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();

    let req = CreateRuleRequest {
        name: "Silenceable rule".into(),
        metric_name: "cpu_usage".into(),
        operator: "gt".into(),
        threshold: 0.9,
    };
    let created = create_alert_rule(&pool, tenant, &req).await.unwrap();
    assert!(!created.silenced);

    let silenced = silence_alert_rule(&pool, tenant, created.rule_id, true)
        .await
        .unwrap()
        .expect("rule must exist for tenant");
    assert!(silenced.silenced);

    let unsilenced = silence_alert_rule(&pool, tenant, created.rule_id, false)
        .await
        .unwrap()
        .expect("rule must exist for tenant");
    assert!(!unsilenced.silenced);
}

#[tokio::test]
async fn silence_returns_none_for_cross_tenant_rule() {
    let (pool, _container) = start_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    let req = CreateRuleRequest {
        name: "Private rule".into(),
        metric_name: "requests".into(),
        operator: "lt".into(),
        threshold: 1.0,
    };
    let created = create_alert_rule(&pool, tenant_a, &req).await.unwrap();

    let result = silence_alert_rule(&pool, tenant_b, created.rule_id, true)
        .await
        .unwrap();
    assert!(
        result.is_none(),
        "tenant B must not be able to silence tenant A's rule"
    );

    let rules = list_alert_rules(&pool, tenant_a).await.unwrap();
    let rule = rules.iter().find(|r| r.rule_id == created.rule_id).unwrap();
    assert!(!rule.silenced, "rule must remain unsilenced after cross-tenant attempt");
}

#[tokio::test]
async fn list_rules_does_not_return_other_tenant_rules() {
    let (pool, _container) = start_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    let req = CreateRuleRequest {
        name: "Tenant A rule".into(),
        metric_name: "errors".into(),
        operator: "gt".into(),
        threshold: 10.0,
    };
    create_alert_rule(&pool, tenant_a, &req).await.unwrap();

    let tenant_b_rules = list_alert_rules(&pool, tenant_b).await.unwrap();
    assert!(
        tenant_b_rules.is_empty(),
        "tenant B must not see tenant A's rules"
    );
}
