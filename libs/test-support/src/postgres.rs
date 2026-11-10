//! Shared PostgreSQL fixture for integration tests.
//!
//! Two backends:
//! - **External** (`OBSERVABLE_TEST_POSTGRES_URL` set, e.g. by
//!   `scripts/local-stack.sh`): reuse a long-lived server. Required under
//!   cargo-nextest, which runs every test in its own process and would
//!   otherwise start one container per test.
//! - **Testcontainers** (variable unset): start one Postgres container per
//!   test process, as before.
//!
//! Either way, migrations run once into a template database named after a
//! hash of `migrations/postgres`; every `shared_pool()` call clones that
//! template (`CREATE DATABASE ... TEMPLATE`) instead of replaying all
//! migrations.

use sqlx::{Connection, PgConnection, PgPool};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tokio::sync::OnceCell;

/// Environment variable naming an external server, without a database path:
/// `postgres://user:password@host:port`. The user must be able to create
/// databases.
pub const EXTERNAL_URL_ENV: &str = "OBSERVABLE_TEST_POSTGRES_URL";

/// Matches the `postgres` service in docker-compose.yml.
const IMAGE_TAG: &str = "18";

/// Arbitrary constant key serializing template creation across processes.
const TEMPLATE_LOCK_KEY: i64 = 0x6f62_735f_7465_6d70;

static CONTAINER: OnceCell<ContainerAsync<Postgres>> = OnceCell::const_new();
static BASE_URL: OnceCell<String> = OnceCell::const_new();
static TEMPLATE: OnceCell<String> = OnceCell::const_new();

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
            .args(["rm", "-f", "-v", id])
            .output();
    }
}

async fn base_url() -> &'static str {
    BASE_URL
        .get_or_init(|| async {
            if let Ok(url) = std::env::var(EXTERNAL_URL_ENV)
                && !url.trim().is_empty()
            {
                return url.trim().trim_end_matches('/').to_string();
            }
            let container = CONTAINER
                .get_or_init(|| async {
                    let container = Postgres::default()
                        .with_tag(IMAGE_TAG)
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
        })
        .await
}

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("migrations/postgres")
}

fn migration_files() -> Vec<PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(migrations_dir())
        .expect("migrations/postgres must exist")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "sql"))
        .collect();
    files.sort();
    files
}

/// FNV-1a over file names and contents: stable across toolchains, unlike
/// `DefaultHasher`, so a persistent server keeps reusing the same template
/// until a migration changes.
fn migrations_fingerprint(files: &[PathBuf]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            hash ^= u64::from(*b);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for file in files {
        feed(file.file_name().unwrap().as_encoded_bytes());
        feed(&[0]);
        feed(&std::fs::read(file).expect("readable migration"));
        feed(&[0]);
    }
    format!("{hash:016x}")
}

async fn admin_connection(base_url: &str) -> PgConnection {
    PgConnection::connect(&format!("{base_url}/postgres"))
        .await
        .expect("admin connection")
}

async fn database_exists(conn: &mut PgConnection, name: &str) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)")
        .bind(name)
        .fetch_one(&mut *conn)
        .await
        .expect("pg_database lookup")
}

async fn execute(conn: &mut PgConnection, sql: String) {
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
        .execute(&mut *conn)
        .await
        .expect("admin statement");
}

/// Returns the name of a migrated template database, creating it on first
/// use. Cross-process safe: creation runs under a session advisory lock, and
/// a template is only renamed into place once fully migrated.
async fn template(base_url: &str) -> &'static str {
    TEMPLATE
        .get_or_init(|| async {
            let files = migration_files();
            let name = format!("obs_tmpl_{}", migrations_fingerprint(&files));
            let mut admin = admin_connection(base_url).await;
            if database_exists(&mut admin, &name).await {
                return name;
            }
            sqlx::query("SELECT pg_advisory_lock($1)")
                .bind(TEMPLATE_LOCK_KEY)
                .execute(&mut admin)
                .await
                .expect("template lock");
            if !database_exists(&mut admin, &name).await {
                let staging = format!("{name}_staging");
                execute(
                    &mut admin,
                    format!(r#"DROP DATABASE IF EXISTS "{staging}""#),
                )
                .await;
                execute(&mut admin, format!(r#"CREATE DATABASE "{staging}""#)).await;
                {
                    let mut conn = PgConnection::connect(&observable_config::with_search_path(
                        &format!("{base_url}/{staging}"),
                    ))
                    .await
                    .expect("staging template connection");
                    for file in &files {
                        let sql = std::fs::read_to_string(file).expect("readable migration");
                        sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
                            .execute(&mut conn)
                            .await
                            .unwrap_or_else(|e| panic!("migration {} failed: {e}", file.display()));
                    }
                    conn.close().await.expect("close staging connection");
                }
                execute(
                    &mut admin,
                    format!(r#"ALTER DATABASE "{staging}" RENAME TO "{name}""#),
                )
                .await;
                execute(
                    &mut admin,
                    format!(
                        r#"ALTER DATABASE "{name}" WITH IS_TEMPLATE true ALLOW_CONNECTIONS false"#
                    ),
                )
                .await;
            }
            sqlx::query("SELECT pg_advisory_unlock($1)")
                .bind(TEMPLATE_LOCK_KEY)
                .execute(&mut admin)
                .await
                .expect("template unlock");
            let _ = admin.close().await;
            name
        })
        .await
}

async fn create_database() -> (PgPool, String) {
    let base_url = base_url().await;
    let template = template(base_url).await;

    let db_name = format!("test_{}", uuid::Uuid::new_v4().simple());
    let mut admin = admin_connection(base_url).await;
    // Default strategy (WAL_LOG): cheaper than FILE_COPY for a ~10 MB
    // template, and it avoids the two checkpoints FILE_COPY forces per clone.
    execute(
        &mut admin,
        format!(r#"CREATE DATABASE "{db_name}" TEMPLATE "{template}""#),
    )
    .await;
    // Database-level settings (ALTER DATABASE ... SET, see migration 040) are
    // keyed by database OID and are not copied from the template.
    execute(
        &mut admin,
        format!(r#"ALTER DATABASE "{db_name}" SET search_path = public, auth, control, alerting"#),
    )
    .await;
    let _ = admin.close().await;

    // See migrations/postgres/040_component_ownership_schemas.sql: the
    // connection URL must carry the search_path option itself, since a
    // server-side ALTER DATABASE default doesn't apply retroactively to
    // pooled connections opened before it ran.
    let pool = PgPool::connect(&observable_config::with_search_path(&format!(
        "{base_url}/{db_name}"
    )))
    .await
    .expect("test database pool connected");
    (pool, db_name)
}

/// Returns a pool connected to a freshly created, migrated database, unique to
/// this call. The first call in a process resolves the server (external or
/// container) and the migrated template; every call then clones the template,
/// which costs a file copy instead of a full migration replay.
///
/// The database outlives the call; `scripts/local-stack.sh up` drops leftover
/// `test_*` databases. Prefer [`isolated_pool`] where the caller can hold a
/// guard.
pub async fn shared_pool() -> PgPool {
    create_database().await.0
}

/// Like [`shared_pool`], plus a guard that drops the database when it goes
/// out of scope, so a long-lived external server does not accumulate one
/// database per test.
pub async fn isolated_pool() -> (PgPool, TestDatabase) {
    let (pool, name) = create_database().await;
    (pool, TestDatabase { name })
}

/// Drops its database on drop. See [`isolated_pool`].
pub struct TestDatabase {
    name: String,
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        let Some(base_url) = BASE_URL.get().cloned() else {
            return;
        };
        let name = std::mem::take(&mut self.name);
        // Drop can run inside a Tokio runtime, where blocking on another
        // runtime panics; do the async cleanup on a plain thread instead.
        // The test's pool belongs to the test runtime (blocked on the join
        // below), so it is not closed here: WITH (FORCE) terminates its
        // sessions server-side.
        let cleanup = std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            runtime.block_on(async move {
                if let Ok(mut admin) = PgConnection::connect(&format!("{base_url}/postgres")).await
                {
                    let _ = sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                        r#"DROP DATABASE IF EXISTS "{name}" WITH (FORCE)"#
                    )))
                    .execute(&mut admin)
                    .await;
                    let _ = admin.close().await;
                }
            });
        });
        let _ = cleanup.join();
    }
}

/// Root connection URL (no database path) of the server `shared_pool` uses,
/// for tests that need to open their own connections, e.g. as a component
/// login user.
pub async fn server_url() -> String {
    base_url().await.to_string()
}

/// A pool on the same database as `owner`, logged in as a component's scoped
/// login (migrations 041/042 create `observable_<component>` with that password),
/// so grant gaps fail the test instead of hiding behind the owner.
pub async fn scoped_pool(owner: &PgPool, login: &str) -> PgPool {
    let options = owner
        .connect_options()
        .as_ref()
        .clone()
        .username(login)
        .password(login);
    sqlx::postgres::PgPoolOptions::new()
        .connect_with(options)
        .await
        .expect("connect as scoped login")
}

/// Tables in `schema` that carry a `tenant_id` column, sorted.
pub async fn tenant_tables(owner: &PgPool, schema: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT c.table_name::text FROM information_schema.columns c \
         JOIN information_schema.tables t USING (table_schema, table_name) \
         WHERE c.table_schema = $1 AND c.column_name = 'tenant_id' AND t.table_type = 'BASE TABLE' \
         ORDER BY 1",
    )
    .bind(schema)
    .fetch_all(owner)
    .await
    .expect("list tenant tables")
}

/// `(table, rows)` for `tenant` in every tenant table of `schema`.
pub async fn tenant_row_counts(
    owner: &PgPool,
    schema: &str,
    tenant: uuid::Uuid,
) -> Vec<(String, i64)> {
    let mut counts = Vec::new();
    for table in tenant_tables(owner, schema).await {
        // Identifiers come from information_schema, not from input.
        let rows: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            r#"SELECT count(*) FROM "{schema}"."{table}" WHERE tenant_id = $1"#
        )))
        .bind(tenant)
        .fetch_one(owner)
        .await
        .expect("count tenant rows");
        counts.push((table, rows));
    }
    counts
}
