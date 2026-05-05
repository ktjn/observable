# P4-S1 Warm Retention Movement Path Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the first warm-retention movement path by exporting aged span rows from hot ClickHouse storage to S3-compatible object storage without changing query results.

**Architecture:** Keep the first Phase 4 slice copy-first and reversible: storage-writer writes deterministic JSONL warm objects to S3-compatible storage and records enough metadata to prove what was exported, but it does not delete hot ClickHouse rows or require query-api to read warm objects yet. This proves object-store connectivity, object naming, serialization, and Testcontainers coverage while preserving current hot-tier query semantics.

**Tech Stack:** Rust, Tokio, ClickHouse, S3-compatible object storage, MinIO for local Compose, Testcontainers, `clickhouse`, and the latest stable Rust S3/object-store crate chosen at implementation time.

---

## File Structure

- Modify: `services/storage-writer/Cargo.toml` to add the selected S3/object-store runtime dependency and the MinIO/S3 Testcontainers dev dependency if needed.
- Create: `services/storage-writer/src/warm_retention.rs` for config parsing, object key construction, span export row serialization, S3 writes, and export-cycle orchestration.
- Modify: `services/storage-writer/src/main.rs` to start the warm-retention worker when `WARM_RETENTION_ENABLED=true`.
- Create: `services/storage-writer/tests/warm_retention_integration.rs` for ClickHouse + S3-compatible object-store Testcontainers coverage.
- Modify: `docker-compose.yml` to add a pinned MinIO service, a setup service that creates the warm-retention bucket, and storage-writer environment variables.
- Modify: `spec/03-storage.md` only if implementation changes the warm-tier duration, object format, or query semantics described by the existing retention tier spec.
- Modify: `spec/12-deployment.md` only if local dependency names, ports, or setup workflow differ from the current Compose/deployment documentation.
- Modify: `docs/agent-context.md` only if implementation discovers a durable gotcha future agents need before starting retention work.

## Slice Contract

Source spec: `spec/03-storage.md §5.3`, `spec/10-process.md §17 Phase 4`, ADR-012, ADR-025.
Phase: 4.
Parent phase item: Add warm retention tiers, compaction, and restore procedures.
Acceptance target: one tenant's aged span rows are exported to a deterministic S3-compatible object key, the object can be read back in an integration test, and existing hot ClickHouse query results are unchanged.
User/operator outcome: operators can prove that data older than the hot window is copied into warm object storage before any destructive retention behavior is introduced.
Files or modules expected to change: `services/storage-writer/`, `docker-compose.yml`, and possibly `spec/03-storage.md`, `spec/12-deployment.md`, and `docs/agent-context.md`.
Out of scope: deleting ClickHouse rows after export, query-api warm-tier federation, cold/archive tiers, backup/restore drills, profiles, logs, metrics, compression, encryption, lifecycle policies, and production cloud credentials.
Verification: focused unit tests, focused storage-writer integration test with real ClickHouse and S3-compatible object storage, `docker compose config --quiet`, and `bash scripts/local-ci.sh` before push because this is a code-change slice.
Baseline: run the focused storage-writer tests before implementation to capture current state.
New errors introduced: none.
Telemetry impact: warm export cycles must emit structured logs with tenant_id, cutoff_unix_nano, exported row count, bucket, object key, and outcome.
Auth/tenancy impact: exported objects must include tenant_id in the object key and export rows must be selected with a tenant predicate.
Data retention or migration impact: copy-first warm retention adds no destructive mutation; ClickHouse TTL and existing hot deletion behavior remain unchanged.
Rollback path: set `WARM_RETENTION_ENABLED=false` or remove the object-store env vars; no hot rows are deleted by this slice, so rollback does not require rehydration.
ADR/spec sync: no ADR change expected if the slice follows ADR-012 and ADR-025; update specs only if object format, durations, or query semantics change.
Checkpoint question: does the first warm movement path preserve current hot-tier query semantics while proving S3-compatible export and readback?
Next smallest slice: add a warm-retention manifest table or query-api warm-read proof, depending on whether operators need auditability or cross-tier reads first.

---

### Task 1: Choose Dependencies And Add Pure Warm-Retention Unit Tests

**Files:**
- Modify: `services/storage-writer/Cargo.toml`
- Create: `services/storage-writer/src/warm_retention.rs`
- Test: `cargo test -p storage-writer warm_retention --lib`

- [ ] **Step 1: Check dependency versions**

Run:

```bash
cargo search object_store
cargo search aws-sdk-s3
cargo search testcontainers-modules
```

Expected: record the latest stable versions in the PR body before editing `Cargo.toml`. Prefer the smallest crate that supports S3-compatible endpoint URL, access key, secret key, bucket, and path-style addressing against MinIO.

- [ ] **Step 2: Add dependencies**

Add the selected runtime dependency to `services/storage-writer/Cargo.toml`. Add a dev-dependency for the object-store Testcontainers module only if the selected Testcontainers crate exposes a MinIO/localstack module; otherwise the integration test may use `testcontainers::GenericImage`.

Expected: dependency entries stay scoped to `storage-writer`; do not add a workspace-wide dependency until at least two crates need it.

- [ ] **Step 3: Create `warm_retention.rs` with failing unit tests first**

Create `services/storage-writer/src/warm_retention.rs` with tests that name the required API before it exists:

```rust
#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{warm_span_object_key, WarmRetentionConfig};
    use uuid::Uuid;

    #[test]
    fn config_defaults_disabled_and_copy_first_safe() {
        let cfg = WarmRetentionConfig::from_values(None, None, None, None, None, None, None, None);
        assert!(!cfg.enabled);
        assert_eq!(cfg.bucket, "observable-warm-retention");
        assert_eq!(cfg.endpoint_url, "http://localhost:9000");
        assert_eq!(cfg.hot_trace_days, 14);
        assert_eq!(cfg.batch_limit, 1_000);
        assert_eq!(cfg.check_interval, Duration::from_secs(3600));
    }

    #[test]
    fn config_clamps_hot_trace_days_to_existing_hot_window() {
        let low = WarmRetentionConfig::from_values(
            None, None, None, None, None, Some("1".into()), None, None,
        );
        let high = WarmRetentionConfig::from_values(
            None, None, None, None, None, Some("90".into()), None, None,
        );
        assert_eq!(low.hot_trace_days, 3);
        assert_eq!(high.hot_trace_days, 14);
    }

    #[test]
    fn object_key_is_tenant_scoped_and_cutoff_scoped() {
        let tenant_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let key = warm_span_object_key(tenant_id, 1_700_000_000_000_000_000);
        assert_eq!(
            key,
            "spans/tenant_id=00000000-0000-0000-0000-000000000001/cutoff=1700000000000000000/spans.jsonl"
        );
    }
}
```

- [ ] **Step 4: Run tests to verify the expected failure**

Run:

```bash
cargo test -p storage-writer warm_retention --lib
```

Expected: FAIL because `from_values()` and `warm_span_object_key()` are not implemented.

- [ ] **Step 5: Implement config parsing and object-key construction**

Add this implementation above the test module:

```rust
use std::time::Duration;

use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarmRetentionConfig {
    pub enabled: bool,
    pub bucket: String,
    pub endpoint_url: String,
    pub access_key: String,
    pub secret_key: String,
    pub hot_trace_days: u64,
    pub batch_limit: u64,
    pub check_interval: Duration,
}

impl WarmRetentionConfig {
    pub fn from_values(
        enabled: Option<String>,
        bucket: Option<String>,
        endpoint_url: Option<String>,
        access_key: Option<String>,
        secret_key: Option<String>,
        hot_trace_days: Option<String>,
        batch_limit: Option<String>,
        interval_secs: Option<String>,
    ) -> Self {
        let enabled = enabled
            .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false);
        let hot_trace_days = hot_trace_days
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(14)
            .clamp(3, 14);
        let batch_limit = batch_limit
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(1_000)
            .clamp(1, 10_000);
        let check_interval = Duration::from_secs(
            interval_secs
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(3600),
        );
        Self {
            enabled,
            bucket: bucket.unwrap_or_else(|| "observable-warm-retention".to_string()),
            endpoint_url: endpoint_url.unwrap_or_else(|| "http://localhost:9000".to_string()),
            access_key: access_key.unwrap_or_else(|| "minioadmin".to_string()),
            secret_key: secret_key.unwrap_or_else(|| "minioadmin".to_string()),
            hot_trace_days,
            batch_limit,
            check_interval,
        }
    }

    pub fn from_env() -> Self {
        Self::from_values(
            std::env::var("WARM_RETENTION_ENABLED").ok(),
            std::env::var("WARM_RETENTION_BUCKET").ok(),
            std::env::var("WARM_RETENTION_ENDPOINT_URL").ok(),
            std::env::var("WARM_RETENTION_ACCESS_KEY").ok(),
            std::env::var("WARM_RETENTION_SECRET_KEY").ok(),
            std::env::var("TRACE_HOT_RETENTION_DAYS").ok(),
            std::env::var("WARM_RETENTION_BATCH_LIMIT").ok(),
            std::env::var("WARM_RETENTION_CHECK_INTERVAL_SECONDS").ok(),
        )
    }
}

pub fn warm_span_object_key(tenant_id: Uuid, cutoff_unix_nano: u64) -> String {
    format!("spans/tenant_id={tenant_id}/cutoff={cutoff_unix_nano}/spans.jsonl")
}
```

The config parsing must use these exact defaults:

```rust
bucket: observable-warm-retention
endpoint_url: http://localhost:9000
access_key: minioadmin
secret_key: minioadmin
hot_trace_days: 14, clamped to 3..=14
batch_limit: 1000, clamped to 1..=10000
check_interval: 3600 seconds
```

- [ ] **Step 6: Re-run unit tests**

Run:

```bash
cargo test -p storage-writer warm_retention --lib
```

Expected: PASS.

- [ ] **Step 7: Commit**

Run:

```bash
git add services/storage-writer/Cargo.toml services/storage-writer/src/warm_retention.rs
git commit -m "Add warm retention config primitives"
```

Expected: commit succeeds on the feature branch after the focused tests pass.

---

### Task 2: Add Copy-First Warm Export Logic

**Files:**
- Modify: `services/storage-writer/src/warm_retention.rs`
- Test: `cargo test -p storage-writer warm_retention --lib`

- [ ] **Step 1: Add export row type and JSONL serializer test**

Extend `warm_retention.rs` with a row type that matches the span columns exported from ClickHouse:

```rust
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, clickhouse::Row)]
pub struct WarmSpanExportRow {
    #[serde(with = "clickhouse::serde::uuid")]
    pub tenant_id: Uuid,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub service_name: String,
    pub operation_name: String,
    pub start_time_unix_nano: u64,
    pub end_time_unix_nano: u64,
    pub duration_ns: u64,
}
```

Add a test:

```rust
#[test]
fn encode_jsonl_writes_one_json_object_per_line() {
    let tenant_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let rows = vec![WarmSpanExportRow {
        tenant_id,
        trace_id: "trace-a".into(),
        span_id: "span-a".into(),
        parent_span_id: None,
        service_name: "checkout".into(),
        operation_name: "GET /checkout".into(),
        start_time_unix_nano: 1,
        end_time_unix_nano: 2,
        duration_ns: 1,
    }];
    let bytes = encode_jsonl(&rows).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.ends_with('\n'));
    assert!(text.contains("\"tenant_id\":\"00000000-0000-0000-0000-000000000001\""));
    assert!(text.contains("\"trace_id\":\"trace-a\""));
}
```

- [ ] **Step 2: Run tests to verify the expected failure**

Run:

```bash
cargo test -p storage-writer warm_retention --lib
```

Expected: FAIL because `encode_jsonl()` is not defined.

- [ ] **Step 3: Implement serializer**

Add this function:

```rust
pub fn encode_jsonl(rows: &[WarmSpanExportRow]) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::new();
    for row in rows {
        serde_json::to_writer(&mut out, row)?;
        out.push(b'\n');
    }
    Ok(out)
}
```

- [ ] **Step 4: Add export query builder test**

Add:

```rust
pub fn select_warm_spans_sql() -> &'static str {
    "SELECT tenant_id, trace_id, span_id, parent_span_id, service_name, operation_name, \
     start_time_unix_nano, end_time_unix_nano, duration_ns \
     FROM observable.spans \
     WHERE tenant_id = ? AND start_time_unix_nano < ? \
     ORDER BY start_time_unix_nano ASC \
     LIMIT ?"
}

#[test]
fn select_warm_spans_sql_is_tenant_scoped_and_cutoff_scoped() {
    let sql = select_warm_spans_sql();
    assert!(sql.contains("WHERE tenant_id = ? AND start_time_unix_nano < ?"));
    assert!(sql.contains("LIMIT ?"));
    assert!(!sql.contains("ALTER TABLE"));
    assert!(!sql.contains("DELETE"));
}
```

- [ ] **Step 5: Add export cycle seam**

Add an `export_warm_spans_for_tenant()` function that:

1. Computes the cutoff with `crate::retention::cutoff_unix_nano(now_unix_secs, config.hot_trace_days)`.
2. Calls `select_warm_spans_sql()` against ClickHouse with tenant_id, cutoff, and batch_limit binds.
3. Returns early with `Ok(None)` when no rows are found.
4. Writes JSONL bytes to the object-store key returned by `warm_span_object_key()`.
5. Returns `Ok(Some(key))` without deleting or mutating ClickHouse rows.

Keep the object-store client behind the smallest local wrapper or trait needed by unit tests; do not introduce a shared abstraction outside storage-writer in this slice.

- [ ] **Step 6: Re-run unit tests**

Run:

```bash
cargo test -p storage-writer warm_retention --lib
```

Expected: PASS.

- [ ] **Step 7: Commit**

Run:

```bash
git add services/storage-writer/src/warm_retention.rs
git commit -m "Add copy-first warm span export logic"
```

Expected: commit succeeds after the focused tests pass.

---

### Task 3: Cover Warm Export With Real ClickHouse And S3-Compatible Storage

**Files:**
- Create: `services/storage-writer/tests/warm_retention_integration.rs`
- Modify: `services/storage-writer/Cargo.toml` if the integration fixture needs an additional dev-dependency.
- Test: `cargo test -p storage-writer --test warm_retention_integration -- --nocapture`

- [ ] **Step 1: Write failing integration test**

Create `services/storage-writer/tests/warm_retention_integration.rs` that:

1. Starts ClickHouse with `testcontainers_modules::clickhouse::ClickHouse` pinned to `24.3`.
2. Applies `migrations/clickhouse/*.sql`.
3. Starts MinIO or localstack with a pinned image through Testcontainers.
4. Creates bucket `observable-warm-retention`.
5. Inserts two spans for the same tenant: one older than the cutoff and one newer than the cutoff.
6. Calls `storage_writer::warm_retention::export_warm_spans_for_tenant()`.
7. Reads the written object from S3-compatible storage.
8. Asserts the object contains the old span and does not contain the new span.
9. Queries ClickHouse and asserts both rows still exist.

Expected test name:

```rust
#[tokio::test]
async fn warm_export_writes_aged_spans_to_object_storage_without_deleting_hot_rows() {
    // full fixture and assertions
}
```

- [ ] **Step 2: Run test to verify expected failure**

Run:

```bash
cargo test -p storage-writer --test warm_retention_integration -- --nocapture
```

Expected: FAIL until the object-store fixture and export seam are complete.

- [ ] **Step 3: Complete fixture and export seam**

Implement only the missing pieces required by the test. Keep credentials local to the fixture:

```text
MINIO_ROOT_USER=minioadmin
MINIO_ROOT_PASSWORD=minioadmin
bucket=observable-warm-retention
```

Expected: the test uses randomized tenant IDs and does not depend on any long-running local service.

- [ ] **Step 4: Re-run focused integration test**

Run:

```bash
cargo test -p storage-writer --test warm_retention_integration -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add services/storage-writer/Cargo.toml services/storage-writer/tests/warm_retention_integration.rs services/storage-writer/src/warm_retention.rs
git commit -m "Add warm retention object-store integration test"
```

Expected: commit succeeds after the focused integration test passes.

---

### Task 4: Wire The Worker Into Storage-Writer And Local Compose

**Files:**
- Modify: `services/storage-writer/src/main.rs`
- Modify: `docker-compose.yml`
- Test: `docker compose config --quiet`

- [ ] **Step 1: Wire module and disabled-by-default worker**

Modify `services/storage-writer/src/main.rs`:

```rust
mod warm_retention;
```

After the existing hot retention worker starts, construct `WarmRetentionConfig::from_env()`. If enabled, create the object-store client and spawn the warm worker. If disabled, log:

```rust
tracing::info!("warm retention disabled");
```

Expected: default local behavior remains unchanged because `WARM_RETENTION_ENABLED` defaults to false.

- [ ] **Step 2: Add local MinIO services to Compose**

Add a Compose service:

```yaml
  minio:
    image: minio/minio:RELEASE.2025-09-07T16-13-09Z
    command: ["server", "/data", "--console-address", ":9001"]
    ports: ["9000:9000", "9001:9001"]
    environment:
      MINIO_ROOT_USER: ${MINIO_ROOT_USER:-minioadmin}
      MINIO_ROOT_PASSWORD: ${MINIO_ROOT_PASSWORD:-minioadmin}
    volumes: [minio_data:/data]
    healthcheck:
      test: ["CMD", "mc", "ready", "local"]
      interval: 2s
      timeout: 1s
      retries: 20
```

Add a setup service that creates `observable-warm-retention` idempotently with `minio/mc:RELEASE.2025-08-13T08-35-41Z` and `mc mb --ignore-existing`.

Add `minio_data:` under `volumes:`.

Before committing, re-check whether a newer stable official MinIO image exists. If it does, use the newer pinned release tag and record the source in the PR body. As of the planning review, Docker Hub lists `minio/minio:RELEASE.2025-09-07T16-13-09Z` as the newest official `minio/minio` release tag and `minio/mc:RELEASE.2025-08-13T08-35-41Z` as the newest official `minio/mc` release tag.

- [ ] **Step 3: Add storage-writer env vars**

Add:

```yaml
      WARM_RETENTION_ENABLED: ${WARM_RETENTION_ENABLED:-false}
      WARM_RETENTION_BUCKET: ${WARM_RETENTION_BUCKET:-observable-warm-retention}
      WARM_RETENTION_ENDPOINT_URL: ${WARM_RETENTION_ENDPOINT_URL:-http://minio:9000}
      WARM_RETENTION_ACCESS_KEY: ${MINIO_ROOT_USER:-minioadmin}
      WARM_RETENTION_SECRET_KEY: ${MINIO_ROOT_PASSWORD:-minioadmin}
      WARM_RETENTION_BATCH_LIMIT: ${WARM_RETENTION_BATCH_LIMIT:-1000}
```

Add `minio-setup` as a dependency only if warm retention startup requires the bucket to exist at service boot. If bucket creation is performed by the worker, keep Compose dependency unchanged and document the decision in the PR.

- [ ] **Step 4: Validate Compose syntax**

Run:

```bash
docker compose config --quiet
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add services/storage-writer/src/main.rs docker-compose.yml
git commit -m "Wire warm retention into storage-writer"
```

Expected: commit succeeds after Compose validation passes.

---

### Task 5: Verify And Update Planning State

**Files:**
- Modify: `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`
- Modify: `docs/agent-context.md` only if future-agent guidance changes.
- Test: `git diff --check`

- [ ] **Step 1: Run focused checks**

Run:

```bash
cargo test -p storage-writer warm_retention --lib
cargo test -p storage-writer --test warm_retention_integration -- --nocapture
docker compose config --quiet
```

Expected: PASS.

- [ ] **Step 2: Run mandatory code gate**

Run:

```bash
bash scripts/local-ci.sh
```

Expected: PASS. If Docker, frontend, Helm, or smoke infrastructure is unavailable, use the narrowest documented skip flag only after recording why the skipped stage is not applicable or cannot run in the local environment.

- [ ] **Step 3: Update active roadmap**

In `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`, mark P4-S1 complete with:

```markdown
- [x] **P4-S1: Add one warm-retention movement path**
  - Outcome: aged span rows are copied from ClickHouse to S3-compatible warm storage under deterministic tenant/cutoff object keys. Hot rows are not deleted, so existing query semantics remain unchanged.
  - Checkpoint: do query semantics stay stable across tiers? Answer: yes for this copy-first slice. Query-api still reads hot ClickHouse only, and the integration test proves warm export does not delete hot rows.
```

- [ ] **Step 4: Update active detailed plan pointer**

If the next detailed plan exists, update `docs/agent-context.md` to point to it. If it does not exist, set:

```markdown
- Active detailed implementation plan: none; write the next plan from `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md` before starting P4-S2.
```

- [ ] **Step 5: Run documentation hygiene**

Run:

```bash
git diff --check
```

Expected: PASS.

- [ ] **Step 6: Commit**

Run:

```bash
git add docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md docs/agent-context.md
git commit -m "Update plan after warm retention slice"
```

Expected: commit succeeds after verification notes are ready for the PR.

---

## Verification Plan

Required for implementation PRs:

```bash
cargo test -p storage-writer warm_retention --lib
cargo test -p storage-writer --test warm_retention_integration -- --nocapture
docker compose config --quiet
bash scripts/local-ci.sh
```

Documentation-only edits to this plan remain exempt from `bash scripts/local-ci.sh`, but must run:

```bash
git diff --check
```

## ADR/Spec Synchronization

This plan is governed by ADR-012, ADR-025, `spec/03-storage.md §5.3`, `spec/10-process.md §17`, and `spec/11-testing.md §18.8`. No ADR change is expected if implementation keeps the first slice copy-first, uses S3-compatible object storage for warm retention, preserves hot query semantics, and adds object-storage Testcontainers coverage. Update ADR/spec files in the same PR if implementation changes retention durations, object format guarantees, destructive deletion behavior, query federation scope, deployment assumptions, or dependency-boundary verification policy.
