# Dashboards Domain: Modelable Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `models/dashboards.mdl` defining `dashboards.Dashboard@1` (entity) plus `value DashboardPanel`/`value DashboardPanelLayout`, generate their TypeScript artifacts (with a one-line cross-model import patch each, due to a new TS-emitter gap), re-export them as `Dashboard`/`DashboardPanel`/`DashboardPanelLayout` from `apps/frontend/src/api/dashboards.ts`, fix the resulting `preset`/`filters`/`time_range`/`content`/`service`/`query_kind`/`query_text`/`visibility` type-fallout in `DashboardDetailPage.tsx`/`DashboardDetailPage.test.tsx`/`DashboardsPage.test.tsx`, and add lineage doc comments to `DashboardItem`/`DashboardPanelItem` in `services/query-api/src/dashboards.rs`. Mark Phase 3 step 3.8 done and record two new Phase 1 backlog items.

**Architecture:** This is a TS-only migration (Phase 3 step 3.8, scope "DashboardItem + DashboardPanelItem incl. nested layout" from the approved design), reusing the already-recorded Phase 1 backlog item 5 (modelable's `timestamp` primitive emits as Rust `String`, incompatible with `sqlx::FromRow`'s `chrono::DateTime<Utc>`). `GrantItem`, `DashboardListResponse`, `CreateDashboardRequest`, `UpdateDashboardRequest`, `DashboardExportPanel`/`DashboardExport` stay hand-written — see `docs/superpowers/specs/2026-06-14-dashboards-modelable-migration-design.md` for the full rationale. Two new gaps are recorded: backlog item 6 (modelable `enum(...)` identifiers can't start with digits, blocking a `Preset` enum) and backlog item 7 (TS emitter doesn't emit `import` statements for cross-model `NamedType` field references — `Dashboard.panels: DashboardPanel[]` and `DashboardPanel.layout: DashboardPanelLayout` both need a manually-added import line).

**Tech Stack:** modelable 0.4.0 (pinned in `models/requirements.txt`, no bump needed), TypeScript/Vite/Vitest frontend, Rust/Axum/sqlx backend.

---

## Task 1: Author `models/dashboards.mdl`

**Files:**
- Create: `models/dashboards.mdl`

- [ ] **Step 1: Create `models/dashboards.mdl`**

```
domain dashboards {
  owner: "platform-team"

  // Canonical dashboard panel layout. Mirrors the {x, y, w, h} shape of
  // services/query-api/src/dashboards.rs's DashboardPanelItem.layout
  // (serde_json::Value in Rust, always this shape in practice). See
  // docs/superpowers/specs/2026-06-14-dashboards-modelable-migration-design.md.
  @wire(json.fieldCase: "snake_case")
  value DashboardPanelLayout {
    x: int
    y: int
    w: int
    h: int
  }

  // Canonical dashboard panel. Mirrors
  // services/query-api/src/dashboards.rs's DashboardPanelItem field-for-field,
  // except: `preset` is `string` not `enum(...)` (Phase 1 backlog item 6 -
  // modelable enum members must be valid identifiers, but Preset values
  // 5m/15m/30m/1h/3h/12h start with digits), and `filters`/`time_range` are
  // `json` (no union/oneof construct for the 3-way DashboardPanelTimeRange
  // discriminated union). See
  // docs/superpowers/specs/2026-06-14-dashboards-modelable-migration-design.md.
  @wire(json.fieldCase: "snake_case")
  value DashboardPanel {
    panelId: uuid
    title: string
    panelKind: enum(query, text)
    queryKind?: enum(logs, traces, metrics)
    service?: string
    preset?: string
    filters: json
    queryText?: string
    content?: string
    layout: DashboardPanelLayout
    timeRange: json
  }

  // Canonical dashboard summary entity. Mirrors
  // services/query-api/src/dashboards.rs's DashboardItem field-for-field.
  // GrantItem, DashboardRow/DashboardPanelRow (Postgres sqlx::FromRow
  // projections), DashboardListResponse, CreateDashboardRequest,
  // UpdateDashboardRequest, and DashboardExportPanel/DashboardExport (export
  // file format) are NOT modeled here — see
  // docs/superpowers/specs/2026-06-14-dashboards-modelable-migration-design.md
  // for why (timestamp -> Rust String gap, Phase 1 backlog item 5).
  @wire(json.fieldCase: "snake_case")
  entity Dashboard @ 1 (additive) {
    @key dashboardId: uuid
    name: string
    visibility: enum(public, private)
    panels: array<DashboardPanel>
    createdAt: timestamp
  }
}
```

- [ ] **Step 2: Validate**

This validates the whole `models/` workspace (`logs.mdl`, `tracing.mdl`, `metrics.mdl`, `notifications.mdl`, `admin.mdl`, `slos.mdl`, `incidents.mdl`, `alerts.mdl`, `dashboards.mdl`, `requirements.txt`):

```bash
cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable validate C:\git\Observable\models
```

Expected: `OK 9 files valid.` (confirms `dashboards.mdl` parses and has no semantic errors alongside the existing files).

- [ ] **Step 3: Commit**

```bash
git add models/dashboards.mdl
git commit -m "feat(models): author dashboards.mdl (Dashboard, DashboardPanel, DashboardPanelLayout)"
```

---

## Task 2: Generate and wire TypeScript `Dashboard`/`DashboardPanel`/`DashboardPanelLayout`

**Files:**
- Create: `apps/frontend/src/api/generated/dashboards/dashboards.Dashboard.v1.ts`
- Create: `apps/frontend/src/api/generated/dashboards/dashboards.DashboardPanel.v0.ts`
- Create: `apps/frontend/src/api/generated/dashboards/dashboards.DashboardPanelLayout.v0.ts`
- Modify: `apps/frontend/src/api/dashboards.ts:7-35`

- [ ] **Step 1: Generate the TypeScript artifacts**

`dashboards.mdl` has no `projection`/`binding` blocks, so it compiles standalone without hitting the duplicate-`binding`-name issue (Phase 1 backlog item 4). Generate it in an isolated scratch workspace:

```bash
mkdir -p /c/tmp/dashboards-gen/models
cp /c/git/Observable/models/dashboards.mdl /c/git/Observable/models/requirements.txt /c/tmp/dashboards-gen/models/
cd /c/tmp/dashboards-gen && /c/git/modelable/cli/.venv/Scripts/python.exe -m modelable compile models --target typescript --out ts
```

Expected: `OK ts\dashboards.DashboardPanelLayout.v0.ts`, `OK ts\dashboards.DashboardPanel.v0.ts`, `OK ts\dashboards.Dashboard.v1.ts`, each with a content hash line, plus two `WARN [EMIT003] Missing metadata required by target: dashboards.Dashboard.panels` / `dashboards.DashboardPanel.layout` warnings — these are the cross-model-import gap (Phase 1 backlog item 7), expected and handled by Steps 2-4 below. If any field fails to compile (not just the two expected `EMIT003` warnings), read the error, consult `C:\git\modelable\cli\src\modelable\emitters\typescript.py` for the supported syntax, and adjust `models/dashboards.mdl` (back in Task 1) accordingly before proceeding — re-run Task 1 Step 2 (`validate`) after any change.

- [ ] **Step 2: Create `apps/frontend/src/api/generated/dashboards/dashboards.DashboardPanelLayout.v0.ts`**

Copy the generated file verbatim, prefixed with the same regen-header convention used by `apps/frontend/src/api/generated/incidents/incidents.Incident.v1.ts`. This file has no cross-model references, so it needs no import patch:

```typescript
// Generated by modelable — do not edit by hand.
// Regenerate: copy models/dashboards.mdl and models/requirements.txt into
// an isolated scratch workspace (dashboards.mdl has no bindings/projections,
// so it compiles standalone — see Phase 1 backlog item 4 for why the full
// C:\git\Observable\models directory currently cannot be compiled from
// scratch), then run:
//   cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable compile <scratch-workspace>\models --target typescript --out <scratch-out>
// then copy dashboards.DashboardPanelLayout.v0.ts into this directory.
/**
 * @modelable domain: dashboards
 * @modelable name: DashboardPanelLayout
 * @modelable owner: platform-team
 * @modelable kind: value
 * @modelable version: 0
 * @modelable changeKind: additive
 */
export interface DashboardsDashboardPanelLayoutV0 {
  x: number;
  y: number;
  w: number;
  h: number;
}
export type DashboardPanelLayout = DashboardsDashboardPanelLayoutV0;
```

If the actual generated output differs from the body shown above (field order, optional-field syntax, quote style, etc.), use the actual generated output instead — only the header comment block is fixed by convention.

- [ ] **Step 3: Create `apps/frontend/src/api/generated/dashboards/dashboards.DashboardPanel.v0.ts`**

Same regen-header convention, plus a manually-added `import type` line for `DashboardPanelLayout` (Phase 1 backlog item 7 — the TS emitter omits this import; the header comment documents the manual step needed on every regen):

```typescript
// Generated by modelable — do not edit by hand.
// Regenerate: copy models/dashboards.mdl and models/requirements.txt into
// an isolated scratch workspace (dashboards.mdl has no bindings/projections,
// so it compiles standalone — see Phase 1 backlog item 4 for why the full
// C:\git\Observable\models directory currently cannot be compiled from
// scratch), then run:
//   cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable compile <scratch-workspace>\models --target typescript --out <scratch-out>
// then copy dashboards.DashboardPanel.v0.ts into this directory.
// MANUAL PATCH (Phase 1 backlog item 7): the TS emitter does not emit an
// import for the `layout: DashboardPanelLayout` field (NamedType reference).
// Re-add the import line below after every regen.
/**
 * @modelable domain: dashboards
 * @modelable name: DashboardPanel
 * @modelable owner: platform-team
 * @modelable kind: value
 * @modelable version: 0
 * @modelable changeKind: additive
 */
import type { DashboardPanelLayout } from "./dashboards.DashboardPanelLayout.v0";
export interface DashboardsDashboardPanelV0 {
  panel_id: string;
  title: string;
  panel_kind: 'query' | 'text';
  query_kind?: 'logs' | 'traces' | 'metrics';
  service?: string;
  preset?: string;
  filters: unknown;
  query_text?: string;
  content?: string;
  layout: DashboardPanelLayout;
  time_range: unknown;
}
export type DashboardPanel = DashboardsDashboardPanelV0;
```

If the actual generated output differs from the body shown above, use the actual generated output instead for the interface body — only the header comment block and the added `import type` line are fixed by convention.

- [ ] **Step 4: Create `apps/frontend/src/api/generated/dashboards/dashboards.Dashboard.v1.ts`**

Same regen-header convention, plus a manually-added `import type` line for `DashboardPanel`:

```typescript
// Generated by modelable — do not edit by hand.
// Regenerate: copy models/dashboards.mdl and models/requirements.txt into
// an isolated scratch workspace (dashboards.mdl has no bindings/projections,
// so it compiles standalone — see Phase 1 backlog item 4 for why the full
// C:\git\Observable\models directory currently cannot be compiled from
// scratch), then run:
//   cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable compile <scratch-workspace>\models --target typescript --out <scratch-out>
// then copy dashboards.Dashboard.v1.ts into this directory.
// MANUAL PATCH (Phase 1 backlog item 7): the TS emitter does not emit an
// import for the `panels: array<DashboardPanel>` field (NamedType reference).
// Re-add the import line below after every regen.
/**
 * @modelable domain: dashboards
 * @modelable name: Dashboard
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
import type { DashboardPanel } from "./dashboards.DashboardPanel.v0";
export interface DashboardsDashboardV1 {
  dashboard_id: string;
  name: string;
  visibility: 'public' | 'private';
  panels: DashboardPanel[];
  created_at: string;
}
export type Dashboard = DashboardsDashboardV1;
```

If the actual generated output differs from the body shown above, use the actual generated output instead for the interface body — only the header comment block and the added `import type` line are fixed by convention.

- [ ] **Step 5: Wire the re-exports into `apps/frontend/src/api/dashboards.ts`**

Current `apps/frontend/src/api/dashboards.ts:1-35`:

```typescript
import type { Preset } from "../router";

function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

export type DashboardQueryKind = "logs" | "traces" | "metrics";
export type DashboardPanelKind = "query" | "text";
export type DashboardPanelLayout = { x: number; y: number; w: number; h: number };
export type DashboardPanelTimeRange =
  | { mode: "global" }
  | { mode: "preset"; preset: Preset }
  | { mode: "absolute"; from_ms: number; to_ms: number };

export interface DashboardPanel {
  panel_id: string;
  title: string;
  panel_kind: DashboardPanelKind;
  query_kind: DashboardQueryKind | null;
  service?: string | null;
  preset: Preset | null;
  filters: Record<string, unknown>;
  query_text?: string | null;
  content?: string | null;
  layout: DashboardPanelLayout;
  time_range: DashboardPanelTimeRange;
}

export interface Dashboard {
  dashboard_id: string;
  name: string;
  panels: DashboardPanel[];
  created_at: string;
}
```

Replace with:

```typescript
import type { Preset } from "../router";
import type { DashboardPanelLayout } from "./generated/dashboards/dashboards.DashboardPanelLayout.v0";
import type { DashboardPanel } from "./generated/dashboards/dashboards.DashboardPanel.v0";
import type { Dashboard } from "./generated/dashboards/dashboards.Dashboard.v1";

function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

export type DashboardQueryKind = "logs" | "traces" | "metrics";
export type DashboardPanelKind = "query" | "text";
export type { DashboardPanelLayout };
export type DashboardPanelTimeRange =
  | { mode: "global" }
  | { mode: "preset"; preset: Preset }
  | { mode: "absolute"; from_ms: number; to_ms: number };

export type { DashboardPanel };

export type { Dashboard };
```

`DashboardQueryKind`/`DashboardPanelKind` stay as hand-written literal-union aliases — they're structurally identical to the generated `DashboardPanel.query_kind`/`panel_kind` fields' literal types, and remain useful as named types for component props (`PanelTemplateLibrary.tsx`, `AddPanelForm`/`EditPanelForm` in `DashboardDetailPage.tsx`). `DashboardPanelTimeRange` stays hand-written (no union/oneof construct in modelable — backlog item, already recorded as accepted fallout in the design). The rest of `dashboards.ts` (`DashboardListResponse`, `CreateDashboardRequest`, `UpdateDashboardRequest`, `DashboardExportPanel`, `DashboardExport`, and all API functions) is unchanged — `DashboardListResponse.items: Dashboard[]` etc. now reference the generated types transitively.

- [ ] **Step 6: Typecheck**

```bash
cd apps/frontend && npm run typecheck
```

Expected: errors in `apps/frontend/src/pages/DashboardDetailPage.tsx`, `apps/frontend/src/pages/DashboardDetailPage.test.tsx`, and `apps/frontend/src/pages/DashboardsPage.test.tsx` due to: (a) `preset`/`service`/`query_kind`/`query_text`/`content` changing from `T | null` to `?: T`, (b) `filters`/`time_range` changing from typed shapes to `unknown`, and (c) the new required `visibility` field on `Dashboard`. These are fixed in Task 3. Do not fix them in this task; just confirm the errors are limited to those three files (and no others).

- [ ] **Step 7: Commit**

```bash
git add apps/frontend/src/api/generated/dashboards apps/frontend/src/api/dashboards.ts
git commit -m "feat(frontend): generate Dashboard/DashboardPanel/DashboardPanelLayout from dashboards.mdl"
```

---

## Task 3: Fix `DashboardDetailPage.tsx`/`DashboardDetailPage.test.tsx`/`DashboardsPage.test.tsx` type fallout

**Files:**
- Modify: `apps/frontend/src/pages/DashboardDetailPage.tsx:7-15`, `apps/frontend/src/pages/DashboardDetailPage.tsx:46-60`
- Modify: `apps/frontend/src/pages/DashboardDetailPage.test.tsx:90-122`, `apps/frontend/src/pages/DashboardDetailPage.test.tsx:354-366`
- Modify: `apps/frontend/src/pages/DashboardsPage.test.tsx:11-30`

- [ ] **Step 1: Import `Preset` and fix `panelToUpdate` in `DashboardDetailPage.tsx`**

Current `apps/frontend/src/pages/DashboardDetailPage.tsx:7-15`:

```typescript
import {
  getDashboard,
  updateDashboard,
  type DashboardPanel,
  type DashboardPanelKind,
  type DashboardPanelTimeRange,
  type DashboardQueryKind,
  type UpdateDashboardRequest,
} from "../api/dashboards";
```

Replace with:

```typescript
import {
  getDashboard,
  updateDashboard,
  type DashboardPanel,
  type DashboardPanelKind,
  type DashboardPanelTimeRange,
  type DashboardQueryKind,
  type UpdateDashboardRequest,
} from "../api/dashboards";
import type { Preset } from "../router";
```

Current `apps/frontend/src/pages/DashboardDetailPage.tsx:46-60`:

```typescript
function panelToUpdate(panel: DashboardPanel): UpdateDashboardRequest["panels"][number] {
  return {
    panel_id: panel.panel_id,
    title: panel.title,
    panel_kind: panel.panel_kind,
    query_kind: panel.query_kind,
    service: panel.service,
    preset: panel.preset,
    filters: panel.filters,
    query_text: panel.query_text,
    content: panel.content,
    layout: panel.layout,
    time_range: panel.time_range,
  };
}
```

`panel.query_kind`/`panel.service`/`panel.query_text`/`panel.content` are now `T | undefined` (was `T | null`), and `UpdateDashboardRequest["panels"][number]`'s corresponding fields are `T | null` (`query_kind`/`preset`) or `T | null | undefined` (`service?: string | null`, `query_text?: string | null`, `content?: string | null` — optional fields, `undefined` is assignable). `query_kind: T | undefined` is NOT assignable to `query_kind: T | null` (the request type's `query_kind` field is non-optional `DashboardQueryKind | null`), so it needs the same `?? null` treatment as `preset`. `panel.filters`/`panel.time_range` are now `unknown` and need casts. Replace with:

```typescript
function panelToUpdate(panel: DashboardPanel): UpdateDashboardRequest["panels"][number] {
  return {
    panel_id: panel.panel_id,
    title: panel.title,
    panel_kind: panel.panel_kind,
    query_kind: panel.query_kind ?? null,
    service: panel.service,
    preset: (panel.preset as Preset | undefined) ?? null,
    filters: panel.filters as Record<string, unknown>,
    query_text: panel.query_text,
    content: panel.content,
    layout: panel.layout,
    time_range: panel.time_range as DashboardPanelTimeRange,
  };
}
```

- [ ] **Step 2: Fix `resolvePanelTimeRange`/`dashboardFiltersToNlqFilters` call sites in `DashboardDetailPage.tsx`**

Current `apps/frontend/src/pages/DashboardDetailPage.tsx:674`:

```typescript
  const resolved = resolvePanelTimeRange(panel.time_range, globalRange);
```

Replace with:

```typescript
  const resolved = resolvePanelTimeRange(panel.time_range as DashboardPanelTimeRange, globalRange);
```

Current `apps/frontend/src/pages/DashboardDetailPage.tsx:681`:

```typescript
    filters: signal === "metrics" ? dashboardFiltersToNlqFilters(panel.filters) : [],
```

Replace with:

```typescript
    filters: signal === "metrics" ? dashboardFiltersToNlqFilters(panel.filters as Record<string, unknown>) : [],
```

No other changes to `DashboardDetailPage.tsx` — `panel.query_kind ?? "logs"` (line 675), `(panel.query_kind as DashboardQueryKind) ?? "logs"` (line 485), `panel.content ?? ""` (line 659), `panel.query_text?.trim()` (line 676), `panel.layout.x/y/w/h` (lines 313-316, 64-65), `preset: null` (lines 161, 183, in `UpdateDashboardRequest["panels"][number]` object literals, unaffected) all behave identically with `T | undefined` and need no changes.

- [ ] **Step 3: Fix the `dashboard` fixture in `DashboardDetailPage.test.tsx`**

Current `apps/frontend/src/pages/DashboardDetailPage.test.tsx:90-122`:

```typescript
const dashboard: dashboardsApi.Dashboard = {
  dashboard_id: "dash-1",
  name: "Checkout Health",
  created_at: "2026-05-10T00:00:00Z",
  panels: [
    {
      panel_id: "query-1",
      title: "Error logs",
      panel_kind: "query",
      query_kind: "logs",
      service: "checkout",
      preset: null,
      filters: {},
      query_text: "errors in checkout",
      content: null,
      layout: { x: 0, y: 0, w: 6, h: 4 },
      time_range: { mode: "global" },
    },
    {
      panel_id: "text-1",
      title: "Incident notes",
      panel_kind: "text",
      query_kind: null,
      service: null,
      preset: null,
      filters: {},
      query_text: null,
      content: "Escalate after deploy verification.",
      layout: { x: 6, y: 0, w: 6, h: 2 },
      time_range: { mode: "global" },
    },
  ],
};
```

Replace with (add `visibility: "private"`; drop all `: null` fields — `preset`/`content` on the first panel, `query_kind`/`service`/`preset`/`query_text` on the second):

```typescript
const dashboard: dashboardsApi.Dashboard = {
  dashboard_id: "dash-1",
  name: "Checkout Health",
  visibility: "private",
  created_at: "2026-05-10T00:00:00Z",
  panels: [
    {
      panel_id: "query-1",
      title: "Error logs",
      panel_kind: "query",
      query_kind: "logs",
      service: "checkout",
      filters: {},
      query_text: "errors in checkout",
      layout: { x: 0, y: 0, w: 6, h: 4 },
      time_range: { mode: "global" },
    },
    {
      panel_id: "text-1",
      title: "Incident notes",
      panel_kind: "text",
      filters: {},
      content: "Escalate after deploy verification.",
      layout: { x: 6, y: 0, w: 6, h: 2 },
      time_range: { mode: "global" },
    },
  ],
};
```

- [ ] **Step 4: Fix the metrics-panel override in `DashboardDetailPage.test.tsx`**

Current `apps/frontend/src/pages/DashboardDetailPage.test.tsx:354-366`:

```typescript
test("metrics panels without query text execute a metric catalog base IR", async () => {
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue({
    ...dashboard,
    panels: [
      {
        ...dashboard.panels[0],
        query_kind: "metrics",
        query_text: null,
        service: null,
        filters: { name: "request", type: "histogram", environment: "prod" },
      },
    ],
  });
```

Replace with (drop `query_text`/`service` keys instead of setting them to `null` — `dashboard.panels[0]` no longer has these fields after Step 3, so this override removes them by omission; `DashboardPanel.query_text?`/`service?` being absent behaves the same as `undefined` for `panel.query_text?.trim()` and `signal === "metrics" ? ... : []`):

```typescript
test("metrics panels without query text execute a metric catalog base IR", async () => {
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue({
    ...dashboard,
    panels: [
      {
        ...dashboard.panels[0],
        query_kind: "metrics",
        query_text: undefined,
        service: undefined,
        filters: { name: "request", type: "histogram", environment: "prod" },
      },
    ],
  });
```

- [ ] **Step 5: Fix the `sampleDashboard` fixture in `DashboardsPage.test.tsx`**

Current `apps/frontend/src/pages/DashboardsPage.test.tsx:11-30`:

```typescript
const sampleDashboard: dashboardsApi.Dashboard = {
  dashboard_id: "dash-1",
  name: "My Dashboard",
  panels: [
    {
      panel_id: "panel-1",
      title: "Error Logs",
      panel_kind: "query",
      query_kind: "logs",
      service: "checkout",
      preset: "1h",
      filters: {},
      query_text: "errors in checkout",
      content: null,
      layout: { x: 0, y: 0, w: 6, h: 4 },
      time_range: { mode: "preset", preset: "1h" },
    },
  ],
  created_at: "2026-05-05T00:00:00Z",
};
```

Replace with (add `visibility: "private"`; drop `content: null,`):

```typescript
const sampleDashboard: dashboardsApi.Dashboard = {
  dashboard_id: "dash-1",
  name: "My Dashboard",
  visibility: "private",
  panels: [
    {
      panel_id: "panel-1",
      title: "Error Logs",
      panel_kind: "query",
      query_kind: "logs",
      service: "checkout",
      preset: "1h",
      filters: {},
      query_text: "errors in checkout",
      layout: { x: 0, y: 0, w: 6, h: 4 },
      time_range: { mode: "preset", preset: "1h" },
    },
  ],
  created_at: "2026-05-05T00:00:00Z",
};
```

- [ ] **Step 6: Typecheck and run frontend tests**

```bash
cd apps/frontend && npm run typecheck && npm test -- DashboardDetailPage DashboardsPage
```

Expected: typecheck passes with no errors; `DashboardDetailPage.test.tsx` and `DashboardsPage.test.tsx` tests pass.

- [ ] **Step 7: Commit**

```bash
git add apps/frontend/src/pages/DashboardDetailPage.tsx apps/frontend/src/pages/DashboardDetailPage.test.tsx apps/frontend/src/pages/DashboardsPage.test.tsx
git commit -m "fix(frontend): adapt dashboard pages to optional preset/filters/time_range/visibility"
```

---

## Task 4: Add lineage doc comments to `services/query-api/src/dashboards.rs`

**Files:**
- Modify: `services/query-api/src/dashboards.rs:16-17`, `services/query-api/src/dashboards.rs:31-32`

- [ ] **Step 1: Add a doc comment above `DashboardPanelItem`**

Before (`services/query-api/src/dashboards.rs:16-17`):

```rust
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct DashboardPanelItem {
```

After:

```rust
/// Canonical dashboard panel. Mirrors `dashboards.DashboardPanel` (value) in
/// `models/dashboards.mdl` field-for-field, except `preset` (Phase 1 backlog
/// item 6: modelable enum members must be valid identifiers, but Preset
/// values 5m/15m/30m/1h/3h/12h start with digits) and `filters`/`time_range`
/// (no union/oneof construct for the 3-way discriminated union) — see
/// `docs/superpowers/specs/2026-06-14-dashboards-modelable-migration-design.md`.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct DashboardPanelItem {
```

- [ ] **Step 2: Add a doc comment above `DashboardItem`**

Before (`services/query-api/src/dashboards.rs:31-32`):

```rust
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct DashboardItem {
```

After:

```rust
/// Canonical dashboard summary entity. Mirrors `dashboards.Dashboard@1` in
/// `models/dashboards.mdl` field-for-field (see
/// `docs/superpowers/specs/2026-06-14-dashboards-modelable-migration-design.md`).
/// `GrantItem`, `DashboardRow`/`DashboardPanelRow` (Postgres `sqlx::FromRow`
/// projections), `DashboardListResponse`, `CreateDashboardRequest`,
/// `UpdateDashboardRequest`, and `DashboardExportPanel`/`DashboardExport` are
/// NOT modeled — `created_at` stays `chrono::DateTime<Utc>` (Phase 1 backlog
/// item 5: modelable's `timestamp` emits as Rust `String`).
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct DashboardItem {
```

No other Rust changes — `GrantItem`, `DashboardRow`, `DashboardPanelRow`, `DashboardListResponse`, and all handlers/functions are untouched.

- [ ] **Step 3: Format and build-check**

```bash
cargo fmt --all
cd services/query-api && cargo check
```

Expected: no warnings/errors; `cargo fmt --all` should produce no diff beyond the new comments.

- [ ] **Step 4: Commit**

```bash
git add services/query-api/src/dashboards.rs
git commit -m "docs(query-api): cross-reference dashboards.mdl Dashboard@1/DashboardPanel"
```

---

## Task 5: Full verification, lineage proof, mark 3.8 done, record backlog items 6 and 7

**Files:**
- Modify: `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md:99`, `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md:104-138`

- [ ] **Step 1: Lineage proof**

```bash
mkdir -p /c/tmp/dashboards-gen/models
cp /c/git/Observable/models/dashboards.mdl /c/git/Observable/models/requirements.txt /c/tmp/dashboards-gen/models/
cd /c/tmp/dashboards-gen && /c/git/modelable/cli/.venv/Scripts/python.exe -m modelable lineage dashboards.Dashboard@1 --path models
```

Expected output:

```
dashboards.Dashboard@1
kind: entity
- dashboardId: uuid [key]
- name: string
- visibility: enum(public, private)
- panels: array<DashboardPanel>
- createdAt: timestamp
```

Save the output — it goes in the commit/PR description for Step 5 below. `DashboardPanel`/`DashboardPanelLayout` are unversioned `value` kinds — no lineage to prove for them.

- [ ] **Step 2: Frontend verification**

```bash
cd apps/frontend
npm run typecheck && npm run lint && npm test && npm run build
```

Expected: all pass with no new failures.

- [ ] **Step 3: Full local CI**

```bash
bash scripts/local-ci.sh
```

Expected: passes (frontend typecheck/lint/build/test, Rust fmt/clippy/unit tests, Docker image build, smoke test). If the Rust integration-test step fails with an LLVM/rustc OOM (`STATUS_STACK_BUFFER_OVERRUN`) as seen in 3.7, this is a pre-existing Windows resource-exhaustion issue unrelated to this migration (doc-comment-only Rust change) — verify with `cargo check -p query-api --all-targets` under `CARGO_BUILD_JOBS=1`, which should pass cleanly, and document the same caveat in the commit/PR description.

- [ ] **Step 4: Mark Phase 3 step 3.8 done and record backlog items 6/7**

In `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`, change line 99:

Before:

```markdown
- [ ] **3.8 Dashboards** — `services/query-api/src/dashboards.rs:17-67` (incl. `GrantItem` `sqlx::FromRow`), `apps/frontend/src/api/dashboards.ts:7-71`
```

After:

```markdown
- [x] **3.8 Dashboards** — Generated `dashboards.Dashboard@1` (entity) and `value DashboardPanel`/`DashboardPanelLayout` from `models/dashboards.mdl` (see `docs/superpowers/specs/2026-06-14-dashboards-modelable-migration-design.md`). `apps/frontend/src/api/dashboards.ts`'s `Dashboard`/`DashboardPanel`/`DashboardPanelLayout` are now re-exports of `apps/frontend/src/api/generated/dashboards/dashboards.Dashboard.v1.ts`/`dashboards.DashboardPanel.v0.ts`/`dashboards.DashboardPanelLayout.v0.ts`, including `enum(...)` for `panel_kind`/`query_kind`/`visibility`. `services/query-api/src/dashboards.rs`'s `DashboardItem`/`DashboardPanelItem` get lineage doc comments only — timestamp fields can't be generated (Phase 1 backlog item 5, same gap as 3.4-3.7). `preset`/`service`/`query_kind`/`query_text`/`content: T | null` -> `?: T`, plus `filters`/`time_range: json` -> `unknown`, required fixing `panelToUpdate`'s casts in `DashboardDetailPage.tsx` and dropping `null` fields from fixtures in `DashboardDetailPage.test.tsx`/`DashboardsPage.test.tsx`, plus adding the new required `visibility` field to those fixtures. `GrantItem` (zero frontend consumers), `DashboardListResponse`, `CreateDashboardRequest`, `UpdateDashboardRequest`, `DashboardExportPanel`/`DashboardExport` remain hand-written. New Phase 1 backlog items 6 (enum identifiers can't start with digits, blocks a `Preset` enum) and 7 (TS emitter doesn't emit imports for cross-model `NamedType` field references).
```

Then, in the same file's "## Phase 1 backlog (modelable gaps discovered during Phase 3)" section (currently ending at line 138 with item 5), append two new items after item 5:

```markdown
6. **`enum(...)` members must be valid identifiers — no numeric-prefixed string-literal unions.**
   The grammar (`cli/src/modelable/grammar/modelable.lark`: `enum_type: "enum" "(" IDENT ("," IDENT)*
   ")"`, `IDENT: /[A-Za-z_][A-Za-z0-9_-]*/`) requires every enum member to start with a letter or
   underscore. This blocks modeling TypeScript string-literal unions whose members start with a
   digit, e.g. `apps/frontend/src/router.ts`'s `Preset = "5m" | "15m" | "30m" | "1h" | "3h" | "12h"`
   (3.8's `DashboardPanel.preset`, modeled as `string` instead).
7. **TS emitter doesn't emit imports for cross-model `NamedType` field references.** When a field's
   type is a `NamedType` (a reference to another model in the same domain — e.g.
   `Dashboard.panels: array<DashboardPanel>` or `DashboardPanel.layout: DashboardPanelLayout`), the
   TypeScript emitter (`cli/src/modelable/emitters/typescript.py`, the `isinstance(field.type,
   NamedType)` branches around `missing_metadata(...)`) emits an `EMIT003 missing_metadata` warning
   and generates a field referencing the sibling type by name with no `import` statement — the
   generated `.ts` file doesn't compile standalone. Worked around in 3.8 by manually adding one
   `import type { X } from "./<domain>.X.v<N>"` line per affected file (documented in each file's
   header comment); the emitter should generate these imports automatically.
```

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md
git commit -m "docs: mark Phase 3 step 3.8 (Dashboards) done, record backlog items 6/7"
```

---

## Out of scope

- `GrantItem`, `AddGrantRequest`, `GrantListResponse`, `DashboardRow`, `DashboardPanelRow`, `DashboardListResponse`, `CreateDashboardRequest`, `UpdateDashboardRequest`, `DashboardExportPanel`, `DashboardExport` — remain entirely hand-written (see design doc Context/Non-Goals). `GrantItem` has zero frontend consumers (confirmed via grep for `Grant`/`grant` in `apps/frontend/src`) — deferred like 3.5b Schemas.
- Any change to `services/query-api/src/dashboards.rs` handler logic, SQL, or Postgres DDL/migrations.
- Bumping the modelable pin — `models/requirements.txt` is already `modelable==0.4.0`.
- Modeling `time_range` as a proper discriminated union, or `preset` as an `enum(...)` — both are recorded gaps (existing union/oneof gap, and new backlog item 6), accepted per the approved design.
- Fixing the modelable TS emitter gaps (backlog items 6, 7) themselves in `C:\git\modelable` — recorded as backlog items for a future modelable release.
- `apps/frontend/src/features/dashboards/PanelTemplateLibrary.tsx` — confirmed zero fallout (only references `DashboardPanelKind`/`DashboardQueryKind` type names, which stay hand-written and unchanged).
- `apps/frontend/src/pages/DashboardsPage.tsx` — confirmed zero fallout (no field-level reads of `.preset`/`.content`/`.query_kind`/`.service`/`.query_text`/`.visibility`).
