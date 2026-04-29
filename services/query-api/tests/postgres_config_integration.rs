use query_api::config::{db_key_present, fetch_db_key, upsert_db_key};
use sqlx::PgPool;
use std::path::Path;
use testcontainers::{runners::AsyncRunner, ImageExt};
use testcontainers_modules::postgres::Postgres;

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

async fn start_pool() -> (PgPool, testcontainers::ContainerAsync<Postgres>) {
    let container = Postgres::default()
        .with_tag("16")
        .start()
        .await
        .expect("postgres container started");
    let host = container.get_host().await.expect("host");
    let port = container.get_host_port_ipv4(5432).await.expect("port");
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    let pool = PgPool::connect(&url).await.expect("pool connected");
    apply_migrations(&pool).await;
    (pool, container)
}

#[tokio::test]
async fn db_key_absent_when_table_empty() {
    let (pool, _container) = start_pool().await;
    let present = db_key_present(&pool).await.expect("query ok");
    assert!(!present, "no key should be present in fresh DB");
}

#[tokio::test]
async fn upsert_and_fetch_key_roundtrip() {
    let (pool, _container) = start_pool().await;
    upsert_db_key(&pool, "sk-test-key")
        .await
        .expect("upsert ok");
    let key = fetch_db_key(&pool).await.expect("fetch ok");
    assert_eq!(key.as_deref(), Some("sk-test-key"));
}

#[tokio::test]
async fn db_key_present_after_upsert() {
    let (pool, _container) = start_pool().await;
    upsert_db_key(&pool, "sk-test-key")
        .await
        .expect("upsert ok");
    let present = db_key_present(&pool).await.expect("query ok");
    assert!(present, "key should be present after upsert");
}

#[tokio::test]
async fn upsert_overwrites_existing_key() {
    let (pool, _container) = start_pool().await;
    upsert_db_key(&pool, "sk-first")
        .await
        .expect("first upsert");
    upsert_db_key(&pool, "sk-second")
        .await
        .expect("second upsert");
    let key = fetch_db_key(&pool).await.expect("fetch ok");
    assert_eq!(
        key.as_deref(),
        Some("sk-second"),
        "second upsert should overwrite"
    );
}

#[tokio::test]
async fn fetch_key_returns_none_when_absent() {
    let (pool, _container) = start_pool().await;
    let key = fetch_db_key(&pool).await.expect("fetch ok");
    assert!(key.is_none(), "no key expected in fresh DB");
}
