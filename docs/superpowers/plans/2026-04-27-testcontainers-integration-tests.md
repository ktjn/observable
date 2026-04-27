# Testcontainers Integration Tests Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add isolated real-dependency integration tests for backend services using Testcontainers, while keeping Docker Compose and kind as full-stack gates.

**Architecture:** Introduce Testcontainers as a service-level harness first, not a platform-wide replacement for existing smoke tests. Start with PostgreSQL migration/repository coverage, then add ClickHouse repository coverage, then Redpanda queue-boundary coverage, promoting shared helpers only after duplication appears in two service crates.

**Tech Stack:** Rust, Tokio, Testcontainers Rust ecosystem, PostgreSQL, ClickHouse, Redpanda/Kafka-compatible broker, `sqlx`, `clickhouse`, `rdkafka`, Docker or compatible container runtime.

---

## File Structure

- Create: `services/auth-service/tests/postgres_integration.rs` for API-key migration and validation repository tests against a real PostgreSQL container.
- Create: `services/query-api/tests/clickhouse_integration.rs` for tenant-filtered query repository tests against a real ClickHouse container.
- Create: `services/stream-processor/tests/redpanda_integration.rs` for raw telemetry consume/produce boundary tests against a real Redpanda container.
- Modify: `services/auth-service/Cargo.toml`, `services/query-api/Cargo.toml`, and `services/stream-processor/Cargo.toml` to add dev-dependencies, using the latest stable crate versions checked at implementation time.
- Modify: `scripts/local-ci.sh` only if the first Testcontainers test command needs a named stage or better diagnostics; preserve all existing mandatory checks.
- Modify: `spec/11-testing.md`, `spec/10-process.md`, `AGENTS.md`, `AGENT.md`, `CLAUDE.md`, and `GEMINI.md` only if implementation discovers policy details that differ from ADR-025.

## Current Coverage And Target Coverage

Current coverage:

- Unit tests cover pure service logic.
- `scripts/local-ci.sh` runs the Docker Compose smoke path for code changes.
- kind tests cover Kubernetes packaging and rollback.
- No standard isolated service-level container harness exists for PostgreSQL, ClickHouse, or Redpanda integration behavior.

Target coverage after this plan:

- Backend changes touching PostgreSQL, ClickHouse, Redpanda/Kafka-compatible brokers, object storage, OpenFGA, or similar real containerized dependencies have a mandatory Testcontainers verification path where applicable.
- Docker Compose smoke tests continue to prove the end-to-end ingest/query platform path.
- kind tests continue to prove Kubernetes chart, rollout, and rollback behavior.

---

### Task 1: Add PostgreSQL Testcontainers Coverage For Auth-Service

**Files:**
- Modify: `services/auth-service/Cargo.toml`
- Create: `services/auth-service/tests/postgres_integration.rs`
- Test: `cargo test -p auth-service --test postgres_integration -- --nocapture`

- [ ] **Step 1: Check dependency versions**

Run:

```bash
cargo search testcontainers
cargo search testcontainers-modules
```

Expected: record the latest stable Rust Testcontainers crate versions in the PR body before editing `Cargo.toml`.

- [ ] **Step 2: Add dev-dependencies**

Add dev-dependencies to `services/auth-service/Cargo.toml` for the Testcontainers crates and any required async/runtime support, using the current stable versions from Step 1.

- [ ] **Step 3: Write the failing integration test**

Create `services/auth-service/tests/postgres_integration.rs` with tests that:

```rust
#[tokio::test]
async fn postgres_container_applies_api_key_migrations_and_validates_seed_key() {
    // Start an isolated PostgreSQL container.
    // Apply migrations/postgres/*.sql in lexical order.
    // Insert or verify a seeded member API key row.
    // Call the auth validation repository path against the real PgPool.
    // Assert the expected tenant_id and role are returned.
}
```

The first version may fail to compile if repository functions are not exported cleanly. That failure identifies the minimum service seam required by the next step.

- [ ] **Step 4: Run the test and capture the expected failure**

Run:

```bash
cargo test -p auth-service --test postgres_integration -- --nocapture
```

Expected: FAIL because either the helper seam is missing or migrations are not yet wired into the Testcontainers fixture.

- [ ] **Step 5: Add the minimum auth-service test seam**

Expose only the repository or migration helper needed by the test. Do not move HTTP handlers or unrelated auth code.

- [ ] **Step 6: Re-run the focused test**

Run:

```bash
cargo test -p auth-service --test postgres_integration -- --nocapture
```

Expected: PASS, proving PostgreSQL migrations and API-key validation work against a real database container.

- [ ] **Step 7: Commit**

Run:

```bash
git add services/auth-service/Cargo.toml services/auth-service/tests/postgres_integration.rs
git commit -m "Add auth-service PostgreSQL integration test"
```

Expected: commit succeeds on the feature branch after the focused test passes.

---

### Task 2: Add ClickHouse Testcontainers Coverage For Query API

**Files:**
- Modify: `services/query-api/Cargo.toml`
- Create: `services/query-api/tests/clickhouse_integration.rs`
- Test: `cargo test -p query-api --test clickhouse_integration -- --nocapture`

- [ ] **Step 1: Add ClickHouse container fixture**

Create a Testcontainers fixture that starts ClickHouse, applies `migrations/clickhouse/*.sql`, and returns a `clickhouse::Client` pointed at the mapped HTTP port.

- [ ] **Step 2: Write the tenant-filter failing test**

Create a test that inserts two traces with the same shape but different `tenant_id` values, then calls the query-api repository path for tenant A.

Expected assertion:

```rust
assert!(result.spans.iter().all(|span| span.tenant_id == tenant_a));
assert!(!result.spans.iter().any(|span| span.tenant_id == tenant_b));
```

- [ ] **Step 3: Run the test and capture the expected failure**

Run:

```bash
cargo test -p query-api --test clickhouse_integration -- --nocapture
```

Expected: FAIL until the repository test seam and ClickHouse fixture are complete.

- [ ] **Step 4: Add the minimum query-api test seam**

Expose a repository-level function or module path that executes the same ClickHouse query used by the HTTP handler. Do not duplicate query SQL in the test.

- [ ] **Step 5: Re-run the focused test**

Run:

```bash
cargo test -p query-api --test clickhouse_integration -- --nocapture
```

Expected: PASS, proving tenant-filtered ClickHouse reads against a real ClickHouse container.

- [ ] **Step 6: Commit**

Run:

```bash
git add services/query-api/Cargo.toml services/query-api/tests/clickhouse_integration.rs
git commit -m "Add query-api ClickHouse integration test"
```

Expected: commit succeeds after the focused test passes.

---

### Task 3: Add Redpanda Testcontainers Coverage For Queue Boundary

**Files:**
- Modify: `services/stream-processor/Cargo.toml`
- Create: `services/stream-processor/tests/redpanda_integration.rs`
- Test: `cargo test -p stream-processor --test redpanda_integration -- --nocapture`

- [ ] **Step 1: Add Redpanda container fixture**

Create a fixture that starts Redpanda, waits for the Kafka API to accept metadata requests, creates a randomized topic name, and returns broker connection details.

- [ ] **Step 2: Write the failing producer/consumer test**

Create a test that publishes one telemetry envelope to the randomized topic, consumes it through the same consumer setup used by stream-processor code, and asserts the tenant ID and payload bytes are preserved.

- [ ] **Step 3: Run the focused test and capture the expected failure**

Run:

```bash
cargo test -p stream-processor --test redpanda_integration -- --nocapture
```

Expected: FAIL until the queue fixture and consumer seam are complete.

- [ ] **Step 4: Add the minimum stream-processor test seam**

Expose only the producer/consumer boundary code required by the test. Keep normalization and storage writing covered by their existing unit or integration tests.

- [ ] **Step 5: Re-run the focused test**

Run:

```bash
cargo test -p stream-processor --test redpanda_integration -- --nocapture
```

Expected: PASS, proving the service queue boundary works against a real Redpanda container.

- [ ] **Step 6: Commit**

Run:

```bash
git add services/stream-processor/Cargo.toml services/stream-processor/tests/redpanda_integration.rs
git commit -m "Add stream-processor Redpanda integration test"
```

Expected: commit succeeds after the focused test passes.

---

### Task 4: Wire Testcontainers Into The Local Gate Deliberately

**Files:**
- Modify: `scripts/local-ci.sh`
- Test: `bash scripts/local-ci.sh --skip-frontend --skip-docker --skip-smoke`

- [ ] **Step 1: Inspect existing local-ci behavior**

Run:

```bash
bash -n scripts/local-ci.sh
git diff -- scripts/local-ci.sh
```

Expected: syntax is valid and there are no unrelated script edits.

- [ ] **Step 2: Decide whether a new stage is needed**

If `cargo test --workspace --all-targets` already runs the new integration tests reliably, document that no script change is needed. If the tests require a named stage for Docker availability checks or diagnostics, add the smallest stage that runs only the Testcontainers tests and preserves existing skip semantics.

- [ ] **Step 3: Verify the gate path**

Run:

```bash
bash scripts/local-ci.sh --skip-frontend --skip-docker --skip-smoke
```

Expected: PASS when Docker is available for Testcontainers, or a clear documented skip/failure mode if Docker is unavailable. Do not silently skip applicable Testcontainers tests.

- [ ] **Step 4: Commit if the script changed**

Run:

```bash
git add scripts/local-ci.sh
git commit -m "Wire Testcontainers tests into local CI"
```

Expected: commit is needed only if `scripts/local-ci.sh` changed.

---

## Verification Plan

Required for implementation PRs:

```bash
cargo test -p auth-service --test postgres_integration -- --nocapture
cargo test -p query-api --test clickhouse_integration -- --nocapture
cargo test -p stream-processor --test redpanda_integration -- --nocapture
bash scripts/local-ci.sh
```

Documentation-only edits to this plan remain exempt from `bash scripts/local-ci.sh`, but must run:

```bash
git diff --check
```

## ADR/Spec Synchronization

This plan is governed by [ADR-025](../../../spec/adr/ADR-025-testcontainers-integration-tests.md), `spec/11-testing.md §18.8`, and `spec/10-process.md §16.7`. If implementation changes the selected harness, required boundaries, or gate semantics, update the ADR and specs in the same PR.
