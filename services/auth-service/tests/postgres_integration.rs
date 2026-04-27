use auth_service::lookup_api_key;
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

#[tokio::test]
async fn postgres_container_applies_migrations_and_validates_seed_key() {
    let container = Postgres::default()
        .with_tag("16")
        .start()
        .await
        .expect("postgres container started");

    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.expect("pool connected");

    apply_migrations(&pool).await;

    // dev-api-key-0000 / tenant 00000000-0000-0000-0000-000000000001 / role member is seeded by migrations
    let (tenant_id, role): (Uuid, String) = lookup_api_key(&pool, "dev-api-key-0000")
        .await
        .expect("seed key must validate");

    assert_eq!(
        tenant_id.to_string(),
        "00000000-0000-0000-0000-000000000001"
    );
    assert_eq!(role, "member");
}
