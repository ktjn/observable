# Shared Testcontainers Across Integration Tests — Design

**Date:** 2026-06-21
**Status:** Approved, ready for implementation planning

## Problem

Every Rust integration test file under `services/*/tests/` that needs a real
Postgres, ClickHouse, or Redpanda dependency spins up its **own** Testcontainers
container per test function via a locally-duplicated `start_pool()`-style
helper. `services/query-api/tests/http_api_integration.rs` alone starts 27
separate Postgres containers — one per `#[tokio::test]`. Across the workspace,
30 test files duplicate this container-bootstrap boilerplate.

This makes `cargo test -p <service>` slow locally (each container pull/start
is seconds, multiplied by every test function) and is pure duplication: the
same `Postgres::default().with_tag("17")` setup, migration-apply loop, and
connection-string plumbing is copy-pasted per file.

**Out of scope:** CI. These tests aren't part of the GitHub Actions gate today
(`.github/workflows/build.yml` runs `cargo test --workspace --lib --bins`,
which excludes `tests/`). This change is a local/agent dev-loop speedup only;
it has no effect on CI minutes.

## Goals

- One Testcontainers container per (service crate × container type), reused
  across every test function that needs it, instead of one per test function.
- Each test keeps full isolation: its own Postgres/ClickHouse database, or its
  own Kafka topic for Redpanda — sharing the container must not introduce
  cross-test data leakage or ordering dependencies.
- Tests keep running in parallel (no serialization/mutex around test bodies).
- No CI changes required.

## Architecture

### 1. New shared crate: `libs/test-support`

A new workspace member, dev-dependency only (added under `[dev-dependencies]`
in each service `Cargo.toml`, not `[dependencies]`), following the existing
`libs/domain` / `libs/observable-auth` pattern.

Three modules, one per container type:

- **`postgres`**: `shared_pool() -> PgPool`
  - Internally holds a `tokio::sync::OnceCell<ContainerAsync<Postgres>>`,
    started once per test binary process via `Postgres::default().with_tag("17")`
    (same image/tag every call site uses today — `get_or_init` guarantees a
    single start even under concurrent test functions).
  - On every call: connects as the container's superuser, runs
    `CREATE DATABASE test_<uuid>`, applies `migrations/postgres/*.sql` against
    that new database (reusing the existing sorted-`read_dir` + `raw_sql` loop
    that already lives in each test file today), and returns a `PgPool`
    connected to `test_<uuid>`.
- **`clickhouse`**: `shared_client() -> Client`
  - Same singleton-container pattern via `testcontainers_modules::clickhouse::ClickHouse`.
  - Per call: `CREATE DATABASE IF NOT EXISTS test_<uuid>` against the shared
    container, applies `migrations/clickhouse/*.sql` (statement-split on `;`,
    matching the existing loop in `clickhouse_integration.rs`), returns a
    `Client` scoped to `test_<uuid>`.
- **`redpanda`**: `shared_brokers() -> String`
  - Same singleton-container pattern via the existing `GenericImage`-based
    Redpanda setup in `redpanda_integration.rs` (advertised-address handling
    via `pick_free_port`, `wait_for_kafka_ready`).
  - Returns the broker address only. Callers are responsible for creating a
    uniquely-named topic per test (already the established pattern) — Kafka
    isolation is topic-based, there's no per-test "database" concept.

Each module's container handle is held for the lifetime of the test binary
process; Testcontainers' Ryuk reaper (already in effect today) cleans it up on
process exit, same as the current per-test containers.

Per-test databases/topics are **not** explicitly dropped after each test —
the whole container is torn down at process exit anyway, so cleanup is a
no-op win, not a correctness requirement.

### 2. Per-crate consolidation into one test binary

Cargo compiles each file directly under `tests/` into its own test binary
(its own OS process). An in-process `OnceCell` singleton can only be shared
across test functions *within one process* — so for the container to be
reused across multiple files in the same crate, those files must become
modules of one binary, not separate binaries.

Per service, for every test file that uses Testcontainers:

1. Move `tests/foo_integration.rs` → `tests/it/foo_integration.rs` (a plain
   `mod` rename, no content changes beyond import paths if any relative paths
   were assumed).
2. Create (or extend) `tests/it.rs` as the single entry point:
   ```rust
   mod foo_integration;
   mod bar_integration;
   // ... one `mod` per moved file
   ```
3. Delete each file's local `start_pool()` / `apply_migrations()` /
   container-bootstrap helper; replace call sites with
   `test_support::postgres::shared_pool().await` (or the `clickhouse`/
   `redpanda` equivalent).

Files with no container dependency (e.g. `http_readyz_integration.rs`) are
left as standalone test binaries — no need to fold them in.

### 3. Rollout order

One slice per service, sequential, each independently shippable:

1. **Pilot: `libs/test-support` + `query-api`.** Biggest payoff (~15 files,
   both Postgres and ClickHouse usage, including the 27-container
   `http_api_integration.rs`). Validates the pattern end-to-end before
   touching other services.
2. Confirm via `cargo test -p query-api`: container-start count drops to one
   per container type for the whole crate's test run, tests still pass, and
   tests still run in parallel without cross-test interference (e.g. run
   `cargo test -p query-api -- --test-threads=8` a few times to catch any
   isolation gaps the per-test-database design might have missed).
3. Repeat the same mechanical pattern for `admin-service`, `alert-evaluator`,
   `auth-service`, `storage-writer`, `ingest-gateway` — each its own slice.
4. `stream-processor` last (Redpanda only, lowest priority — currently just 1
   container-using test).

## Testing

This change touches test infrastructure, not application behavior — the
"test" for each slice is that the existing integration test suite for that
crate still passes after the migration, with no test logic changes (only the
container-acquisition call site changes). No new test cases are needed; no
existing test's assertions change.

## Rollback

Each per-service slice is an independent commit/PR. If a service's
consolidation surfaces an isolation bug (e.g. a test that implicitly relied on
a previous test's leftover container state), that slice can be reverted on
its own without affecting already-migrated or not-yet-migrated services —
`libs/test-support` itself is additive and doesn't change any production code
path.
