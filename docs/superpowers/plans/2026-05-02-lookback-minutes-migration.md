# Lookback Minutes Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove `lookback_minutes` from all API contracts (frontend + backend) and replace with `from`/`to` timestamp ranges; dashboard panels adopt a hybrid model where `preset: null` means "follow global date range."

**Architecture:** Three-layer migration executed front-to-back in compilation order. DB schema first (SQL migration), backend Rust structs + handlers second (removes the old param, adds from/to), frontend API layer + pages third (stops computing minutes, passes timestamps directly). The existing `useGlobalDateRange` hook already provides `fromMs`/`toMs` — pages just stop converting them back to minutes.

**Tech Stack:** PostgreSQL (sqlx migration), Rust/Axum (query param structs), TypeScript/React (TanStack Query + Router, Vitest)

---

## File Map

| Action | Path | What changes |
|--------|------|------|
| Create | `migrations/postgres/014_dashboard_preset.sql` | Add `preset TEXT`, backfill, drop `lookback_minutes` |
| Modify | `services/query-api/src/dashboards.rs` | `lookback_minutes: i32` → `preset: Option<String>` everywhere |
| Modify | `services/query-api/src/discovery.rs` | `SummaryParams`, `TopologyParams`, `ResponseTimeHistoryParams`: replace `lookback_minutes` with `from`/`to` |
| Modify | `services/query-api/src/traces.rs` | Remove `lookback_minutes` from `SearchParams`; update 4 usage sites |
| Modify | `services/query-api/src/planner/mod.rs` | Fix `trace_search_where_clause` guard; update `TopologyParams` test fixtures |
| Modify | `apps/frontend/src/api/services.ts` | Replace `lookback_minutes?: number` with `from?: number; to?: number` on 4 functions |
| Modify | `apps/frontend/src/api/traces.ts` | Remove `lookback_minutes` from `searchTraces` |
| Modify | `apps/frontend/src/api/setup.ts` | Update `searchTraces` call to use `from`/`to` |
| Modify | `apps/frontend/src/api/dashboards.ts` | Replace `lookback_minutes` with `preset: Preset \| null` |
| Modify | `apps/frontend/src/pages/ServiceDetailPage.tsx` | Remove minutes conversion; pass `from`/`to` to API calls |
| Modify | `apps/frontend/src/pages/LogSearch.tsx` | Update `createDashboard` call to use `preset: null` |
| Modify | `apps/frontend/src/pages/TraceSearch.tsx` | Update `createDashboard` call to use `preset: null` |
| Modify | `apps/frontend/src/pages/ServiceTopologyPage.tsx` | Remove hardcoded `?lookback_minutes=60` from popover links |
| Modify | `apps/frontend/src/pages/DashboardsPage.tsx` | Update panel card display from minutes to preset label |
| Modify | `apps/frontend/src/App.test.tsx` | Update mock responses + assertions |

---

## Task 1: Database Migration

**Files:**
- Create: `migrations/postgres/014_dashboard_preset.sql`

- [ ] **Step 1: Write the migration**

```sql
-- Add nullable preset column
ALTER TABLE dashboard_panels ADD COLUMN preset TEXT;

-- Backfill: snap lookback_minutes to nearest preset string
UPDATE dashboard_panels SET preset = CASE
  WHEN lookback_minutes <=   5 THEN '5m'
  WHEN lookback_minutes <=  15 THEN '15m'
  WHEN lookback_minutes <=  30 THEN '30m'
  WHEN lookback_minutes <=  60 THEN '1h'
  WHEN lookback_minutes <= 180 THEN '3h'
  ELSE '12h'
END;

-- Drop the old column
ALTER TABLE dashboard_panels DROP COLUMN lookback_minutes;
```

- [ ] **Step 2: Apply the migration**

Run from repo root:
```bash
cd C:/git/Observable
sqlx migrate run --source migrations/postgres
```

Expected: migration applies cleanly, `lookback_minutes` column is gone from `dashboard_panels`.

- [ ] **Step 3: Commit**

```bash
git add migrations/postgres/014_dashboard_preset.sql
git commit -m "feat: add dashboard_panels.preset column, drop lookback_minutes"
```

---

## Task 2: Backend — dashboards.rs

**Files:**
- Modify: `services/query-api/src/dashboards.rs`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)]` block at the bottom of `dashboards.rs`:

```rust
#[test]
fn validate_create_request_accepts_null_preset() {
    let req = CreateDashboardRequest {
        name: "My dashboard".into(),
        panels: vec![DashboardPanelRequest {
            title: "Panel 1".into(),
            query_kind: "logs".into(),
            service: None,
            preset: None,
            filters: serde_json::json!({}),
        }],
    };
    assert!(validate_create_request(&req).is_ok());
}

#[test]
fn validate_create_request_accepts_valid_preset() {
    let req = CreateDashboardRequest {
        name: "My dashboard".into(),
        panels: vec![DashboardPanelRequest {
            title: "Panel 1".into(),
            query_kind: "traces".into(),
            service: None,
            preset: Some("1h".into()),
            filters: serde_json::json!({}),
        }],
    };
    assert!(validate_create_request(&req).is_ok());
}

#[test]
fn validate_create_request_rejects_invalid_preset() {
    let req = CreateDashboardRequest {
        name: "My dashboard".into(),
        panels: vec![DashboardPanelRequest {
            title: "Panel 1".into(),
            query_kind: "logs".into(),
            service: None,
            preset: Some("99m".into()),
            filters: serde_json::json!({}),
        }],
    };
    assert!(validate_create_request(&req).is_err());
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd C:/git/Observable/services/query-api
cargo test validate_create_request
```

Expected: FAIL — `DashboardPanelRequest` has no `preset` field yet.

- [ ] **Step 3: Update the structs and SQL**

Replace the contents of `dashboards.rs` with the following changes (read the file first, then apply):

**`DashboardPanelItem`** — replace `lookback_minutes: i32` with `preset: Option<String>`:
```rust
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct DashboardPanelItem {
    pub panel_id: Uuid,
    pub title: String,
    pub query_kind: String,
    pub service: Option<String>,
    pub preset: Option<String>,
    pub filters: serde_json::Value,
}
```

**`DashboardPanelRow`** — same replacement:
```rust
#[derive(sqlx::FromRow)]
struct DashboardPanelRow {
    dashboard_id: Uuid,
    panel_id: Uuid,
    title: String,
    query_kind: String,
    service: Option<String>,
    preset: Option<String>,
    filters: serde_json::Value,
}
```

**`DashboardPanelRequest`** — same replacement:
```rust
#[derive(Deserialize)]
pub struct DashboardPanelRequest {
    pub title: String,
    pub query_kind: String,
    pub service: Option<String>,
    pub preset: Option<String>,
    pub filters: serde_json::Value,
}
```

**`list_dashboards` SELECT query** — change `lookback_minutes` → `preset`:
```rust
sqlx::query_as::<_, DashboardPanelRow>(
    "SELECT dashboard_id, panel_id, title, query_kind, service, preset, filters \
     FROM dashboard_panels \
     WHERE dashboard_id = ANY($1) \
     ORDER BY dashboard_id, position ASC",
)
```

**`list_dashboards` mapping** — change `lookback_minutes: panel.lookback_minutes` → `preset: panel.preset.clone()`:
```rust
.map(|panel| DashboardPanelItem {
    panel_id: panel.panel_id,
    title: panel.title.clone(),
    query_kind: panel.query_kind.clone(),
    service: panel.service.clone(),
    preset: panel.preset.clone(),
    filters: panel.filters.clone(),
})
```

**`create_dashboard` INSERT query** — change column and binding:
```rust
let item = sqlx::query_as::<_, DashboardPanelRow>(
    "INSERT INTO dashboard_panels \
     (dashboard_id, title, query_kind, service, preset, filters, position) \
     VALUES ($1, $2, $3, $4, $5, $6, $7) \
     RETURNING dashboard_id, panel_id, title, query_kind, service, preset, filters",
)
.bind(row.dashboard_id)
.bind(panel.title.trim())
.bind(&panel.query_kind)
.bind(panel.service.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()))
.bind(panel.preset.as_deref())
.bind(&panel.filters)
.bind(position as i32)
.fetch_one(&mut *tx)
```

**`create_dashboard` mapping** — same as list mapping:
```rust
panels.push(DashboardPanelItem {
    panel_id: item.panel_id,
    title: item.title,
    query_kind: item.query_kind,
    service: item.service,
    preset: item.preset,
    filters: item.filters,
});
```

**`validate_create_request`** — replace the `lookback_minutes` check with a preset validator:

The valid presets are `"5m"`, `"15m"`, `"30m"`, `"1h"`, `"3h"`, `"12h"`. `None` (global date range) is always valid.

```rust
const VALID_PRESETS: &[&str] = &["5m", "15m", "30m", "1h", "3h", "12h"];

fn validate_create_request(req: &CreateDashboardRequest) -> Result<(), CreateDashboardError> {
    if req.name.trim().is_empty() {
        return Err(CreateDashboardError::InvalidInput(
            "name is required".into(),
        ));
    }
    if req.panels.is_empty() {
        return Err(CreateDashboardError::InvalidInput(
            "at least one panel is required".into(),
        ));
    }
    for panel in &req.panels {
        if panel.title.trim().is_empty() {
            return Err(CreateDashboardError::InvalidInput(
                "panel title is required".into(),
            ));
        }
        if !VALID_QUERY_KINDS.contains(&panel.query_kind.as_str()) {
            return Err(CreateDashboardError::InvalidInput(format!(
                "query_kind must be one of: {}",
                VALID_QUERY_KINDS.join(", ")
            )));
        }
        if let Some(ref preset) = panel.preset {
            if !VALID_PRESETS.contains(&preset.as_str()) {
                return Err(CreateDashboardError::InvalidInput(format!(
                    "preset must be one of: {} (or omitted for global date range)",
                    VALID_PRESETS.join(", ")
                )));
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run the tests and verify they pass**

```bash
cd C:/git/Observable/services/query-api
cargo test dashboards
```

Expected: all dashboard tests pass.

- [ ] **Step 5: Commit**

```bash
git add services/query-api/src/dashboards.rs
git commit -m "feat: replace lookback_minutes with preset in dashboards backend"
```

---

## Task 3: Backend — discovery.rs (SummaryParams, TopologyParams, ResponseTimeHistoryParams)

**Files:**
- Modify: `services/query-api/src/discovery.rs`

These structs have no direct unit tests for their handler logic, so we update the structs and handler bodies together, then verify compilation.

- [ ] **Step 1: Update `SummaryParams`**

```rust
use chrono::{DateTime, Utc};

#[derive(Deserialize)]
pub struct SummaryParams {
    pub environment: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}
```

- [ ] **Step 2: Update `TopologyParams`**

```rust
#[derive(Deserialize)]
pub struct TopologyParams {
    pub environment: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub service: Option<String>,
}
```

- [ ] **Step 3: Update `ResponseTimeHistoryParams`**

```rust
#[derive(Deserialize)]
pub struct ResponseTimeHistoryParams {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub buckets: Option<u32>,
}
```

- [ ] **Step 4: Update `list_service_summaries` handler**

Replace the `lookback_mins` / `lookback_ns` computation with from/to:

```rust
pub async fn list_service_summaries(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<SummaryParams>,
) -> Result<Json<ServiceSummaryResponse>, StatusCode> {
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let from_ns = params
        .from
        .map(|dt| dt.timestamp_nanos_opt().unwrap_or(0) as u64)
        .unwrap_or_else(|| now_ns.saturating_sub(3_600_000_000_000));
    let to_ns = params
        .to
        .map(|dt| dt.timestamp_nanos_opt().unwrap_or(0) as u64)
        .unwrap_or(now_ns);
    let duration_secs = (to_ns.saturating_sub(from_ns)) as f64 / 1_000_000_000.0;

    let mut sql = "SELECT \
            service_name, \
            count() as request_count, \
            countIf(status_code = 'ERROR') as error_count, \
            quantile(0.95)(duration_ns) as p95_latency_ns \
        FROM observable.spans \
        WHERE tenant_id = ? AND start_time_unix_nano >= ?"
        .to_string();

    if params.environment.is_some() {
        sql.push_str(" AND environment = ?");
    }

    sql.push_str(" GROUP BY service_name ORDER BY service_name");

    let mut query = state.ch.query(&sql).bind(ctx.tenant_id).bind(from_ns);

    if let Some(ref env) = params.environment {
        query = query.bind(env);
    }

    let rows: Vec<ServiceSummaryRow> = query.fetch_all().await.map_err(|e| {
        tracing::error!(error = ?e, "ClickHouse service summary error");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let items = rows
        .into_iter()
        .map(|row| service_summary_from_row(row, duration_secs))
        .collect();

    Ok(Json(ServiceSummaryResponse { items }))
}
```

- [ ] **Step 5: Update `get_service_summary` handler**

```rust
pub async fn get_service_summary(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(service_name): Path<String>,
    Query(params): Query<SummaryParams>,
) -> Result<Json<ServiceDetailResponse>, StatusCode> {
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let from_ns = params
        .from
        .map(|dt| dt.timestamp_nanos_opt().unwrap_or(0) as u64)
        .unwrap_or_else(|| now_ns.saturating_sub(3_600_000_000_000));
    let to_ns = params
        .to
        .map(|dt| dt.timestamp_nanos_opt().unwrap_or(0) as u64)
        .unwrap_or(now_ns);
    let duration_secs = (to_ns.saturating_sub(from_ns)) as f64 / 1_000_000_000.0;

    let mut sql = "SELECT \
            service_name, \
            count() as request_count, \
            countIf(status_code = 'ERROR') as error_count, \
            quantile(0.95)(duration_ns) as p95_latency_ns \
        FROM observable.spans \
        WHERE tenant_id = ? AND service_name = ? AND start_time_unix_nano >= ?"
        .to_string();

    if params.environment.is_some() {
        sql.push_str(" AND environment = ?");
    }

    sql.push_str(" GROUP BY service_name LIMIT 1");

    let mut query = state
        .ch
        .query(&sql)
        .bind(ctx.tenant_id)
        .bind(&service_name)
        .bind(from_ns);

    if let Some(ref env) = params.environment {
        query = query.bind(env);
    }

    let row = query
        .fetch_optional::<ServiceSummaryRow>()
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "ClickHouse single service summary error");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(ServiceDetailResponse {
        service: service_summary_from_row(row, duration_secs),
    }))
}
```

- [ ] **Step 6: Update `get_service_response_time_history` handler**

```rust
pub async fn get_service_response_time_history(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(service_name): Path<String>,
    Query(params): Query<ResponseTimeHistoryParams>,
) -> Result<Json<ResponseTimeHistoryResponse>, StatusCode> {
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let from_ns = params
        .from
        .map(|dt| dt.timestamp_nanos_opt().unwrap_or(0) as u64)
        .unwrap_or_else(|| now_ns.saturating_sub(3_600_000_000_000));
    let to_ns = params
        .to
        .map(|dt| dt.timestamp_nanos_opt().unwrap_or(0) as u64)
        .unwrap_or(now_ns);
    let bucket_count = params.buckets.unwrap_or(60).clamp(1, 200);

    let plan = state
        .planner
        .plan_response_time_histogram(from_ns, to_ns, bucket_count);

    let mut cursor = state
        .ch
        .query(&plan.sql)
        .bind(plan.from_ns)
        .bind(plan.interval_ns)
        .bind(ctx.tenant_id)
        .bind(&service_name)
        .bind(from_ns)
        .bind(to_ns)
        .fetch::<(i64, f64, f64, u64)>()
        .map_err(|e| {
            tracing::error!(error = ?e, "ClickHouse response time histogram error");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let interval_secs = plan.interval_ns as f64 / 1_000_000_000.0;
    let mut raw: HashMap<i64, (f64, f64, u64)> = HashMap::new();
    while let Some((bucket_idx, p50_ns, p95_ns, span_count)) = cursor.next().await.map_err(|e| {
        tracing::error!(error = ?e, "ClickHouse response time histogram fetch error");
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        if bucket_idx >= 0 && bucket_idx < bucket_count as i64 {
            raw.insert(bucket_idx, (p50_ns, p95_ns, span_count));
        }
    }

    let buckets = (0..bucket_count)
        .map(|i| {
            let start_ns = plan.from_ns + i as u64 * plan.interval_ns;
            let end_ns = start_ns + plan.interval_ns;
            let (p50_ns, p95_ns, span_count) = raw.remove(&(i as i64)).unwrap_or((0.0, 0.0, 0));
            ResponseTimeBucket {
                start_ms: start_ns / 1_000_000,
                end_ms: end_ns / 1_000_000,
                p50_ms: p50_ns / 1_000_000.0,
                p95_ms: p95_ns / 1_000_000.0,
                request_rate: span_count as f64 / interval_secs,
            }
        })
        .collect();

    Ok(Json(ResponseTimeHistoryResponse { buckets }))
}
```

- [ ] **Step 7: Update `get_topology` handler**

Replace the `lookback_mins`/`lookback_ns` computation with from/to:

```rust
pub async fn get_topology(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<TopologyParams>,
) -> Result<Json<TopologyResponse>, StatusCode> {
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let start_ns = params
        .from
        .map(|dt| dt.timestamp_nanos_opt().unwrap_or(0) as u64)
        .unwrap_or_else(|| now_ns.saturating_sub(3_600_000_000_000));

    let plan = state.planner.plan_topology(&params);

    // Branch 1 binds: tenant_id, tenant_id, start_ns [, env, env] [, service, service]
    let mut query = state
        .ch
        .query(&plan.sql)
        .bind(ctx.tenant_id)
        .bind(ctx.tenant_id)
        .bind(start_ns);

    if let Some(ref env) = params.environment {
        query = query.bind(env).bind(env);
    }
    if let Some(ref service) = params.service {
        query = query.bind(service).bind(service);
    }

    // Branch 2 binds: tenant_id, tenant_id, start_ns [, env, env] [, service, service]
    query = query.bind(ctx.tenant_id).bind(ctx.tenant_id).bind(start_ns);

    if let Some(ref env) = params.environment {
        query = query.bind(env).bind(env);
    }
    if let Some(ref service) = params.service {
        query = query.bind(service).bind(service);
    }

    let rows: Vec<TopologyRow> = query.fetch_all().await.map_err(|e| {
        tracing::error!(error = ?e, "ClickHouse topology error");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let edges = rows
        .into_iter()
        .map(|row| {
            let error_rate = if row.request_count > 0 {
                (row.error_count as f64) / (row.request_count as f64)
            } else {
                0.0
            };
            TopologyEdge {
                caller: row.caller,
                callee: row.callee,
                request_count: row.request_count,
                error_rate,
                p95_latency_ms: row.p95_latency_ns / 1_000_000.0,
            }
        })
        .collect();

    Ok(Json(TopologyResponse { edges }))
}
```

- [ ] **Step 8: Run the tests**

```bash
cd C:/git/Observable/services/query-api
cargo test
```

Expected: all existing discovery tests pass (service summary row, health state thresholds, infrastructure tests).

- [ ] **Step 9: Commit**

```bash
git add services/query-api/src/discovery.rs
git commit -m "feat: replace lookback_minutes with from/to in discovery handlers"
```

---

## Task 4: Backend — traces.rs + planner

**Files:**
- Modify: `services/query-api/src/traces.rs`
- Modify: `services/query-api/src/planner/mod.rs`

- [ ] **Step 1: Remove `lookback_minutes` from `SearchParams` in `traces.rs`**

In `traces.rs` at the `SearchParams` struct (around line 50), remove the `lookback_minutes` field:

```rust
#[derive(Deserialize)]
pub struct SearchParams {
    pub service: Option<String>,
    pub limit: Option<u32>,
    pub facets: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}
```

- [ ] **Step 2: Update `from_ns` computation in `search_traces`**

In `search_traces` (around line 160), remove the `lookback_minutes` branch:

```rust
let from_ns = if let Some(dt) = params.from {
    dt.timestamp_nanos_opt().unwrap_or(0) as u64
} else {
    now_ns.saturating_sub(3_600_000_000_000)
};
```

- [ ] **Step 3: Update `bind_common` closure in `search_traces`**

The closure (around line 175) checks `params.from.is_some() || params.lookback_minutes.is_some()`. Replace with:

```rust
let bind_common = |mut q: clickhouse::query::Query| {
    q = q.bind(ctx.tenant_id);
    if params.from.is_some() {
        q = q.bind(from_ns);
    }
    if params.to.is_some() {
        q = q.bind(to_ns);
    }
    if let Some(ref svc) = params.service {
        q = q.bind(svc.as_str());
    }
    q
};
```

- [ ] **Step 4: Update facet SQL guard in `search_traces`**

Around line 217, change `if params.from.is_some() || params.lookback_minutes.is_some()` to:

```rust
if params.from.is_some() {
    facet_sql.push_str(" AND start_time_unix_nano >= ?");
}
```

- [ ] **Step 5: Update `trace_search_common_bind_count`**

```rust
fn trace_search_common_bind_count(params: &SearchParams) -> usize {
    let mut count = 1;
    if params.from.is_some() {
        count += 1;
    }
    if params.to.is_some() {
        count += 1;
    }
    if params.service.is_some() {
        count += 1;
    }
    count
}
```

- [ ] **Step 6: Update `trace_search_where_clause` in `planner/mod.rs`**

In `planner/mod.rs` (around line 254):

```rust
fn trace_search_where_clause(params: &TraceSearchParams) -> String {
    let mut where_clause = "WHERE tenant_id = ?".to_string();
    if params.from.is_some() {
        where_clause.push_str(" AND start_time_unix_nano >= ?");
    }
    if params.to.is_some() {
        where_clause.push_str(" AND start_time_unix_nano <= ?");
    }
    if params.service.is_some() {
        where_clause.push_str(" AND service_name = ?");
    }
    where_clause
}
```

- [ ] **Step 7: Update `TopologyParams` test fixtures in `planner/mod.rs`**

The planner tests construct `TopologyParams` with `lookback_minutes: None`. Since the field no longer exists, remove it from every occurrence. Search for `lookback_minutes: None` inside the `#[cfg(test)]` block of `planner/mod.rs` and remove those lines. The remaining fields (`environment`, `service`) are unchanged.

For example, `topology_plan_includes_tenant_and_time_filters` becomes:
```rust
let params = TopologyParams {
    environment: None,
    from: None,
    to: None,
    service: None,
};
```

Apply the same pattern to every `TopologyParams { ... }` constructor in the test block.

- [ ] **Step 8: Update `SearchParams` test fixtures in `traces.rs`**

Search for `lookback_minutes: None` in the `#[cfg(test)]` block of `traces.rs` (around lines 543, 564) and remove those lines from the struct literals.

- [ ] **Step 9: Run the tests**

```bash
cd C:/git/Observable/services/query-api
cargo test
```

Expected: all tests pass.

- [ ] **Step 10: Commit**

```bash
git add services/query-api/src/traces.rs services/query-api/src/planner/mod.rs
git commit -m "feat: remove lookback_minutes from trace SearchParams and planner"
```

---

## Task 5: Frontend — services.ts

**Files:**
- Modify: `apps/frontend/src/api/services.ts`

- [ ] **Step 1: Write the failing test**

Open `apps/frontend/src/api/services.test.ts` if it exists, otherwise create it. Add:

```typescript
import { describe, it, expect, vi, beforeEach } from "vitest";
import { listServiceSummaries, getServiceSummary, getTopology, getServiceResponseTimeHistory } from "./services";

describe("services API from/to params", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ items: [] }),
    }));
  });

  it("listServiceSummaries sends from and to as ISO strings", async () => {
    await listServiceSummaries({ from: 1_000_000, to: 3_600_000 });
    const url = new URL((vi.mocked(fetch).mock.calls[0][0] as string));
    expect(url.searchParams.get("from")).toBe(new Date(1_000_000).toISOString());
    expect(url.searchParams.get("to")).toBe(new Date(3_600_000).toISOString());
    expect(url.searchParams.has("lookback_minutes")).toBe(false);
  });

  it("getServiceSummary sends from and to as ISO strings", async () => {
    vi.mocked(fetch).mockResolvedValueOnce({
      ok: true,
      json: async () => ({ service: {} }),
    } as Response);
    await getServiceSummary("checkout", { from: 1_000_000, to: 3_600_000 });
    const url = new URL((vi.mocked(fetch).mock.calls[0][0] as string));
    expect(url.searchParams.get("from")).toBe(new Date(1_000_000).toISOString());
    expect(url.searchParams.get("to")).toBe(new Date(3_600_000).toISOString());
    expect(url.searchParams.has("lookback_minutes")).toBe(false);
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd C:/git/Observable/apps/frontend
npm test -- services.test
```

Expected: FAIL — `listServiceSummaries` and `getServiceSummary` don't accept `from`/`to` yet.

- [ ] **Step 3: Update `services.ts`**

Replace the parameter type and URL construction on all four functions. Helper to serialize timestamps:

```typescript
function msToIso(ms?: number): string | undefined {
  return ms != null ? new Date(ms).toISOString() : undefined;
}
```

**`listServiceSummaries`:**
```typescript
export async function listServiceSummaries(params: {
  environment?: string;
  from?: number;
  to?: number;
} = {}): Promise<ServiceSummaryResponse> {
  const url = new URL("/v1/services/summary", window.location.origin);
  if (params.environment) url.searchParams.set("environment", params.environment);
  const fromIso = msToIso(params.from);
  const toIso = msToIso(params.to);
  if (fromIso) url.searchParams.set("from", fromIso);
  if (toIso) url.searchParams.set("to", toIso);
  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}
```

**`getServiceSummary`:**
```typescript
export async function getServiceSummary(
  serviceName: string,
  params: {
    environment?: string;
    from?: number;
    to?: number;
  } = {},
): Promise<ServiceDetailResponse> {
  const encodedService = encodeURIComponent(serviceName);
  const url = new URL(`/v1/services/${encodedService}/summary`, window.location.origin);
  if (params.environment) url.searchParams.set("environment", params.environment);
  const fromIso = msToIso(params.from);
  const toIso = msToIso(params.to);
  if (fromIso) url.searchParams.set("from", fromIso);
  if (toIso) url.searchParams.set("to", toIso);
  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}
```

**`getTopology`:**
```typescript
export async function getTopology(params: {
  environment?: string;
  from?: number;
  to?: number;
  service?: string;
} = {}): Promise<TopologyResponse> {
  const url = new URL("/v1/topology", window.location.origin);
  if (params.environment) url.searchParams.set("environment", params.environment);
  const fromIso = msToIso(params.from);
  const toIso = msToIso(params.to);
  if (fromIso) url.searchParams.set("from", fromIso);
  if (toIso) url.searchParams.set("to", toIso);
  if (params.service) url.searchParams.set("service", params.service);
  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}
```

**`getServiceResponseTimeHistory`:**
```typescript
export async function getServiceResponseTimeHistory(
  serviceName: string,
  params: {
    from?: number;
    to?: number;
    buckets?: number;
  } = {},
): Promise<ResponseTimeHistoryResponse> {
  const encodedService = encodeURIComponent(serviceName);
  const url = new URL(
    `/v1/services/${encodedService}/response-time-history`,
    window.location.origin,
  );
  const fromIso = msToIso(params.from);
  const toIso = msToIso(params.to);
  if (fromIso) url.searchParams.set("from", fromIso);
  if (toIso) url.searchParams.set("to", toIso);
  if (params.buckets) url.searchParams.set("buckets", String(params.buckets));
  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}
```

- [ ] **Step 4: Run tests and verify they pass**

```bash
cd C:/git/Observable/apps/frontend
npm test -- services.test
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/api/services.ts apps/frontend/src/api/services.test.ts
git commit -m "feat: replace lookback_minutes with from/to in services API"
```

---

## Task 6: Frontend — traces.ts + setup.ts

**Files:**
- Modify: `apps/frontend/src/api/traces.ts`
- Modify: `apps/frontend/src/api/setup.ts`

- [ ] **Step 1: Remove `lookback_minutes` from `searchTraces` in `traces.ts`**

In `searchTraces`, remove `lookback_minutes?: number` from the params and remove its URL serialization:

```typescript
export async function searchTraces(params: {
  service?: string;
  limit?: number;
  facets?: string[];
  from?: string;
  to?: string;
}): Promise<TraceListResponse> {
  const url = new URL("/v1/traces", window.location.origin);
  if (params.service) url.searchParams.set("service", params.service);
  if (params.limit) url.searchParams.set("limit", String(params.limit));
  if (params.facets) url.searchParams.set("facets", params.facets.join(","));
  if (params.from) url.searchParams.set("from", params.from);
  if (params.to) url.searchParams.set("to", params.to);

  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}
```

- [ ] **Step 2: Update the `searchTraces` call in `setup.ts`**

In `setup.ts` line 20, replace `lookback_minutes: 60` with `from`/`to` ISO strings:

```typescript
searchTraces({
  from: new Date(Date.now() - 60 * 60 * 1000).toISOString(),
  to: new Date().toISOString(),
  limit: 1,
}),
```

- [ ] **Step 3: Run tests**

```bash
cd C:/git/Observable/apps/frontend
npm test
```

Expected: tests that called `searchTraces` without type errors still pass.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/api/traces.ts apps/frontend/src/api/setup.ts
git commit -m "feat: remove lookback_minutes from searchTraces, update setup.ts"
```

---

## Task 7: Frontend — dashboards.ts

**Files:**
- Modify: `apps/frontend/src/api/dashboards.ts`

- [ ] **Step 1: Update `DashboardPanel` type**

Add the `Preset` import and replace `lookback_minutes`:

```typescript
import type { Preset } from "../router";

export interface DashboardPanel {
  panel_id: string;
  title: string;
  query_kind: DashboardQueryKind;
  service?: string | null;
  preset: Preset | null;
  filters: Record<string, unknown>;
}
```

- [ ] **Step 2: Update `CreateDashboardRequest` panel shape**

```typescript
export interface CreateDashboardRequest {
  name: string;
  panels: Array<{
    title: string;
    query_kind: DashboardQueryKind;
    service?: string;
    preset: Preset | null;
    filters: Record<string, unknown>;
  }>;
}
```

- [ ] **Step 3: Run the TypeScript compiler**

```bash
cd C:/git/Observable/apps/frontend
npx tsc --noEmit
```

Expected: type errors in call sites (pages) — those are fixed in Task 8.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/api/dashboards.ts
git commit -m "feat: replace lookback_minutes with preset in dashboards API type"
```

---

## Task 8: Frontend — Page Call Sites

**Files:**
- Modify: `apps/frontend/src/pages/ServiceDetailPage.tsx`
- Modify: `apps/frontend/src/pages/LogSearch.tsx`
- Modify: `apps/frontend/src/pages/TraceSearch.tsx`
- Modify: `apps/frontend/src/pages/ServiceTopologyPage.tsx`
- Modify: `apps/frontend/src/pages/DashboardsPage.tsx`

- [ ] **Step 1: Update `ServiceDetailPage.tsx`**

In the `ServiceDetailPage` component (around line 37), remove the `lookback_minutes` conversion:

```typescript
const { data, isLoading, isError } = useQuery({
  queryKey: ["service-summary", serviceName, fromMs, toMs],
  queryFn: () => getServiceSummary(serviceName, { from: fromMs, to: toMs }),
});
```

In `ResponseTimeGraphSection` (around line 279), remove `lookbackMinutes` and pass `from`/`to` directly:

```typescript
const { data: historyData } = useQuery({
  queryKey: ["service-response-time", serviceName, fromMs, toMs],
  queryFn: () =>
    getServiceResponseTimeHistory(serviceName, {
      from: fromMs,
      to: toMs,
      buckets: 60,
    }),
});
```

- [ ] **Step 2: Update `LogSearch.tsx` promote handler**

In `handlePromote` (around line 98), replace `lookback_minutes` with `preset: null`:

```typescript
await createDashboard({
  name: service ? `Logs for ${service}` : "Promoted log query",
  panels: [
    {
      title: service ? `Logs for ${service}` : "Log search",
      query_kind: "logs",
      service: service || undefined,
      preset: null,
      filters: { facets: ["service_name", "severity_number", "environment", "host_id"] },
    },
  ],
});
```

- [ ] **Step 3: Update `TraceSearch.tsx` promote handler**

In `handlePromote` (around line 106), same change:

```typescript
await createDashboard({
  name: service ? `Traces for ${service}` : "Promoted trace query",
  panels: [
    {
      title: service ? `Traces for ${service}` : "Trace search",
      query_kind: "traces",
      service: service || undefined,
      preset: null,
      filters: { facets: ["service_name", "status_code", "span_kind"] },
    },
  ],
});
```

- [ ] **Step 4: Update `ServiceTopologyPage.tsx` popover links**

Remove `&lookback_minutes=60` from the hardcoded href values (around lines 100-106). The global date range in the URL already handles time selection for those pages:

```tsx
<a href={`/traces?caller=${encodeURIComponent(edgePopover.edge.caller)}&callee=${encodeURIComponent(edgePopover.edge.callee)}`}>
  View Traces
</a>
<a href={`/logs?service=${encodeURIComponent(edgePopover.edge.caller)}`}>
  View Logs
</a>
```

- [ ] **Step 5: Update `DashboardsPage.tsx` panel card**

Replace `Last {panel.lookback_minutes}m` with a label that handles both `null` (global range) and a preset string. Import `PRESET_OPTIONS` from the hook:

```typescript
import { PRESET_OPTIONS } from "../hooks/useGlobalDateRange";

function DashboardPanelCard({ panel }: { panel: DashboardPanel }) {
  const service = panel.service || "all services";
  const timeLabel = panel.preset
    ? (PRESET_OPTIONS.find((o) => o.value === panel.preset)?.label ?? panel.preset)
    : "Global date range";
  return (
    <div className="border border-[var(--border)] bg-[var(--surface)] p-4">
      <div className="text-xs font-bold uppercase text-[var(--muted)]">
        {panel.query_kind} · {service} · {timeLabel}
      </div>
      <h2 className="mt-2 mb-0 text-base font-bold text-[var(--text-strong)]">{panel.title}</h2>
    </div>
  );
}
```

- [ ] **Step 6: Run the TypeScript compiler**

```bash
cd C:/git/Observable/apps/frontend
npx tsc --noEmit
```

Expected: no type errors.

- [ ] **Step 7: Run the test suite**

```bash
cd C:/git/Observable/apps/frontend
npm test
```

Expected: tests pass (App.test.tsx will fail — fixed in Task 9).

- [ ] **Step 8: Commit**

```bash
git add apps/frontend/src/pages/ServiceDetailPage.tsx \
  apps/frontend/src/pages/LogSearch.tsx \
  apps/frontend/src/pages/TraceSearch.tsx \
  apps/frontend/src/pages/ServiceTopologyPage.tsx \
  apps/frontend/src/pages/DashboardsPage.tsx
git commit -m "feat: migrate page call sites away from lookback_minutes"
```

---

## Task 9: Frontend — Update App.test.tsx

**Files:**
- Modify: `apps/frontend/src/App.test.tsx`

- [ ] **Step 1: Update dashboard POST mock assertions**

Find the two places (around lines 83 and 163) where `lookback_minutes: 60` is asserted in the dashboard POST body. Replace with `preset: null`:

```typescript
expect(body.panels[0]).toMatchObject({
  query_kind: "logs",
  service: "checkout",
  preset: null,
});
```

```typescript
expect(body.panels[0]).toMatchObject({
  query_kind: "traces",
  service: "checkout",
  preset: null,
});
```

- [ ] **Step 2: Update dashboard GET mock response**

Around line 212, the mocked `GET /v1/dashboards` response contains `lookback_minutes: 60`. Replace with `preset: "1h"`:

```typescript
panels: [
  {
    panel_id: "panel-1",
    title: "Logs for checkout",
    query_kind: "logs",
    service: "checkout",
    preset: "1h",
    filters: { facets: ["service_name", "severity_number"] },
  },
],
```

- [ ] **Step 3: Update topology popover link assertions**

Around lines 1054-1057, update the expected href values:

```typescript
expect(tracesLink).toHaveAttribute(
  "href",
  "/traces?caller=checkout-api&callee=payments-api",
);
expect(logsLink).toHaveAttribute("href", "/logs?service=checkout-api");
```

- [ ] **Step 4: Run the full test suite**

```bash
cd C:/git/Observable/apps/frontend
npm test
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/App.test.tsx
git commit -m "test: update App.test.tsx fixtures for preset migration"
```

---

## Self-Review

**Spec coverage check:**
- ✅ DB migration: Task 1
- ✅ Backend dashboards.rs preset: Task 2
- ✅ Backend discovery.rs SummaryParams/TopologyParams/ResponseTimeHistoryParams: Task 3
- ✅ Backend traces.rs remove lookback_minutes: Task 4
- ✅ Frontend services.ts from/to: Task 5
- ✅ Frontend traces.ts remove lookback_minutes: Task 6
- ✅ Frontend dashboards.ts preset type: Task 7
- ✅ Frontend pages (ServiceDetailPage, LogSearch, TraceSearch, ServiceTopologyPage, DashboardsPage): Task 8
- ✅ E2E test fixtures: Task 9
- ✅ Dashboard hybrid panel logic (DashboardsPage uses preset or global range label): Task 8 Step 5

**Notes for implementer:**
- The `lookback_minutes=60` in the Metrics navigation link (`ServiceDetailPage.tsx` line 143) is intentionally left unchanged — the metrics page has its own time controls and is out of scope for this migration.
- `InfrastructureInventoryParams` and `InfrastructureDetailParams` in `discovery.rs` also contain `lookback_minutes`, but they are served by a different API surface (infrastructure endpoints) that has no frontend migration in scope. Leave them unchanged.
- If `cargo test` fails with a migration-related error about `lookback_minutes` column not existing, ensure Task 1 ran successfully against the test database before running Tasks 2–4.
