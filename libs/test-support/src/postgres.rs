use sqlx::PgPool;
use std::path::Path;
use std::sync::OnceLock;
use testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tokio::sync::OnceCell;

static CONTAINER: OnceCell<ContainerAsync<Postgres>> = OnceCell::const_new();

// `ContainerAsync`'s `Drop` impl is the only cleanup mechanism this crate version
// has (no Ryuk/reaper sidecar) -- but `Drop` never runs for `static` values at
// normal process exit, so the container above would leak on every test run.
// This records the container id so `cleanup_on_exit` (below) can remove it
// directly via the `docker` CLI, bypassing Rust's Drop entirely.
static CONTAINER_ID: OnceLock<String> = OnceLock::new();

#[dtor::dtor(unsafe)]
fn cleanup_on_exit() {
    if let Some(id) = CONTAINER_ID.get() {
        let _ = std::process::Command::new("docker")
            .args(["rm", "-f", id])
            .output();
    }
}

async fn admin_url() -> String {
    let container = CONTAINER
        .get_or_init(|| async {
            let container = Postgres::default()
                .with_tag("17")
                .start()
                .await
                .expect("postgres container started");
            let _ = CONTAINER_ID.set(container.id().to_string());
            container
        })
        .await;
    let host = container.get_host().await.expect("host");
    let port = container.get_host_port_ipv4(5432).await.expect("port");
    format!("postgres://postgres:postgres@{host}:{port}")
}

/// Returns a pool connected to a freshly created, migrated database, unique to
/// this call. Starts the shared Postgres 17 container on first use (once per
/// process); subsequent calls reuse it and only pay the cost of creating a new
/// database plus running migrations.
pub async fn shared_pool() -> PgPool {
    let base_url = admin_url().await;

    let admin_pool = PgPool::connect(&format!("{base_url}/postgres"))
        .await
        .expect("admin pool connected");
    let db_name = format!("test_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!(
        r#"CREATE DATABASE "{db_name}""#
    )))
    .execute(&admin_pool)
    .await
    .expect("test database created");
    admin_pool.close().await;

    let pool = PgPool::connect(&format!("{base_url}/{db_name}"))
        .await
        .expect("test database pool connected");
    apply_migrations(&pool).await;
    pool
}

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
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
            .execute(pool)
            .await
            .expect("migration applied");
    }
}
