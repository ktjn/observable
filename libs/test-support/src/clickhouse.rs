use clickhouse::Client;
use std::path::Path;
use testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner};
use testcontainers_modules::clickhouse::ClickHouse;
use tokio::sync::OnceCell;

static CONTAINER: OnceCell<ContainerAsync<ClickHouse>> = OnceCell::const_new();
static MIGRATED: OnceCell<()> = OnceCell::const_new();

const USER: &str = "default";
const PASSWORD: &str = "test";

async fn base_url() -> String {
    let container = CONTAINER
        .get_or_init(|| async {
            ClickHouse::default()
                .with_tag("25.3")
                .with_env_var("CLICKHOUSE_USER", USER)
                .with_env_var("CLICKHOUSE_PASSWORD", PASSWORD)
                .start()
                .await
                .expect("clickhouse container started")
        })
        .await;
    let port = container.get_host_port_ipv4(8123).await.expect("port");
    format!("http://127.0.0.1:{port}")
}

/// Returns a client scoped to the shared `observable` database. Starts the
/// shared ClickHouse container and applies migrations on first use (once per
/// process); subsequent calls reuse both. Callers are responsible for using a
/// unique tenant_id per test to stay isolated from other tests sharing this
/// database.
pub async fn shared_client() -> Client {
    let base_url = base_url().await;
    MIGRATED
        .get_or_init(|| async { apply_migrations(&base_url).await })
        .await;

    Client::default()
        .with_url(&base_url)
        .with_user(USER)
        .with_password(PASSWORD)
        .with_database("observable")
}

async fn apply_migrations(base_url: &str) {
    let root = Client::default()
        .with_url(base_url)
        .with_user(USER)
        .with_password(PASSWORD);

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
                    .expect("migration statement applied");
            }
        }
    }
}
