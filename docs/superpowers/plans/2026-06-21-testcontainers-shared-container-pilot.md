# Shared Testcontainers Pilot (libs/test-support + query-api) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace per-test-function Testcontainers spin-up in `query-api`'s integration tests
with one shared Postgres container and one shared ClickHouse container per test binary, cutting
container starts from ~60+ down to 2 for the crate's full test run, while preserving full test
isolation and parallelism.

**Architecture:** A new `libs/test-support` workspace crate holds two singleton helpers
(`postgres::shared_pool()`, `clickhouse::shared_client()`), each backed by a `tokio::sync::OnceCell`
that starts its container once per process. Postgres isolation is a fresh, migrated
`test_<uuid>` database per call. ClickHouse isolation stays on the single `observable` database
(production SQL hardcodes that name) with per-test random tenant IDs as the existing isolation
boundary. All of `query-api`'s Testcontainers-using test files move from separate `tests/*.rs`
binaries into one `tests/it.rs` binary (via `mod` declarations under `tests/it/`) so the
in-process singleton is actually shared across them.

**Tech Stack:** Rust, Cargo workspaces, `testcontainers` 0.27.3, `testcontainers-modules` 0.15.0
(`postgres`, `clickhouse` features), `tokio::sync::OnceCell`, `sqlx`, `clickhouse` crate.

## Global Constraints

- `cargo fmt --all` after every Rust edit, before staging/committing.
- `libs/test-support` is a dev-dependency only for service crates — never a `[dependencies]` entry.
- Per-test databases/topics are never explicitly dropped; the container teardown (Ryuk reaper, at
  process exit) is the only cleanup. Don't add manual `DROP DATABASE` calls.
- Do not touch production source (`services/query-api/src/**`) in this plan — every change is
  confined to `libs/test-support/**` and `services/query-api/tests/**`, plus `Cargo.toml` files.
- `services/query-api/tests/http_api_integration.rs`'s ClickHouse-touching tests (those calling
  `start_clickhouse()`) are explicitly **out of scope** for container sharing — leave
  `start_clickhouse()` and every one of its call sites completely untouched. Only that file's
  `start_postgres()` half is migrated. (Reason: its tests key inserted ClickHouse rows off a fixed
  `DEV_TENANT_ID` required by the auth header path, and the production trace/log histogram
  endpoints expose no per-test query discriminator finer than tenant + time window — making
  collision-proof sharing impossible without sacrificing parallelism via serialization, which the
  design explicitly avoids.)
- Where a file uses a **fixed** `Uuid::from_u128(...)` or `Uuid::parse_str(...)` tenant constant
  reused across multiple `#[tokio::test]` functions for ClickHouse inserts (not the
  `DEV_TENANT_ID` HTTP-auth case above), convert it to a per-test local `Uuid::new_v4()` as part
  of that file's migration — required for correctness once the container is shared, not optional
  cleanup.

---

## Task 1: Scaffold `libs/test-support` crate

**Files:**
- Create: `libs/test-support/Cargo.toml`
- Create: `libs/test-support/src/lib.rs`
- Modify: `Cargo.toml:4-12` (workspace `members`)

**Interfaces:**
- Produces: the `test-support` crate, empty modules `postgres` and `clickhouse` (filled in Tasks
  2-3), referenceable as `test-support = { path = "../../libs/test-support" }`.

- [ ] **Step 1: Add the workspace member**

In `Cargo.toml`, add `"libs/test-support",` to the `members` list (after `"libs/observable-auth",`):

```toml
[workspace]
resolver = "2"
members = [
    "libs/domain",
    "libs/observable-auth",
    "libs/test-support",
    "services/ingest-gateway",
    "services/auth-service",
    "services/stream-processor",
    "services/storage-writer",
    "services/query-api",
    "services/alert-evaluator",
    "services/admin-service",
]
```

- [ ] **Step 2: Create the crate manifest**

```toml
[package]
name    = "test-support"
version = "0.1.0"
edition.workspace = true

[lib]
name = "test_support"
path = "src/lib.rs"

[dependencies]
tokio                  = { workspace = true }
sqlx                   = { workspace = true }
clickhouse             = { workspace = true }
testcontainers         = "0.27.3"
testcontainers-modules  = { version = "0.15.0", features = ["postgres", "clickhouse"] }
uuid                   = { workspace = true }
```

- [ ] **Step 3: Create the crate root and empty module files**

```rust
// libs/test-support/src/lib.rs
pub mod clickhouse;
pub mod postgres;
```

Create `libs/test-support/src/postgres.rs` and `libs/test-support/src/clickhouse.rs` as empty
files (Tasks 2-3 fill them in).

- [ ] **Step 4: Verify the workspace resolves**

Run: `cargo check -p test-support`
Expected: compiles clean.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml libs/test-support
git commit -m "feat(test-support): scaffold shared Testcontainers crate"
```

---

## Task 2: Implement `test_support::postgres::shared_pool`

**Files:**
- Create: `libs/test-support/src/postgres.rs`
- Test: `libs/test-support/tests/postgres_shared_pool.rs`
- Modify: `libs/test-support/Cargo.toml` (add `[dev-dependencies]`)

**Interfaces:**
- Produces: `pub async fn shared_pool() -> sqlx::PgPool` — starts a shared Postgres 17 container
  once per process, returns a pool connected to a freshly created, migrated database unique to
  this call.
- Consumes: `migrations/postgres/*.sql` (repo-root relative, sorted by filename).

- [ ] **Step 1: Write the failing test**

```rust
// libs/test-support/tests/postgres_shared_pool.rs
use sqlx::Row;

#[tokio::test]
async fn two_calls_share_a_container_but_get_isolated_databases() {
    let pool_a = test_support::postgres::shared_pool().await;
    let pool_b = test_support::postgres::shared_pool().await;

    // Migrations seed the dev tenant in every fresh database.
    let row_a = sqlx::query("SELECT count(*) AS n FROM tenants")
        .fetch_one(&pool_a)
        .await
        .expect("query against pool_a");
    assert!(row_a.get::<i64, _>("n") >= 1);

    // Insert into pool_a only; pool_b must not see it (separate databases).
    sqlx::query(
        "INSERT INTO tenants (id, name) VALUES ('11111111-1111-1111-1111-111111111111', 'only-in-a')",
    )
    .execute(&pool_a)
    .await
    .expect("insert into pool_a");

    let row_b = sqlx::query("SELECT count(*) AS n FROM tenants WHERE name = 'only-in-a'")
        .fetch_one(&pool_b)
        .await
        .expect("query against pool_b");
    assert_eq!(
        row_b.get::<i64, _>("n"),
        0,
        "pool_b must not see rows inserted into pool_a's database"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p test-support --test postgres_shared_pool`
Expected: FAIL with "could not find `postgres` in `test_support`" or "function not found" (module
is empty from Task 1).

- [ ] **Step 3: Write the implementation**

```rust
// libs/test-support/src/postgres.rs
use sqlx::PgPool;
use std::path::Path;
use testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tokio::sync::OnceCell;

static CONTAINER: OnceCell<ContainerAsync<Postgres>> = OnceCell::const_new();

async fn admin_url() -> String {
    let container = CONTAINER
        .get_or_init(|| async {
            Postgres::default()
                .with_tag("17")
                .start()
                .await
                .expect("postgres container started")
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
    sqlx::query(&format!(r#"CREATE DATABASE "{db_name}""#))
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
        sqlx::raw_sql(&sql)
            .execute(pool)
            .await
            .expect("migration applied");
    }
}
```

- [ ] **Step 4: Add dev-dependencies for the test**

```toml
# libs/test-support/Cargo.toml, appended
[dev-dependencies]
tokio = { version = "1", features = ["full"] }
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p test-support --test postgres_shared_pool`
Expected: PASS (requires Docker running locally).

- [ ] **Step 6: Commit**

```bash
git add libs/test-support
git commit -m "feat(test-support): implement shared Postgres container with per-test databases"
```

---

## Task 3: Implement `test_support::clickhouse::shared_client`

**Files:**
- Create: `libs/test-support/src/clickhouse.rs`
- Test: `libs/test-support/tests/clickhouse_shared_client.rs`

**Interfaces:**
- Produces: `pub async fn shared_client() -> clickhouse::Client` — starts a shared ClickHouse
  container once per process, migrates the fixed `observable` database on first call only,
  returns a client scoped to it on every call.
- Consumes: `migrations/clickhouse/*.sql` (repo-root relative, sorted, `;`-split statements).

**Note:** Unlike Postgres, this does **not** create a fresh database per call — production SQL in
`services/query-api/src/sql_templates.rs` (and others) hardcodes `observable.<table>` directly in
query strings, so every caller must share that one database. Isolation between tests comes from
each test using its own random tenant ID (already the established pattern in the test files being
migrated), not from a fresh database.

- [ ] **Step 1: Write the failing test**

```rust
// libs/test-support/tests/clickhouse_shared_client.rs
use uuid::Uuid;

#[tokio::test]
async fn shared_client_is_scoped_to_observable_and_reusable() {
    let ch_a = test_support::clickhouse::shared_client().await;
    let ch_b = test_support::clickhouse::shared_client().await;

    // Migrations must have created the `metric_series` table; a trivial typed
    // query against it (scoped by a random tenant_id) must succeed on both
    // handles, proving they're both connected to the same migrated database.
    let tenant = Uuid::new_v4();
    let count: u64 = ch_a
        .query("SELECT count() FROM metric_series WHERE tenant_id = ?")
        .bind(tenant)
        .fetch_one()
        .await
        .expect("query via ch_a succeeds");
    assert_eq!(count, 0);

    let count_b: u64 = ch_b
        .query("SELECT count() FROM metric_series WHERE tenant_id = ?")
        .bind(tenant)
        .fetch_one()
        .await
        .expect("query via ch_b succeeds");
    assert_eq!(count_b, 0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p test-support --test clickhouse_shared_client`
Expected: FAIL with "could not find `clickhouse` in `test_support`" (module empty from Task 1).

- [ ] **Step 3: Write the implementation**

```rust
// libs/test-support/src/clickhouse.rs
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p test-support --test clickhouse_shared_client`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add libs/test-support
git commit -m "feat(test-support): implement shared ClickHouse container with one-time migration"
```

---

## Task 4: Wire `test-support` into query-api; migrate `postgres_tenants_integration.rs` (template)

**Files:**
- Modify: `services/query-api/Cargo.toml` (`[dev-dependencies]`)
- Create: `services/query-api/tests/it.rs`
- Create: `services/query-api/tests/it/postgres_tenants_integration.rs` (moved + edited from
  `services/query-api/tests/postgres_tenants_integration.rs`)
- Delete: `services/query-api/tests/postgres_tenants_integration.rs`

**Interfaces:**
- Consumes: `test_support::postgres::shared_pool() -> sqlx::PgPool` (Task 2).
- Produces: the `tests/it.rs` + `tests/it/` structure later tasks add `mod` lines and files to.

- [ ] **Step 1: Add the dev-dependency**

In `services/query-api/Cargo.toml`'s `[dev-dependencies]`, add:

```toml
test-support = { path = "../../libs/test-support" }
```

- [ ] **Step 2: Create the consolidated test binary entry point**

```rust
// services/query-api/tests/it.rs
mod postgres_tenants_integration;
```

- [ ] **Step 3: Move and edit the file**

Move `services/query-api/tests/postgres_tenants_integration.rs` to
`services/query-api/tests/it/postgres_tenants_integration.rs`. In the moved file:

Delete lines 19-21 (now-unused imports) and lines 25-63 (`apply_migrations` and `start_pool`):

```rust
// Remove these three import lines:
use std::path::Path;
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;

// Remove this entire block (the `// ── Postgres helpers ──` comment through
// the end of `start_pool`'s closing brace):
// ── Postgres helpers ─────────────────────────────────────────────────────────

async fn apply_migrations(pool: &PgPool) {
    ...
}

async fn start_pool() -> (PgPool, testcontainers::ContainerAsync<Postgres>) {
    ...
}
```

Replace all 6 call sites of `let (pool, _container) = start_pool().await;` with:

```rust
let pool = test_support::postgres::shared_pool().await;
```

- [ ] **Step 4: Run the moved test**

Run: `cargo test -p query-api --test it`
Expected: PASS (all 6 tests from this file run as part of the `it` binary).

- [ ] **Step 5: Commit**

```bash
git add services/query-api/Cargo.toml services/query-api/tests
git commit -m "test(query-api): consolidate tests into tests/it.rs, migrate tenants tests to shared pool"
```

---

## Task 5: Migrate the seven remaining pure-Postgres files

Each of these seven files has the identical shape as Task 4's template: an `apply_migrations`
function, a `start_pool` function (signature varies cosmetically — single-line vs multi-line
return type — logic is identical), three now-unused imports (`std::path::Path`,
`testcontainers::{ImageExt, runners::AsyncRunner}`, `testcontainers_modules::postgres::Postgres`),
and N call sites of the shape `let (pool, _container) = start_pool().await;` or
`let (pool, _c) = start_pool().await;`.

**Files (move each from `tests/<name>.rs` to `tests/it/<name>.rs`, add a `mod <name>;` line to
`tests/it.rs`):**
- `postgres_alerts_integration.rs` — delete lines 1-37 down through `start_pool`'s closing brace
  (keep the `use query_api::alerts::{...}` import at the top); replace all 10 call sites of
  `let (pool, _container) = start_pool().await;`.
- `postgres_change_events_integration.rs` — delete lines 4-36 (helpers + the three unused
  imports, keep `use chrono::Utc;` and `use query_api::change_events::{...}`); replace all 6 call
  sites of `let (pool, _container) = start_pool().await;`.
- `postgres_dashboard_rebac_integration.rs` — delete lines 9-44 (helpers + unused imports, keep
  the `use query_api::dashboards::{...}` import); replace all 7 call sites of
  `let (pool, _container) = start_pool().await;`.
- `postgres_dashboards_integration.rs` — delete lines 6-49 (helpers + unused imports, keep the
  `use query_api::dashboards::{...}` import); replace all 8 call sites of
  `let (pool, _container) = start_pool().await;`.
- `postgres_mcp_tools_integration.rs` — delete lines 5-49 (helpers + unused imports, keep the
  `use query_api::mcp_tools::{...}` import); replace all 12 call sites of
  `let (pool, _c) = start_pool().await;` (note: this file's variable is `_c`, not `_container`).
- `postgres_schemas_integration.rs` — delete lines 5-48 (helpers + unused imports, keep the
  `use query_api::schemas::{...}` import); replace all 11 call sites of
  `let (pool, _container) = start_pool().await;`.
- `postgres_slos_integration.rs` — delete lines 2-46 (helpers + unused imports, keep the
  `use query_api::slos::list_slos;` import); replace the single call site of
  `let (pool, _container) = start_pool().await;`.

In every case, the replacement is:

```rust
let pool = test_support::postgres::shared_pool().await;
```

- [ ] **Step 1: Move and edit `postgres_alerts_integration.rs`** (per the rule above), then add
  `mod postgres_alerts_integration;` to `tests/it.rs`.

- [ ] **Step 2: Run `cargo test -p query-api --test it`** — expect all alerts tests PASS, no
  unused-import warnings for this file.

- [ ] **Step 3: Move and edit `postgres_change_events_integration.rs`**, add its `mod` line.

- [ ] **Step 4: Run `cargo test -p query-api --test it`** — expect PASS.

- [ ] **Step 5: Move and edit `postgres_dashboard_rebac_integration.rs`**, add its `mod` line.

- [ ] **Step 6: Run `cargo test -p query-api --test it`** — expect PASS.

- [ ] **Step 7: Move and edit `postgres_dashboards_integration.rs`**, add its `mod` line.

- [ ] **Step 8: Run `cargo test -p query-api --test it`** — expect PASS.

- [ ] **Step 9: Move and edit `postgres_mcp_tools_integration.rs`** (remember: `_c`, not
  `_container`), add its `mod` line.

- [ ] **Step 10: Run `cargo test -p query-api --test it`** — expect PASS.

- [ ] **Step 11: Move and edit `postgres_schemas_integration.rs`**, add its `mod` line.

- [ ] **Step 12: Run `cargo test -p query-api --test it`** — expect PASS.

- [ ] **Step 13: Move and edit `postgres_slos_integration.rs`**, add its `mod` line.

- [ ] **Step 14: Run `cargo test -p query-api --test it`** — expect PASS, full `it` binary now
  covers all 8 pure-Postgres files (7 + Task 4's template).

- [ ] **Step 15: Commit**

```bash
git add services/query-api/tests
git commit -m "test(query-api): migrate remaining pure-Postgres integration tests to shared pool"
```

---

## Task 6: Migrate the three remaining pure-Postgres files with non-uniform helper names

These three use the same logic but different function/variable names (`start_postgres` instead
of `start_pool`, `apply_pg_migrations` instead of `apply_migrations`) and one uses `PgPoolOptions`
instead of bare `PgPool::connect`.

**Files:**
- `api_key_audit_integration.rs` — delete the `start_postgres`/`apply_pg_migrations` functions
  (lines 37-76) and the three unused imports (`std::path::Path` — but keep `sync::Arc` from the
  same `use std::{path::Path, sync::Arc};` line, so narrow that import to
  `use std::sync::Arc;` — plus `testcontainers::{ImageExt, runners::AsyncRunner}` and
  `testcontainers_modules::postgres::Postgres`); replace both call sites of
  `let (pool, _container) = start_postgres().await;` with
  `let pool = test_support::postgres::shared_pool().await;`. Keep `sqlx::postgres::{PgPool,
  PgPoolOptions}` — `PgPoolOptions` may become unused; if `cargo check` flags it, remove just that
  half of the import, keeping `PgPool`.
- `session_auth_integration.rs` — same shape: delete `start_postgres`/`apply_pg_migrations`,
  narrow `use std::{path::Path, sync::Arc};` to `use std::sync::Arc;`, remove the two
  testcontainers imports, replace both call sites of
  `let (pool, _container) = start_postgres().await;` (note: this file's call sites use
  `_container`, confirm via `cargo build --tests` which will error on any mismatched name if
  guessed wrong) with `let pool = test_support::postgres::shared_pool().await;`.
- `nlq_shorthand_integration.rs` — delete `start_postgres`/`apply_pg_migrations`, remove the two
  testcontainers imports, replace the single call site of
  `let (pool, _container) = start_postgres().await;` with
  `let pool = test_support::postgres::shared_pool().await;`.

- [ ] **Step 1: Move and edit `api_key_audit_integration.rs`** into `tests/it/`, add its `mod`
  line to `tests/it.rs`.

- [ ] **Step 2: Run `cargo test -p query-api --test it`** — expect PASS.

- [ ] **Step 3: Move and edit `session_auth_integration.rs`** into `tests/it/`, add its `mod`
  line.

- [ ] **Step 4: Run `cargo test -p query-api --test it`** — expect PASS.

- [ ] **Step 5: Move and edit `nlq_shorthand_integration.rs`** into `tests/it/`, add its `mod`
  line.

- [ ] **Step 6: Run `cargo test -p query-api --test it`** — expect PASS. All 11 Postgres-only
  files are now consolidated and sharing one container.

- [ ] **Step 7: Commit**

```bash
git add services/query-api/tests
git commit -m "test(query-api): migrate remaining Postgres-only integration tests to shared pool"
```

---

## Task 7: Migrate `clickhouse_integration.rs` (pure ClickHouse, already tenant-isolated)

**Files:**
- Move: `services/query-api/tests/clickhouse_integration.rs` →
  `services/query-api/tests/it/clickhouse_integration.rs`
- Modify: `services/query-api/tests/it.rs` (add `mod clickhouse_integration;`)

This file's 20 tests each start their own ClickHouse container **inline** (no shared helper
function — `let container = ClickHouse::default()...start().await...` directly inside each test
body), followed by `let ch = apply_migrations(&base_url, "default", "test").await;`. Every test
already uses locally-scoped `Uuid::new_v4()` tenant IDs (confirmed: no fixed tenant constant is
reused across tests in this file) — no tenant-randomization fix is needed here, only the
mechanical swap.

**Per test function, replace this 3-statement block:**

```rust
let container = ClickHouse::default()
    .with_tag("25.3")
    .with_env_var("CLICKHOUSE_USER", "default")
    .with_env_var("CLICKHOUSE_PASSWORD", "test")
    .start()
    .await
    .expect("clickhouse container started");

let port = container.get_host_port_ipv4(8123).await.unwrap();
let base_url = format!("http://127.0.0.1:{port}");
let ch = apply_migrations(&base_url, "default", "test").await;
```

**with:**

```rust
let ch = test_support::clickhouse::shared_client().await;
```

Then delete the file-level `apply_migrations` function (lines 13-58) and the two now-unused
imports (`std::path::Path`, `testcontainers::{ImageExt, runners::AsyncRunner}`,
`testcontainers_modules::clickhouse::ClickHouse`).

- [ ] **Step 1: Move the file**, apply the per-test block replacement to all 20 tests (search for
  the literal string `.expect("clickhouse container started");` to locate each occurrence — every
  one is followed by the same two lines and the `apply_migrations` call).

- [ ] **Step 2: Delete the `apply_migrations` function and the three now-unused imports.**

- [ ] **Step 3: Add `mod clickhouse_integration;` to `tests/it.rs`.**

- [ ] **Step 4: Run test to verify**

Run: `cargo test -p query-api --test it`
Expected: PASS, all 20 tests in this file still pass.

- [ ] **Step 5: Commit**

```bash
git add services/query-api/tests
git commit -m "test(query-api): migrate clickhouse_integration.rs to shared ClickHouse client"
```

---

## Task 8: Migrate `clickhouse_fetch_label_keys_integration.rs` (fix: fixed tenant constant)

**Files:**
- Move: `services/query-api/tests/clickhouse_fetch_label_keys_integration.rs` →
  `services/query-api/tests/it/clickhouse_fetch_label_keys_integration.rs`
- Modify: `services/query-api/tests/it.rs`

This file's `const TENANT: Uuid = Uuid::from_u128(0xCCCC_..._0003);` is reused by all 3 tests.
Under a shared ClickHouse database, this must become a per-test local variable so each test's
inserted `metric_series` rows don't accumulate under the same tenant.

- [ ] **Step 1: Move the file.**

- [ ] **Step 2: Delete the file-level constant**

```rust
// Delete this line:
const TENANT: Uuid = Uuid::from_u128(0xCCCC_0000_0000_0000_0000_0000_0000_0003);
```

- [ ] **Step 3: Add a per-test local in each of the 3 test functions**, immediately after the
  `let (ch, _container) = start_ch().await;` line is replaced (next step) — i.e. each test gets:

```rust
let tenant = Uuid::new_v4();
```

and every use of `TENANT` later in that same test function becomes `tenant` (3 occurrences per
test: `fetch_label_keys_returns_native_columns`, `fetch_label_keys_no_attributes_returns_only_native`,
`fetch_label_keys_deduplicates_native_columns` — each calls `make_series_with_attrs(TENANT, ...)`
and `fetch_label_keys(&ch, TENANT, 20)`, both becoming `tenant`).

- [ ] **Step 4: Replace all 3 call sites of `let (ch, _container) = start_ch().await;`**

```rust
let ch = test_support::clickhouse::shared_client().await;
```

- [ ] **Step 5: Delete the `start_ch` and `apply_ch_migrations` functions (lines 23-83) and the
  three now-unused imports** (`std::path::Path`,
  `testcontainers::{ImageExt, runners::AsyncRunner}`,
  `testcontainers_modules::clickhouse::ClickHouse`).

- [ ] **Step 6: Add `mod clickhouse_fetch_label_keys_integration;` to `tests/it.rs`.**

- [ ] **Step 7: Run test to verify**

Run: `cargo test -p query-api --test it`
Expected: PASS, all 3 tests pass with per-test random tenants.

- [ ] **Step 8: Commit**

```bash
git add services/query-api/tests
git commit -m "test(query-api): migrate clickhouse_fetch_label_keys_integration.rs, randomize tenant per test"
```

---

## Task 9: Migrate `clickhouse_mcp_query_integration.rs` (fix: fixed TENANT_A/TENANT_B constants)

**Files:**
- Move: `services/query-api/tests/clickhouse_mcp_query_integration.rs` →
  `services/query-api/tests/it/clickhouse_mcp_query_integration.rs`
- Modify: `services/query-api/tests/it.rs`

This file's `const TENANT_A` / `const TENANT_B` are reused across 9 test functions. Each is passed
directly as a function argument to `execute_mcp_query(&db, &ch, TENANT_A, &ir)` (a plain Rust
function call, not an HTTP path) — there's no auth/header constraint forcing a fixed value, so
both become per-test local `Uuid::new_v4()` calls.

- [ ] **Step 1: Move the file.**

- [ ] **Step 2: Delete the two file-level constants**

```rust
// Delete these two lines:
const TENANT_A: Uuid = Uuid::from_u128(0xAAAA_0000_0000_0000_0000_0000_0000_0001);
const TENANT_B: Uuid = Uuid::from_u128(0xBBBB_0000_0000_0000_0000_0000_0000_0002);
```

- [ ] **Step 3: In every one of the 9 test functions that reference `TENANT_A` and/or
  `TENANT_B`, add local declarations at the top of the test body** (after the pool/client setup
  from Step 5 below) and replace every in-body use of `TENANT_A` → `tenant_a`, `TENANT_B` →
  `tenant_b`:

```rust
let tenant_a = Uuid::new_v4();
let tenant_b = Uuid::new_v4(); // only in tests that also reference TENANT_B
```

(Confirmed call sites needing this treatment, from the test bodies: lines 237-247, 288-299,
312-319, 346-353, 369-377, 399-420 (uses both), 436, 457, 479-486, 529-564 — use
`cargo build --tests -p query-api` after this edit; any remaining bare `TENANT_A`/`TENANT_B`
reference is now a compile error (undefined name), which is the correctness signal that a
reference was missed.)

- [ ] **Step 4: Replace all 8 call sites of `let (db, _pg) = start_pg().await;`**

```rust
let db = test_support::postgres::shared_pool().await;
```

- [ ] **Step 5: Replace all 8 call sites of `let (ch, _ch_container) = start_ch().await;` and the
  2 call sites of `let (_ch, _ch_container) = start_ch().await;`**

```rust
let ch = test_support::clickhouse::shared_client().await;
// or, where the original was `_ch` (the two tests at lines 427-428 and 449-450 that don't
// actually use the client):
let _ch = test_support::clickhouse::shared_client().await;
```

- [ ] **Step 6: Delete `start_pg`, `apply_pg_migrations`, `start_ch`, `apply_ch_migrations`
  (lines 28-100ish — confirm exact span via the function boundaries) and the now-unused imports**
  (`std::path::Path`, `testcontainers::{ImageExt, runners::AsyncRunner}`,
  `testcontainers_modules::{clickhouse::ClickHouse, postgres::Postgres}`).

- [ ] **Step 7: Add `mod clickhouse_mcp_query_integration;` to `tests/it.rs`.**

- [ ] **Step 8: Run test to verify**

Run: `cargo test -p query-api --test it`
Expected: PASS, all 9 tests pass with per-test random tenants. Pay particular attention to the
tenant-isolation test (around line 393-420, asserting `tenant_b`'s query sees none of
`tenant_a`'s data) — this assertion's correctness is unaffected by randomizing the constants,
since it already compared two different tenant values, just previously fixed ones.

- [ ] **Step 9: Commit**

```bash
git add services/query-api/tests
git commit -m "test(query-api): migrate clickhouse_mcp_query_integration.rs, randomize tenants per test"
```

---

## Task 10: Migrate `http_api_integration.rs`'s Postgres half only (ClickHouse stays exempt)

**Files:**
- Move: `services/query-api/tests/http_api_integration.rs` →
  `services/query-api/tests/it/http_api_integration.rs`
- Modify: `services/query-api/tests/it.rs`

Per the Global Constraints, `start_clickhouse()` and every one of its call sites are **left
untouched** in this task — only the Postgres half is migrated.

- [ ] **Step 1: Move the file.**

- [ ] **Step 2: Replace all 11 call sites of `let (db, _pg) = start_postgres().await;`**

```rust
let db = test_support::postgres::shared_pool().await;
```

- [ ] **Step 3: Replace all 16 call sites of `let (pg, _pg_container) = start_postgres().await;`**

```rust
let pg = test_support::postgres::shared_pool().await;
```

- [ ] **Step 4: Replace the 1 call site of `let (db, _pg_container) = start_postgres().await;`**
  (line 1336)

```rust
let db = test_support::postgres::shared_pool().await;
```

- [ ] **Step 5: Delete the `start_postgres` and `apply_pg_migrations` functions** (locate via
  `async fn start_postgres()` at line 51 and `async fn apply_pg_migrations` immediately following
  it — delete through that function's closing brace). **Do not delete `start_clickhouse` or
  `ChClient`-related code.**

- [ ] **Step 6: Remove only the now-unused-for-Postgres imports** — `std::path::Path` is still
  needed if `start_clickhouse`'s own migration helper (if it has one) uses it; check before
  removing. `testcontainers_modules::postgres::Postgres` becomes unused (ClickHouse's import is
  separate); remove only the `postgres::Postgres` half if the import line is combined, e.g.
  `testcontainers_modules::{clickhouse::ClickHouse, postgres::Postgres}` →
  `testcontainers_modules::clickhouse::ClickHouse`. Keep
  `testcontainers::{ImageExt, runners::AsyncRunner}` since `start_clickhouse` still needs it.

- [ ] **Step 7: Add `mod http_api_integration;` to `tests/it.rs`.**

- [ ] **Step 8: Run test to verify**

Run: `cargo test -p query-api --test it`
Expected: PASS. All Postgres-only tests in this file now share the container; the ~10
ClickHouse-touching tests still start their own container exactly as before (unchanged behavior,
confirmed by these tests passing identically to their pre-migration state).

- [ ] **Step 9: Commit**

```bash
git add services/query-api/tests
git commit -m "test(query-api): migrate http_api_integration.rs Postgres calls to shared pool, ClickHouse stays per-test"
```

---

## Task 11: Final cleanup — dependency pruning, full suite run, docs note

**Files:**
- Modify: `services/query-api/Cargo.toml`
- Modify: `docs/agent-context.md`

**Files now fully migrated and consolidated into `tests/it.rs`:** all 13 files from Tasks
4-10 (`postgres_tenants_integration`, the 7 from Task 5, the 3 from Task 6,
`clickhouse_integration`, `clickhouse_fetch_label_keys_integration`,
`clickhouse_mcp_query_integration`, `http_api_integration` — 14 total). `nlq_sql_safety_integration.rs`
(no Testcontainers usage) remains a standalone test binary, untouched.

- [ ] **Step 1: Check whether `query-api`'s direct `testcontainers`/`testcontainers-modules`
  dev-dependencies are still needed**

Run: `cargo build --tests -p query-api`

If this succeeds with no "unresolved import" errors after removing the two lines from
`[dev-dependencies]` in `services/query-api/Cargo.toml`:

```toml
testcontainers         = "0.27.3"
testcontainers-modules = { version = "0.15.0", features = ["clickhouse", "postgres"] }
```

remove them (the consolidated `tests/it.rs` binary's `http_api_integration` module still uses
`testcontainers`/`testcontainers-modules` directly for its exempted ClickHouse path, so this will
likely **fail** and the two lines must stay — confirm either way and keep whichever the build
requires).

- [ ] **Step 2: Run the full crate test suite**

Run: `cargo test -p query-api`
Expected: PASS. Confirm via the test runner's summary line that the `it` binary now runs all 14
consolidated files' tests together, and `nlq_sql_safety_integration` runs as its own binary.

- [ ] **Step 3: Confirm container-count reduction**

Run: `cargo test -p query-api --test it -- --test-threads=1 2>&1 | grep -c "container started"`
(or equivalent — count how many times a container-start log line appears). Expected: 2 (one
Postgres start, one ClickHouse start for the shared singletons — plus however many the exempted
`http_api_integration.rs` ClickHouse tests still start individually, which is unchanged from
before this plan).

- [ ] **Step 4: Run with parallelism to confirm no isolation regressions**

Run: `cargo test -p query-api --test it -- --test-threads=8` three times in a row.
Expected: PASS all three runs, no flakes (catches any isolation gap the per-test-database or
per-test-random-tenant design might have missed).

- [ ] **Step 5: `cargo fmt --all --check`**

Expected: no diff.

- [ ] **Step 6: Update `docs/agent-context.md`**

Add a dated bullet near the existing Testcontainers-related entries:

```markdown
- **2026-06-21**: `query-api`'s Testcontainers integration tests now share one Postgres and one
  ClickHouse container per `cargo test` run (`libs/test-support`'s `postgres::shared_pool()` /
  `clickhouse::shared_client()`), consolidated into a single `tests/it.rs` binary, instead of
  spinning up a fresh container per test function. Postgres isolation is a fresh migrated
  database per test; ClickHouse isolation is per-test random tenant IDs against one shared
  `observable` database (production SQL hardcodes that name). `http_api_integration.rs`'s
  ClickHouse-touching tests are deliberately exempt — they key off a fixed `DEV_TENANT_ID` for
  the auth header path and the production histogram endpoints have no per-test query
  discriminator finer than tenant + time window, so sharing would require sacrificing test
  parallelism. See `docs/superpowers/specs/2026-06-21-testcontainers-shared-container-design.md`
  for the full design and `archived/plans/2026-06-21-testcontainers-shared-container-pilot.md`
  for this pilot's implementation. Follow-up slices (same pattern, one per service) remain for
  `admin-service`, `alert-evaluator`, `auth-service`, `storage-writer`, `ingest-gateway`, and
  `stream-processor` (the last needs a new `test_support::redpanda` module, not yet built).
```

- [ ] **Step 7: Move this plan to `archived/plans/`**

```bash
mv docs/superpowers/plans/2026-06-21-testcontainers-shared-container-pilot.md archived/plans/
```

- [ ] **Step 8: Commit**

```bash
git add services/query-api/Cargo.toml docs/agent-context.md archived/plans docs/superpowers/plans
git commit -m "test(query-api): finish shared-Testcontainers pilot cleanup and doc the pattern"
```

## Verification (full pilot)

- `cargo build -p test-support` and `cargo build -p query-api` clean.
- `cargo test -p test-support` passing (both `shared_pool` and `shared_client` proving tests).
- `cargo test -p query-api` full suite passing, including 3x repeated `--test-threads=8` runs of
  the `it` binary with no flakes.
- `cargo fmt --all --check` clean.
- Container-start count for `cargo test -p query-api --test it` is 2 (shared Postgres + shared
  ClickHouse singletons) plus the unchanged per-test count from the exempted
  `http_api_integration.rs` ClickHouse tests.

## Rollback

Each task is its own commit. Reverting any single task's commit only affects that file's
container-sharing — `libs/test-support` is purely additive and doesn't change production code, so
a partial rollback (e.g. reverting Task 9 but keeping Tasks 4-8) leaves the crate in a working,
if less-optimized, state.
