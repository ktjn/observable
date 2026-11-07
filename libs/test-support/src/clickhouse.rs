use clickhouse::Client;
use std::path::Path;
use std::sync::OnceLock;
use testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner};
use testcontainers_modules::clickhouse::ClickHouse;
use tokio::sync::OnceCell;

static CONTAINER: OnceCell<ContainerAsync<ClickHouse>> = OnceCell::const_new();
static MIGRATED: OnceCell<()> = OnceCell::const_new();

// See the matching comment in postgres.rs: `Drop` never runs for `static`
// values at normal process exit, so this records the container id for
// `cleanup_on_exit` to remove directly via the `docker` CLI.
static CONTAINER_ID: OnceLock<String> = OnceLock::new();

#[dtor::dtor(unsafe)]
fn cleanup_on_exit() {
    if let Some(id) = CONTAINER_ID.get() {
        let _ = std::process::Command::new("docker")
            .args(["rm", "-f", "-v", id])
            .output();
    }
}

/// Environment variables naming an external server (e.g. the stack started by
/// `scripts/local-stack.sh`). Required under cargo-nextest, which runs every
/// test in its own process and would otherwise start one container per test.
pub const EXTERNAL_URL_ENV: &str = "OBSERVABLE_TEST_CLICKHOUSE_URL";
pub const EXTERNAL_USER_ENV: &str = "OBSERVABLE_TEST_CLICKHOUSE_USER";
pub const EXTERNAL_PASSWORD_ENV: &str = "OBSERVABLE_TEST_CLICKHOUSE_PASSWORD";

const CONTAINER_USER: &str = "default";
const CONTAINER_PASSWORD: &str = "test";

struct Server {
    url: String,
    user: String,
    password: String,
}

static SERVER: OnceCell<Server> = OnceCell::const_new();

async fn server() -> &'static Server {
    SERVER
        .get_or_init(|| async {
            if let Ok(url) = std::env::var(EXTERNAL_URL_ENV)
                && !url.trim().is_empty()
            {
                return Server {
                    url: url.trim().trim_end_matches('/').to_string(),
                    user: std::env::var(EXTERNAL_USER_ENV).unwrap_or_else(|_| "default".into()),
                    password: std::env::var(EXTERNAL_PASSWORD_ENV).unwrap_or_default(),
                };
            }
            let container = CONTAINER
                .get_or_init(|| async {
                    let container = ClickHouse::default()
                        .with_tag("26.9")
                        .with_env_var("CLICKHOUSE_USER", CONTAINER_USER)
                        .with_env_var("CLICKHOUSE_PASSWORD", CONTAINER_PASSWORD)
                        .start()
                        .await
                        .expect("clickhouse container started");
                    let _ = CONTAINER_ID.set(container.id().to_string());
                    container
                })
                .await;
            let port = container.get_host_port_ipv4(8123).await.expect("port");
            Server {
                url: format!("http://127.0.0.1:{port}"),
                user: CONTAINER_USER.into(),
                password: CONTAINER_PASSWORD.into(),
            }
        })
        .await
}

/// Root (database-less) client for the server `shared_client` uses, for tests
/// that manage their own databases.
pub async fn root_client() -> Client {
    let server = server().await;
    Client::default()
        .with_url(&server.url)
        .with_user(&server.user)
        .with_password(&server.password)
        // See docs/agent-context.md: clickhouse-rs 0.15.2 can't decompress
        // ClickHouse 26.9's response framing. Remove once a fixed release ships.
        .with_compression(clickhouse::Compression::None)
}

/// Returns a client scoped to the shared `observable` database. Starts the
/// shared ClickHouse container and applies migrations on first use (once per
/// process); subsequent calls reuse both. Callers are responsible for using a
/// unique tenant_id per test to stay isolated from other tests sharing this
/// database.
pub async fn shared_client() -> Client {
    // Migrations are idempotent (CREATE ... IF NOT EXISTS, MODIFY SETTING), so
    // re-applying them once per process is safe against a shared server.
    MIGRATED.get_or_init(apply_migrations).await;
    root_client().await.with_database("observable")
}

async fn apply_migrations() {
    let root = root_client().await;

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
