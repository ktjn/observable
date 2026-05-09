use query_api::slos::list_slos;
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
async fn list_slos_returns_seeded_dev_slo() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    let slos = list_slos(&pool, tenant).await.unwrap();

    assert!(slos.iter().any(|s| s.service_name == "checkout"));
    let checkout = slos.iter().find(|s| s.service_name == "checkout").unwrap();
    assert_eq!(checkout.environment, "prod");
    assert_eq!(checkout.sli_type, "availability");
    assert!((checkout.target - 0.999).abs() < f64::EPSILON);
}
