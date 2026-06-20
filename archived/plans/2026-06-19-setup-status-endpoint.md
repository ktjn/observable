# Setup Status Endpoint Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Onboarding Wizard's client-side 3-query fan-out (separate calls to `/v1/traces`, `/v1/logs`, `/v1/metrics`) with a single consolidated `GET /v1/setup/status` backend endpoint, closing the literal gap between the roadmap's Onboarding Wizard deliverable and what's actually shipped.

**Architecture:** The Onboarding Wizard frontend (`apps/frontend/src/features/onboarding/OnboardingWizard.tsx`) and its 4-step flow (language picker → API key creation → polling → success) are already fully built and shipped on `main`. The only literal gap vs. the roadmap line ("Onboarding Wizard ... `GET /v1/setup/status`") is that polling currently calls 3 separate existing query-api endpoints client-side instead of one purpose-built status endpoint. This plan adds that endpoint to `query-api` (mirroring the existing tenant-scoped GET handler pattern, e.g. `metrics::list_metrics`), and rewires the frontend's `getFirstSignalStatus()` to call it instead.

**Tech Stack:** Rust (axum, ClickHouse client), React 19 + TanStack Query, Vitest.

## Global Constraints

- Reuse the existing `TenantContext` / `require_tenant` auth middleware pattern — no new auth bypass needed (the wizard already has a valid API key by the time it reaches the polling step).
- Every backend slice touching ClickHouse adds a Testcontainers integration test (roadmap §1 rule 5). This task adds one to the existing `services/query-api/tests/clickhouse_integration.rs`, following that file's established helper pattern (`make_span`/`insert_span`, `make_log_row`/`insert_log`) rather than creating a new test file.
- `cargo fmt --all` after every Rust edit, before staging (recurring project requirement).
- Match the existing "60 minute lookback" window the frontend currently uses (`Date.now() - 60 * 60 * 1000` in the old `getFirstSignalStatus`), so behavior doesn't regress.
- Keep the PR small — this is a single focused backend+frontend slice, not a rewrite of the wizard.

---

## Task 1: `GET /v1/setup/status` backend endpoint

**Files:**
- Create: `services/query-api/src/setup.rs`
- Modify: `services/query-api/src/main.rs:14` (add `mod setup;` alphabetically after `mod schemas;`, before `mod slos;`) and the route chain (add after the `/v1/metrics/{series_id}` line, ~line 85)
- Modify: `services/query-api/src/lib.rs:20` (add `pub mod setup;` at the same alphabetical position)

**Interfaces:**
- Consumes: `crate::traces::AppState` (existing, has `ch: clickhouse::Client`), `crate::middleware::auth::TenantContext` (existing).
- Produces: `pub async fn compute_setup_status(ch: &clickhouse::Client, tenant_id: Uuid) -> anyhow::Result<SetupStatusResponse>` — a plain ClickHouse-client-level function (not just the axum handler) so Task 2's integration test can call it directly without going through HTTP/auth, matching `traces::fetch_trace_spans`'s `anyhow::Result<Vec<SpanRow>>` shape. `SetupStatusResponse` is `pub` with `pub` fields.

- [x] **Step 1: Write `services/query-api/src/setup.rs`**

```rust
use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
};
use chrono::Utc;
use clickhouse::Client;
use serde::Serialize;
use uuid::Uuid;

use crate::middleware::auth::TenantContext;
use crate::traces::AppState;

/// How far back to look for a tenant's first ingested signal — matches the
/// onboarding wizard's original client-side polling window (60 minutes).
const LOOKBACK_NANOS: u64 = 3_600_000_000_000;

#[derive(Serialize, PartialEq, Debug)]
pub struct SetupStatusResponse {
    pub state: &'static str,
    pub traces: u64,
    pub logs: u64,
    pub metrics: u64,
}

pub async fn compute_setup_status(ch: &Client, tenant_id: Uuid) -> anyhow::Result<SetupStatusResponse> {
    let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64;
    let since_ns = now_ns.saturating_sub(LOOKBACK_NANOS);
    let since_secs = since_ns / 1_000_000_000;

    let traces: u64 = ch
        .query(
            "SELECT count(DISTINCT trace_id) FROM observable.spans \
             WHERE tenant_id = ? AND start_time_unix_nano >= ?",
        )
        .bind(tenant_id)
        .bind(since_ns)
        .fetch_one()
        .await?;

    let logs: u64 = ch
        .query(
            "SELECT count() FROM observable.logs \
             WHERE tenant_id = ? AND timestamp_unix_nano >= ?",
        )
        .bind(tenant_id)
        .bind(since_ns)
        .fetch_one()
        .await?;

    let metrics: u64 = ch
        .query(
            "SELECT count() FROM observable.metric_series \
             WHERE tenant_id = ? AND created_at >= fromUnixTimestamp(?)",
        )
        .bind(tenant_id)
        .bind(since_secs)
        .fetch_one()
        .await?;

    let state = if traces + logs + metrics > 0 {
        "detected"
    } else {
        "waiting"
    };

    Ok(SetupStatusResponse {
        state,
        traces,
        logs,
        metrics,
    })
}

pub async fn get_setup_status(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<SetupStatusResponse>, StatusCode> {
    let status = compute_setup_status(&state.ch, ctx.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "setup_status query error");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(status))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_serializes_detected_state() {
        let r = SetupStatusResponse {
            state: "detected",
            traces: 2,
            logs: 0,
            metrics: 1,
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["state"], "detected");
        assert_eq!(v["traces"], 2);
        assert_eq!(v["logs"], 0);
        assert_eq!(v["metrics"], 1);
    }

    #[test]
    fn response_serializes_waiting_state() {
        let r = SetupStatusResponse {
            state: "waiting",
            traces: 0,
            logs: 0,
            metrics: 0,
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["state"], "waiting");
    }
}
```

- [x] **Step 2: Register the module and route**

In `services/query-api/src/main.rs`, add `mod setup;` to the `mod` list (insert after `mod schemas;`, before `mod slos;` — alphabetical order matches the existing list).

Also add `pub mod setup;` to `services/query-api/src/lib.rs` at the same alphabetical position (after `pub mod schemas;`, before `pub mod slos;`).

Add the route in the same `Router::new()` chain that has `.route("/v1/metrics/{series_id}", get(metrics::get_metric_points))` (~line 85), immediately after it:

```rust
        .route("/v1/setup/status", get(setup::get_setup_status))
```

- [x] **Step 3: Run unit tests**

Run: `cargo test -p query-api setup`
Expected: 2 tests pass (`response_serializes_detected_state`, `response_serializes_waiting_state`).

- [x] **Step 4: Format and commit**

```bash
cargo fmt --all
git add services/query-api/src/setup.rs services/query-api/src/main.rs services/query-api/src/lib.rs
git commit -m "feat(query-api): add GET /v1/setup/status onboarding status endpoint"
```

---

## Task 2: ClickHouse Testcontainers integration test

**Files:**
- Modify: `services/query-api/tests/clickhouse_integration.rs`

**Interfaces:**
- Consumes: `query_api::setup::compute_setup_status` (Task 1, must be `pub`); reuses this file's existing `make_span`/`insert_span` and `make_log_row_at`/`insert_log` helpers.
- Produces: nothing other tasks depend on.

- [x] **Step 1: Add a `metric_series` helper**

In `services/query-api/tests/clickhouse_integration.rs`, add `MetricSeriesRow` to the top-level `use domain::{LogRow, SpanRow};` import (changing it to `use domain::{LogRow, MetricSeriesRow, SpanRow};`), then add this helper function immediately after the existing `insert_log` function (after line 172):

```rust
fn make_metric_series(tenant_id: Uuid, metric_name: &str) -> MetricSeriesRow {
    MetricSeriesRow {
        tenant_id,
        metric_series_id: Uuid::new_v4(),
        metric_name: metric_name.into(),
        description: String::new(),
        unit: "1".into(),
        metric_type: "sum".into(),
        is_monotonic: Some(1),
        aggregation_temporality: Some("delta".into()),
        attributes: "{}".into(),
        resource_attributes: "{}".into(),
        service_name: "test-svc".into(),
        environment: "test".into(),
    }
}

async fn insert_metric_series(ch: &Client, row: MetricSeriesRow) {
    let mut ins = ch
        .insert::<MetricSeriesRow>("metric_series")
        .await
        .expect("metric_series insert handle created");
    ins.write(&row).await.expect("metric_series row written");
    ins.end().await.expect("metric_series insert committed");
}
```

- [x] **Step 2: Add the test module**

Add this `use` near the top of the file (alongside the other `query_api::` imports, e.g. after `use query_api::planner::QueryPlanner;`):

```rust
use query_api::setup::compute_setup_status;
```

Add these test functions at the end of the file:

```rust
#[tokio::test]
async fn setup_status_reports_waiting_when_tenant_has_no_signals() {
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

    let tenant_id = Uuid::new_v4();
    let status = compute_setup_status(&ch, tenant_id)
        .await
        .expect("query succeeds");

    assert_eq!(status.state, "waiting");
    assert_eq!(status.traces, 0);
    assert_eq!(status.logs, 0);
    assert_eq!(status.metrics, 0);
}

#[tokio::test]
async fn setup_status_reports_detected_after_first_span() {
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

    let tenant_id = Uuid::new_v4();
    insert_span(&ch, make_span(tenant_id, "trace-1", "span-1")).await;

    let status = compute_setup_status(&ch, tenant_id)
        .await
        .expect("query succeeds");

    assert_eq!(status.state, "detected");
    assert_eq!(status.traces, 1);
}

#[tokio::test]
async fn setup_status_reports_detected_from_logs_or_metrics_alone() {
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

    let tenant_id = Uuid::new_v4();
    insert_log(&ch, make_log_row(tenant_id, "test-svc")).await;
    insert_metric_series(&ch, make_metric_series(tenant_id, "requests_total")).await;

    let status = compute_setup_status(&ch, tenant_id)
        .await
        .expect("query succeeds");

    assert_eq!(status.state, "detected");
    assert_eq!(status.traces, 0);
    assert_eq!(status.logs, 1);
    assert_eq!(status.metrics, 1);
}

#[tokio::test]
async fn setup_status_excludes_other_tenants_signals() {
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

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    insert_span(&ch, make_span(tenant_a, "trace-a", "span-a")).await;

    let status_b = compute_setup_status(&ch, tenant_b)
        .await
        .expect("query succeeds");

    assert_eq!(status_b.state, "waiting", "tenant B must not see tenant A's signals");
}

#[tokio::test]
async fn setup_status_excludes_signals_outside_the_lookback_window() {
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

    let tenant_id = Uuid::new_v4();
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let two_hours_ago_ns = now_ns.saturating_sub(2 * 3_600_000_000_000);
    insert_log(&ch, make_log_row_at(tenant_id, "test-svc", two_hours_ago_ns)).await;

    let status = compute_setup_status(&ch, tenant_id)
        .await
        .expect("query succeeds");

    assert_eq!(status.state, "waiting");
    assert_eq!(status.logs, 0);
}
```

- [x] **Step 3: Run the integration tests**

Run: `cargo test -p query-api --test clickhouse_integration setup_status`
Expected: all 5 new tests pass (requires Docker running for Testcontainers).

- [x] **Step 4: Format and commit**

```bash
cargo fmt --all
git add services/query-api/tests/clickhouse_integration.rs
git commit -m "test(query-api): add Testcontainers coverage for setup_status"
```

---

## Task 3: Frontend — wire `getFirstSignalStatus` to the new endpoint

**Files:**
- Modify: `apps/frontend/src/api/setup.ts`
- Create: `apps/frontend/src/api/setup.test.ts`

**Interfaces:**
- Consumes: `GET /v1/setup/status` (Task 1).
- Produces: `getFirstSignalStatus(tenantId)` keeps its existing signature and `FirstSignalStatus` return shape — Task 4's wizard test file and `OnboardingWizard.tsx`'s `StepWaiting` component (which calls this function) are unaffected by the internal rewrite.

- [x] **Step 1: Rewrite `getFirstSignalStatus` in `apps/frontend/src/api/setup.ts`**

Replace lines 1-45 (the `searchLogs`/`listMetrics`/`searchTraces` imports through the end of `getFirstSignalStatus`) with:

```typescript
export const OTLP_GRPC_ENDPOINT = "http://localhost:4317";
export const OTLP_HTTP_JSON_TRACES = "http://localhost:4318/v1/traces";
export const OTLP_HTTP_JSON_METRICS = "http://localhost:4318/v1/metrics";
export const OTLP_HTTP_JSON_LOGS = "http://localhost:4318/v1/logs";
export const LOCAL_DEV_API_KEY = "dev-api-key-0000";
export const REDACTED_LOCAL_API_KEY = "dev-api-key-...-0000";

export interface FirstSignalStatus {
  state: "detected" | "waiting" | "error";
  traces: number;
  logs: number;
  metrics: number;
}

export async function getFirstSignalStatus(tenantId: string): Promise<FirstSignalStatus> {
  try {
    const res = await fetch("/v1/setup/status", {
      credentials: "include",
      headers: {
        "x-api-key": LOCAL_DEV_API_KEY,
        "X-Tenant-ID": tenantId,
      },
    });
    if (!res.ok) {
      return { state: "error", traces: 0, logs: 0, metrics: 0 };
    }
    return (await res.json()) as FirstSignalStatus;
  } catch {
    return { state: "error", traces: 0, logs: 0, metrics: 0 };
  }
}
```

This removes the now-unused `searchLogs`, `listMetrics`, and `searchTraces` imports (the only call site was `getFirstSignalStatus`) and replaces the 3-request `Promise.allSettled` fan-out with one call to the new backend endpoint, matching `getConfig`'s existing header/credentials pattern later in the same file.

- [x] **Step 2: Write `apps/frontend/src/api/setup.test.ts`**

```typescript
import { describe, it, expect, vi, beforeEach } from "vitest";
import { getFirstSignalStatus } from "./setup";

const MOCK_TENANT_ID = "00000000-0000-0000-0000-000000000001";

describe("getFirstSignalStatus", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", vi.fn());
  });

  it("calls GET /v1/setup/status with tenant headers", async () => {
    vi.mocked(fetch).mockResolvedValueOnce({
      ok: true,
      json: async () => ({ state: "waiting", traces: 0, logs: 0, metrics: 0 }),
    } as Response);

    await getFirstSignalStatus(MOCK_TENANT_ID);

    const [url, init] = vi.mocked(fetch).mock.calls[0];
    expect(url).toBe("/v1/setup/status");
    expect((init?.headers as Record<string, string>)["X-Tenant-ID"]).toBe(MOCK_TENANT_ID);
  });

  it("returns the parsed status on success", async () => {
    vi.mocked(fetch).mockResolvedValueOnce({
      ok: true,
      json: async () => ({ state: "detected", traces: 3, logs: 1, metrics: 0 }),
    } as Response);

    const result = await getFirstSignalStatus(MOCK_TENANT_ID);

    expect(result).toEqual({ state: "detected", traces: 3, logs: 1, metrics: 0 });
  });

  it("returns an error state when the request fails", async () => {
    vi.mocked(fetch).mockResolvedValueOnce({ ok: false } as Response);

    const result = await getFirstSignalStatus(MOCK_TENANT_ID);

    expect(result).toEqual({ state: "error", traces: 0, logs: 0, metrics: 0 });
  });

  it("returns an error state when fetch throws", async () => {
    vi.mocked(fetch).mockRejectedValueOnce(new Error("network down"));

    const result = await getFirstSignalStatus(MOCK_TENANT_ID);

    expect(result).toEqual({ state: "error", traces: 0, logs: 0, metrics: 0 });
  });
});
```

- [x] **Step 3: Run the frontend tests**

Run: `npm test -- setup.test`
Expected: all 4 tests pass.

- [x] **Step 4: Commit**

```bash
git add apps/frontend/src/api/setup.ts apps/frontend/src/api/setup.test.ts
git commit -m "feat(frontend): consolidate onboarding signal polling into /v1/setup/status"
```

---

## Task 4: Update the Onboarding Wizard's test fetch stubs

**Files:**
- Modify: `apps/frontend/src/features/onboarding/OnboardingWizard.test.tsx`

**Interfaces:**
- Consumes: Task 3's new `/v1/setup/status` contract.
- Produces: nothing other tasks depend on (terminal verification task).

**Why this is needed:** `OnboardingWizard.test.tsx`'s `stubFetch` currently stubs `/v1/traces`, `/v1/logs`, and `/v1/metrics` to simulate the polling responses. After Task 3, `getFirstSignalStatus` calls `/v1/setup/status` instead, so these stubs no longer intercept the polling request — it would fall through to the catch-all `{}` response, breaking the "shows success state when signals detected" test (which currently overrides `/v1/traces` to return `total: 1`).

- [x] **Step 1: Update the default stub and the success-state test**

In `apps/frontend/src/features/onboarding/OnboardingWizard.test.tsx`, replace lines 36-44 (the `/v1/traces`, `/v1/logs`, `/v1/metrics` branches inside `stubFetch`'s default handler) with:

```typescript
      if (url.includes("/v1/setup/status")) {
        return new Response(
          JSON.stringify({ state: "waiting", traces: 0, logs: 0, metrics: 0 }),
          { status: 200 },
        );
      }
```

Then update the `"shows success state when signals detected"` test (currently lines 126-163) to override `/v1/setup/status` instead of `/v1/traces`. Change the override block (currently lines 145-148):

```typescript
    "/v1/traces": () =>
      new Response(JSON.stringify({ traces: [{ trace_id: "t1" }], total: 1 }), {
        status: 200,
      }),
```

to:

```typescript
    "/v1/setup/status": () =>
      new Response(
        JSON.stringify({ state: "detected", traces: 1, logs: 0, metrics: 0 }),
        { status: 200 },
      ),
```

- [x] **Step 2: Run the wizard tests**

Run: `npm test -- OnboardingWizard`
Expected: all 6 existing tests still pass (`renders language picker...`, `Next button disabled...`, `advances to API key step...`, `creates token and shows waiting step`, `shows success state when signals detected`, `skip wizard sets complete flag`).

- [x] **Step 3: Run frontend typecheck and full suite**

Run: `npm run typecheck && npm test`
Expected: no errors; no regressions elsewhere.

- [x] **Step 4: Commit**

```bash
git add apps/frontend/src/features/onboarding/OnboardingWizard.test.tsx
git commit -m "test(onboarding): stub /v1/setup/status instead of the old 3-query fan-out"
```

---

## Task 5: Roadmap and agent-context housekeeping

**Files:**
- Modify: `docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md`
- Modify: `docs/agent-context.md`
- Move: this plan file to `archived/plans/`

**Interfaces:**
- Consumes: completion of Tasks 1-4.
- Produces: nothing (terminal documentation task).

- [x] **Step 1: Check off the roadmap item**

In `docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md`, change the Tier 1 line:

```markdown
- [ ] **Onboarding Wizard** (was P9-S1) — guided zero-to-first-trace flow: language/framework
```
to
```markdown
- [x] **Onboarding Wizard** (was P9-S1) — guided zero-to-first-trace flow: language/framework
```

- [x] **Step 2: Add an agent-context.md entry**

In `docs/agent-context.md`, add a new `## Setup Status Endpoint (P9-S1, completed 2026-06-19)` section after the most recent completed-feature section, summarizing: the Onboarding Wizard (`features/onboarding/OnboardingWizard.tsx`) was already shipped with its full 4-step flow; this slice closed the one literal gap (`GET /v1/setup/status`) by adding a consolidated backend endpoint (`services/query-api/src/setup.rs`) and rewiring `getFirstSignalStatus` to call it instead of fanning out to `/v1/traces`, `/v1/logs`, `/v1/metrics` client-side. Also append `archived/plans/2026-06-19-setup-status-endpoint.md` to the "Completed / archived detailed plans" bullet list.

- [x] **Step 3: Archive this plan**

```bash
git mv docs/superpowers/plans/2026-06-19-setup-status-endpoint.md archived/plans/2026-06-19-setup-status-endpoint.md
```
Mark every checkbox above `[x]` in the archived copy before committing.

- [x] **Step 4: Final verification**

Run: `bash scripts/local-ci.sh`
Expected: passes end-to-end (Rust fmt/clippy/tests, frontend typecheck/test/build, smoke test).

- [x] **Step 5: Commit**

```bash
git add docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md docs/agent-context.md archived/plans/2026-06-19-setup-status-endpoint.md
git commit -m "docs: close out onboarding wizard setup-status roadmap item"
```
