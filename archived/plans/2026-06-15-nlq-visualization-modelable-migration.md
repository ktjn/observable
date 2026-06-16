# NLQ/Visualization Domain: Modelable Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `models/nlq.mdl` defining `value NlqTimeRange`, `value NlqFilter`, `value FieldRole`, and `value NlqIr` (9 of its 13 fields), generate their TypeScript artifacts (with cross-model import patches, Phase 1 backlog item 7), and in `apps/frontend/src/api/nlq.ts` retype `VisualizationFrame`/`NlqIr`/`NlqIrResponse.ir` to use them — deriving `NlqOperation`/`FieldRoleKind` via indexed-access types and keeping `NlqSignal`/`NlqVisualizationHint`/`VisualizationFrameType` hand-written (two new gaps). Fix the resulting fallout in `NlqPanel.test.tsx`/`VisualizationPanel.test.tsx`, add lineage doc comments to `libs/domain/src/nlq.rs`/`visualization.rs`, mark Phase 3 step 3.9 done, and record two new Phase 1 backlog items.

**Architecture:** TS-only migration (Phase 3 step 3.9, the last regular Phase 3 domain — see `docs/superpowers/specs/2026-06-15-nlq-visualization-modelable-migration-design.md` for full rationale). Two new modelable gaps are recorded: backlog item 8 (`Option<T>` without `skip_serializing_if` — i.e. always-serialized `T | null` — can't be represented; modelable's `?` only means omittable) and backlog item 9 (`array<enum(...))` emits invalid TypeScript — `_type_to_ts` doesn't parenthesize the enum's union before appending `[]`). `NlqResponse` (no oneof/union construct), `NlqIrLike` (loose handler-local type), `envelope.rs` (no frontend consumer), and `mcp_tools.rs`'s `MetricSchema`/`SignalField` (folded into deferred 3.5b) remain hand-written/untouched.

**Tech Stack:** modelable 0.4.0 (pinned in `models/requirements.txt`, no bump needed), TypeScript/Vite/Vitest frontend, Rust/Axum backend.

---

## Task 1: Author `models/nlq.mdl`

**Files:**
- Create: `models/nlq.mdl`

- [ ] **Step 1: Create `models/nlq.mdl`**

```
domain nlq {
  owner: "platform-team"

  // Canonical NLQ query time range. Mirrors libs/domain/src/nlq.rs's
  // NlqTimeRange field-for-field. Shared by NlqIr.time_range and
  // VisualizationFrame.time_range. See
  // docs/superpowers/specs/2026-06-15-nlq-visualization-modelable-migration-design.md.
  @wire(json.fieldCase: "snake_case")
  value NlqTimeRange {
    from: string
    to: string
  }

  // Canonical NLQ field-level filter predicate. Mirrors
  // libs/domain/src/nlq.rs's NlqFilter, except `op` is `string` not
  // `enum(...)` — NlqFilterOp's variants serialize as comparison symbols
  // (=, !=, =~, !~, >, >=, <, <=), which are not valid modelable enum
  // identifiers (Phase 1 backlog item 6). See
  // docs/superpowers/specs/2026-06-15-nlq-visualization-modelable-migration-design.md.
  @wire(json.fieldCase: "snake_case")
  value NlqFilter {
    field: string
    op: string
    value: string
  }

  // Canonical visualization field-role annotation. Mirrors
  // libs/domain/src/visualization.rs's FieldRole field-for-field.
  @wire(json.fieldCase: "snake_case")
  value FieldRole {
    name: string
    role: enum(time, value, bucket, series, label)
  }

  // Canonical NLQ intermediate representation. Mirrors
  // libs/domain/src/nlq.rs's NlqIr, except `metric`/`window`/`resolution`/
  // `visualization_hint` are NOT modeled here (Phase 1 backlog item 8 -
  // these are Option<T> without skip_serializing_if, i.e. always-serialized
  // `T | null`, which modelable's `?` (omittable) cannot represent), and
  // `signals` is `array<string>` not `array<enum(...))` (Phase 1 backlog
  // item 9 - array<enum(...)> emits invalid TypeScript). Both are added back
  // in apps/frontend/src/api/nlq.ts via interface extension. See
  // docs/superpowers/specs/2026-06-15-nlq-visualization-modelable-migration-design.md.
  @wire(json.fieldCase: "snake_case")
  value NlqIr {
    operation: enum(timeseries, rate, irate, increase, histogram, topk, table, distribution, catalog, inventory)
    signals: array<string>
    filters: array<NlqFilter>
    groupBy: array<string>
    timeRange: NlqTimeRange
    percentiles?: array<string>
    catalogField?: string
    limit?: int
    query?: string
  }
}
```

- [ ] **Step 2: Validate**

This validates the whole `models/` workspace (`logs.mdl`, `tracing.mdl`, `metrics.mdl`, `notifications.mdl`, `admin.mdl`, `slos.mdl`, `incidents.mdl`, `alerts.mdl`, `dashboards.mdl`, `nlq.mdl`, `requirements.txt`):

```bash
cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable validate C:\git\Observable\models
```

Expected: `OK 10 files valid.` (confirms `nlq.mdl` parses and has no semantic errors alongside the existing files). If `array<string>` for `signals` or any other field fails validation, read the error, consult `C:\git\modelable\cli\src\modelable\grammar\modelable.lark` and `cli/src/modelable/emitters/typescript.py` for the supported syntax, and adjust `models/nlq.mdl` accordingly before proceeding.

- [ ] **Step 3: Commit**

```bash
git add models/nlq.mdl
git commit -m "feat(models): author nlq.mdl (NlqIr, NlqFilter, NlqTimeRange, FieldRole)"
```

---

## Task 2: Generate and wire TypeScript `NlqIr`/`NlqFilter`/`NlqTimeRange`/`FieldRole`

**Files:**
- Create: `apps/frontend/src/api/generated/nlq/nlq.NlqTimeRange.v0.ts`
- Create: `apps/frontend/src/api/generated/nlq/nlq.NlqFilter.v0.ts`
- Create: `apps/frontend/src/api/generated/nlq/nlq.FieldRole.v0.ts`
- Create: `apps/frontend/src/api/generated/nlq/nlq.NlqIr.v0.ts`
- Modify: `apps/frontend/src/api/nlq.ts:1-49`

- [ ] **Step 1: Generate the TypeScript artifacts**

`nlq.mdl` has no `projection`/`binding` blocks, so it compiles standalone without hitting the duplicate-`binding`-name issue (Phase 1 backlog item 4). Generate it in an isolated scratch workspace:

```bash
mkdir -p /c/tmp/nlq-gen/models
cp /c/git/Observable/models/nlq.mdl /c/git/Observable/models/requirements.txt /c/tmp/nlq-gen/models/
cd /c/tmp/nlq-gen && /c/git/modelable/cli/.venv/Scripts/python.exe -m modelable compile models --target typescript --out ts
```

Expected: `OK ts\nlq.NlqTimeRange.v0.ts`, `OK ts\nlq.NlqFilter.v0.ts`, `OK ts\nlq.FieldRole.v0.ts`, `OK ts\nlq.NlqIr.v0.ts`, each with a content hash line, plus two `WARN [EMIT003] Missing metadata required by target: nlq.NlqIr.filters` / `nlq.NlqIr.timeRange` warnings — these are the cross-model-import gap (Phase 1 backlog item 7), expected and handled by Step 5 below. If any field fails to compile (not just these two expected `EMIT003` warnings), read the error and adjust `models/nlq.mdl` (back in Task 1) accordingly — re-run Task 1 Step 2 (`validate`) after any change.

- [ ] **Step 2: Create `apps/frontend/src/api/generated/nlq/nlq.NlqTimeRange.v0.ts`**

Copy the generated file verbatim, prefixed with the standard regen-header convention used by `apps/frontend/src/api/generated/dashboards/dashboards.DashboardPanelLayout.v0.ts`. This file has no cross-model references, so it needs no import patch:

```typescript
// Generated by modelable — do not edit by hand.
// Regenerate: copy models/nlq.mdl and models/requirements.txt into an
// isolated scratch workspace (nlq.mdl has no bindings/projections, so it
// compiles standalone — see Phase 1 backlog item 4 for why the full
// C:\git\Observable\models directory currently cannot be compiled from
// scratch), then run:
//   cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable compile <scratch-workspace>\models --target typescript --out <scratch-out>
// then copy nlq.NlqTimeRange.v0.ts into this directory.
/**
 * @modelable domain: nlq
 * @modelable name: NlqTimeRange
 * @modelable owner: platform-team
 * @modelable kind: value
 * @modelable version: 0
 * @modelable changeKind: additive
 */
export interface NlqNlqTimeRangeV0 {
  from: string;
  to: string;
}
export type NlqTimeRange = NlqNlqTimeRangeV0;
```

If the actual generated output differs (field order, quote style, etc.), use the actual generated output instead — only the header comment block is fixed by convention.

- [ ] **Step 3: Create `apps/frontend/src/api/generated/nlq/nlq.NlqFilter.v0.ts`**

Same regen-header convention, no cross-model references:

```typescript
// Generated by modelable — do not edit by hand.
// Regenerate: copy models/nlq.mdl and models/requirements.txt into an
// isolated scratch workspace (nlq.mdl has no bindings/projections, so it
// compiles standalone — see Phase 1 backlog item 4 for why the full
// C:\git\Observable\models directory currently cannot be compiled from
// scratch), then run:
//   cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable compile <scratch-workspace>\models --target typescript --out <scratch-out>
// then copy nlq.NlqFilter.v0.ts into this directory.
/**
 * @modelable domain: nlq
 * @modelable name: NlqFilter
 * @modelable owner: platform-team
 * @modelable kind: value
 * @modelable version: 0
 * @modelable changeKind: additive
 */
export interface NlqNlqFilterV0 {
  field: string;
  op: string;
  value: string;
}
export type NlqFilter = NlqNlqFilterV0;
```

If the actual generated output differs, use the actual generated output instead — only the header comment block is fixed by convention.

- [ ] **Step 4: Create `apps/frontend/src/api/generated/nlq/nlq.FieldRole.v0.ts`**

Same regen-header convention, no cross-model references:

```typescript
// Generated by modelable — do not edit by hand.
// Regenerate: copy models/nlq.mdl and models/requirements.txt into an
// isolated scratch workspace (nlq.mdl has no bindings/projections, so it
// compiles standalone — see Phase 1 backlog item 4 for why the full
// C:\git\Observable\models directory currently cannot be compiled from
// scratch), then run:
//   cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable compile <scratch-workspace>\models --target typescript --out <scratch-out>
// then copy nlq.FieldRole.v0.ts into this directory.
/**
 * @modelable domain: nlq
 * @modelable name: FieldRole
 * @modelable owner: platform-team
 * @modelable kind: value
 * @modelable version: 0
 * @modelable changeKind: additive
 */
export interface NlqFieldRoleV0 {
  name: string;
  role: 'time' | 'value' | 'bucket' | 'series' | 'label';
}
export type FieldRole = NlqFieldRoleV0;
```

If the actual generated output differs, use the actual generated output instead — only the header comment block is fixed by convention.

- [ ] **Step 5: Create `apps/frontend/src/api/generated/nlq/nlq.NlqIr.v0.ts`**

Same regen-header convention, plus manually-added `import type` lines for `NlqFilter` and `NlqTimeRange` (Phase 1 backlog item 7 — the TS emitter omits these imports; the header comment documents the manual step needed on every regen):

```typescript
// Generated by modelable — do not edit by hand.
// Regenerate: copy models/nlq.mdl and models/requirements.txt into an
// isolated scratch workspace (nlq.mdl has no bindings/projections, so it
// compiles standalone — see Phase 1 backlog item 4 for why the full
// C:\git\Observable\models directory currently cannot be compiled from
// scratch), then run:
//   cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable compile <scratch-workspace>\models --target typescript --out <scratch-out>
// then copy nlq.NlqIr.v0.ts into this directory.
// MANUAL PATCH (Phase 1 backlog item 7): the TS emitter does not emit
// imports for the `filters: array<NlqFilter>` and `timeRange: NlqTimeRange`
// fields (NamedType references). Re-add the import lines below after every
// regen.
/**
 * @modelable domain: nlq
 * @modelable name: NlqIr
 * @modelable owner: platform-team
 * @modelable kind: value
 * @modelable version: 0
 * @modelable changeKind: additive
 */
import type { NlqFilter } from "./nlq.NlqFilter.v0";
import type { NlqTimeRange } from "./nlq.NlqTimeRange.v0";
export interface NlqNlqIrV0 {
  operation: 'timeseries' | 'rate' | 'irate' | 'increase' | 'histogram' | 'topk' | 'table' | 'distribution' | 'catalog' | 'inventory';
  signals: string[];
  filters: NlqFilter[];
  group_by: string[];
  time_range: NlqTimeRange;
  percentiles?: string[];
  catalog_field?: string;
  limit?: number;
  query?: string;
}
export type NlqIr = NlqNlqIrV0;
```

If the actual generated output differs from the body shown above, use the actual generated output instead for the interface body — only the header comment block and the two added `import type` lines are fixed by convention.

- [ ] **Step 6: Wire the new types into `apps/frontend/src/api/nlq.ts`**

Current `apps/frontend/src/api/nlq.ts:1-49`:

```typescript
import type { NlqIrLike } from "../features/nlq/queryFilters";

export type { NlqIrLike };

function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

// ── Types ─────────────────────────────────────────────────────────────────────

export interface FieldRole {
  name: string;
  role: "time" | "value" | "bucket" | "series" | "label";
}

export interface VisualizationFrame {
  frame_type:
    | "timeseries"
    | "histogram"
    | "heatmap"
    | "table"
    | "topk"
    | "flamegraph"
    | "distribution";
  x_field: string | null;
  y_field: string | null;
  series_field: string | null;
  unit: string | null;
  suggested_visualization: string;
  field_roles: FieldRole[];
  data: Record<string, unknown>[];
  // Provenance fields (ADR-021 — always present)
  nlq_ir: Record<string, unknown>;
  source_sql: string;
  time_range: { from: string; to: string };
  signal_types: string[];
  sample_rate: number | null;
  approximation_statement: string;
}

export interface NlqFrameResponse {
  type: "frame";
  frame: VisualizationFrame;
}

export interface NlqIrResponse {
  type: "ir";
  ir: Record<string, unknown>;
}
```

Replace with:

```typescript
import type { NlqIrLike } from "../features/nlq/queryFilters";
import type { NlqIr as GeneratedNlqIr } from "./generated/nlq/nlq.NlqIr.v0";
import type { NlqFilter } from "./generated/nlq/nlq.NlqFilter.v0";
import type { NlqTimeRange } from "./generated/nlq/nlq.NlqTimeRange.v0";
import type { FieldRole } from "./generated/nlq/nlq.FieldRole.v0";

export type { NlqIrLike, NlqFilter, NlqTimeRange, FieldRole };

function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

// ── Types ─────────────────────────────────────────────────────────────────────

// Derived from the generated NlqIr/FieldRole literal unions — no duplication.
export type NlqOperation = GeneratedNlqIr["operation"];
export type FieldRoleKind = FieldRole["role"];

// Hand-written: array<enum(...)> emits invalid TS (Phase 1 backlog item 9),
// so NlqIr.signals is generated as string[] and NlqSignal can't be derived.
// Mirrors libs/domain/src/nlq.rs::NlqSignal (rename_all = "lowercase").
export type NlqSignal = "metrics" | "traces" | "logs";

// Hand-written: shared by NlqIr.visualization_hint (Phase 1 backlog item 8 -
// Option<T> without skip_serializing_if, can't be generated) and
// VisualizationFrame.frame_type (libs/domain/src/visualization.rs's
// `impl From<NlqVisualizationHint> for VisualizationFrameType` - identical
// 7-variant value sets). Mirrors libs/domain/src/nlq.rs::NlqVisualizationHint
// / libs/domain/src/visualization.rs::VisualizationFrameType
// (both rename_all = "snake_case").
export type NlqVisualizationHint =
  | "timeseries"
  | "histogram"
  | "heatmap"
  | "table"
  | "topk"
  | "flamegraph"
  | "distribution";

export type VisualizationFrameType = NlqVisualizationHint;

// Adds back the 4 fields nlq.mdl's NlqIr can't represent (Phase 1 backlog
// item 8) and narrows `signals` from string[] to NlqSignal[] (Phase 1
// backlog item 9).
export interface NlqIr extends GeneratedNlqIr {
  signals: NlqSignal[];
  metric: string | null;
  window: string | null;
  resolution: string | null;
  visualization_hint: NlqVisualizationHint | null;
}

export interface VisualizationFrame {
  frame_type: VisualizationFrameType;
  x_field: string | null;
  y_field: string | null;
  series_field: string | null;
  unit: string | null;
  suggested_visualization: string;
  field_roles: FieldRole[];
  data: Record<string, unknown>[];
  // Provenance fields (ADR-021 — always present)
  nlq_ir: NlqIr;
  source_sql: string;
  time_range: NlqTimeRange;
  signal_types: NlqSignal[];
  sample_rate: number | null;
  approximation_statement: string;
}

export interface NlqFrameResponse {
  type: "frame";
  frame: VisualizationFrame;
}

export interface NlqIrResponse {
  type: "ir";
  ir: NlqIr;
}
```

The rest of `apps/frontend/src/api/nlq.ts` (`NlqDeclineResponse`, `NlqInvalidResponse`, `NlqCapabilitiesResponse`, `NlqResponse`, `NlqRequest`, `submitNlqQuery`) is unchanged.

- [ ] **Step 7: Typecheck**

```bash
cd apps/frontend && npm run typecheck
```

Expected: errors only in `apps/frontend/src/features/nlq/NlqPanel.test.tsx` and `apps/frontend/src/features/nlq/VisualizationPanel.test.tsx` (both construct `nlq_ir` literals that no longer satisfy the now-required `NlqIr` shape). `apps/frontend/src/App.test.tsx`'s `/v1/nlq` mock responses are plain object literals passed to `JSON.stringify(...)` (untyped, no `VisualizationFrame`/`NlqResponse` annotation) and are unaffected — confirm no errors are reported in `App.test.tsx`. These two files' errors are fixed in Task 3. Do not fix them in this task; just confirm the errors are limited to those two files.

- [ ] **Step 8: Commit**

```bash
git add apps/frontend/src/api/generated/nlq apps/frontend/src/api/nlq.ts
git commit -m "feat(frontend): generate NlqIr/NlqFilter/NlqTimeRange/FieldRole from nlq.mdl"
```

---

## Task 3: Fix `NlqPanel.test.tsx`/`VisualizationPanel.test.tsx` type fallout

**Files:**
- Modify: `apps/frontend/src/features/nlq/NlqPanel.test.tsx:36`
- Modify: `apps/frontend/src/features/nlq/VisualizationPanel.test.tsx:25`

- [ ] **Step 1: Fix `FRAME_RESPONSE.frame.nlq_ir` in `NlqPanel.test.tsx`**

Current `apps/frontend/src/features/nlq/NlqPanel.test.tsx:22-44`:

```typescript
const FRAME_RESPONSE: NlqResponse = {
  type: "frame",
  frame: {
    frame_type: "timeseries",
    x_field: "bucket",
    y_field: "value",
    series_field: null,
    unit: "ms",
    suggested_visualization: "timeseries",
    field_roles: [
      { name: "bucket", role: "time" },
      { name: "value", role: "value" },
    ],
    data: [{ bucket: "2026-01-01 10:00:00", value: 120.5 }],
    nlq_ir: { operation: "timeseries", metric: "latency_ms" },
    source_sql: "SELECT bucket, avg(value) FROM ...",
    time_range: { from: "now-1h", to: "now" },
    signal_types: ["metrics"],
    sample_rate: null,
    approximation_statement:
      "Advisory result for now-1h to now. This result is approximate and must not be used for billing.",
  },
};
```

Replace the `nlq_ir` line with a complete `NlqIr` literal (matching the sibling `signal_types`/`time_range` already present):

```typescript
const FRAME_RESPONSE: NlqResponse = {
  type: "frame",
  frame: {
    frame_type: "timeseries",
    x_field: "bucket",
    y_field: "value",
    series_field: null,
    unit: "ms",
    suggested_visualization: "timeseries",
    field_roles: [
      { name: "bucket", role: "time" },
      { name: "value", role: "value" },
    ],
    data: [{ bucket: "2026-01-01 10:00:00", value: 120.5 }],
    nlq_ir: {
      operation: "timeseries",
      signals: ["metrics"],
      filters: [],
      group_by: [],
      time_range: { from: "now-1h", to: "now" },
      metric: "latency_ms",
      window: null,
      resolution: null,
      visualization_hint: null,
    },
    source_sql: "SELECT bucket, avg(value) FROM ...",
    time_range: { from: "now-1h", to: "now" },
    signal_types: ["metrics"],
    sample_rate: null,
    approximation_statement:
      "Advisory result for now-1h to now. This result is approximate and must not be used for billing.",
  },
};
```

- [ ] **Step 2: Fix `baseFrame`'s `nlq_ir` in `VisualizationPanel.test.tsx`**

Current `apps/frontend/src/features/nlq/VisualizationPanel.test.tsx:15-33`:

```typescript
function baseFrame(overrides: Partial<VisualizationFrame> = {}): VisualizationFrame {
  return {
    frame_type: "table",
    x_field: null,
    y_field: "value",
    series_field: null,
    unit: null,
    suggested_visualization: "table",
    field_roles: [],
    data: [],
    nlq_ir: {},
    source_sql: "SELECT ...",
    time_range: { from: "now-1h", to: "now" },
    signal_types: ["metrics"],
    sample_rate: null,
    approximation_statement: "Advisory",
    ...overrides,
  };
}
```

Replace the `nlq_ir` line with a complete `NlqIr` literal (matching the sibling `frame_type`/`signal_types`/`time_range` already present):

```typescript
function baseFrame(overrides: Partial<VisualizationFrame> = {}): VisualizationFrame {
  return {
    frame_type: "table",
    x_field: null,
    y_field: "value",
    series_field: null,
    unit: null,
    suggested_visualization: "table",
    field_roles: [],
    data: [],
    nlq_ir: {
      operation: "table",
      signals: ["metrics"],
      filters: [],
      group_by: [],
      time_range: { from: "now-1h", to: "now" },
      metric: null,
      window: null,
      resolution: null,
      visualization_hint: null,
    },
    source_sql: "SELECT ...",
    time_range: { from: "now-1h", to: "now" },
    signal_types: ["metrics"],
    sample_rate: null,
    approximation_statement: "Advisory",
    ...overrides,
  };
}
```

- [ ] **Step 3: Typecheck and run the affected tests**

```bash
cd apps/frontend && npm run typecheck && npm test -- NlqPanel VisualizationPanel
```

Expected: typecheck passes with no errors; `NlqPanel.test.tsx` and `VisualizationPanel.test.tsx` tests pass.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/features/nlq/NlqPanel.test.tsx apps/frontend/src/features/nlq/VisualizationPanel.test.tsx
git commit -m "fix(frontend): complete NlqIr fixtures in NlqPanel/VisualizationPanel tests"
```

---

## Task 4: Add lineage doc comments to `libs/domain/src/nlq.rs` and `visualization.rs`

**Files:**
- Modify: `libs/domain/src/nlq.rs:13-14`, `libs/domain/src/nlq.rs:57-59`, `libs/domain/src/nlq.rs:86-88`, `libs/domain/src/nlq.rs:95-96`, `libs/domain/src/nlq.rs:140-141`
- Modify: `libs/domain/src/visualization.rs:101-102`, `libs/domain/src/visualization.rs:110-112`

- [ ] **Step 1: Add a doc comment above `NlqIr`**

Before (`libs/domain/src/nlq.rs:13-14`):

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NlqIr {
```

After:

```rust
/// Top-level NLQ intermediate representation. Mirrors `nlq.NlqIr` (value) in
/// `models/nlq.mdl` field-for-field, except `metric`/`window`/`resolution`/
/// `visualization_hint` (Phase 1 backlog item 8: `Option<T>` without
/// `skip_serializing_if` can't be generated) and `signals` (Phase 1 backlog
/// item 9: `array<enum(...))` emits invalid TypeScript, modeled as
/// `array<string>`) — see
/// `docs/superpowers/specs/2026-06-15-nlq-visualization-modelable-migration-design.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NlqIr {
```

- [ ] **Step 2: Add a doc comment above `NlqOperation`**

Before (`libs/domain/src/nlq.rs:57-59`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NlqOperation {
```

After:

```rust
/// Mirrors the inline `enum(...)` used for `nlq.NlqIr.operation` in
/// `models/nlq.mdl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NlqOperation {
```

- [ ] **Step 3: Add a doc comment above `NlqSignal`**

Before (`libs/domain/src/nlq.rs:86-88`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NlqSignal {
```

After:

```rust
/// Mirrors `nlq.NlqIr.signals: array<string>` in `models/nlq.mdl` (Phase 1
/// backlog item 9 — not modeled as `array<enum(...))`, kept as a real Rust
/// enum here).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NlqSignal {
```

- [ ] **Step 4: Add a doc comment above `NlqFilter`**

Before (`libs/domain/src/nlq.rs:95-96`):

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NlqFilter {
```

After:

```rust
/// Mirrors `nlq.NlqFilter` (value) in `models/nlq.mdl`, except `op` is
/// `string` not `enum(...)` (Phase 1 backlog item 6 — comparison-symbol
/// variants aren't valid modelable identifiers).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NlqFilter {
```

- [ ] **Step 5: Add a doc comment above `NlqTimeRange`**

Before (`libs/domain/src/nlq.rs:140-141`):

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NlqTimeRange {
```

After:

```rust
/// Mirrors `nlq.NlqTimeRange` (value) in `models/nlq.mdl` field-for-field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NlqTimeRange {
```

- [ ] **Step 6: Add a doc comment above `FieldRole` in `visualization.rs`**

Before (`libs/domain/src/visualization.rs:99-102`):

```rust
/// Describes the semantic role of a column in the result set.
/// Required for columns that the UI cannot infer from name alone (e.g. histogram buckets).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldRole {
```

After:

```rust
/// Describes the semantic role of a column in the result set.
/// Required for columns that the UI cannot infer from name alone (e.g. histogram buckets).
///
/// Mirrors `nlq.FieldRole` (value) in `models/nlq.mdl` field-for-field. See
/// `docs/superpowers/specs/2026-06-15-nlq-visualization-modelable-migration-design.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldRole {
```

- [ ] **Step 7: Add a doc comment above `FieldRoleKind`**

Before (`libs/domain/src/visualization.rs:109-112`):

```rust
/// Semantic role kinds for result columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldRoleKind {
```

After:

```rust
/// Semantic role kinds for result columns.
///
/// Mirrors the inline `enum(...)` used for `nlq.FieldRole.role` in
/// `models/nlq.mdl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldRoleKind {
```

No other Rust changes — `VisualizationFrame`, `VisualizationFrameType`, `NlqVisualizationHint`, `NlqFilterOp`, `EnvelopePayload`/`TelemetryEnvelope` are untouched (each is a named exception per the design doc's Non-Goals/backlog items; none has a `.mdl` counterpart).

- [ ] **Step 8: Format and build-check**

```bash
cargo fmt --all
cd libs/domain && cargo check
```

Expected: no warnings/errors; `cargo fmt --all` should produce no diff beyond the new comments.

- [ ] **Step 9: Commit**

```bash
git add libs/domain/src/nlq.rs libs/domain/src/visualization.rs
git commit -m "docs(domain): cross-reference nlq.mdl NlqIr/NlqFilter/NlqTimeRange/FieldRole"
```

---

## Task 5: Full verification, mark 3.9 done, record backlog items 8 and 9

**Files:**
- Modify: `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md:100`, `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md:145-153`

- [ ] **Step 1: Frontend verification**

```bash
cd apps/frontend
npm run typecheck && npm run lint && npm test && npm run build
```

Expected: all pass with no new failures.

- [ ] **Step 2: Full local CI**

```bash
bash scripts/local-ci.sh
```

Expected: passes (frontend typecheck/lint/build/test, Rust fmt/clippy/unit tests, Docker image build, smoke test). If the Rust integration-test step fails with an LLVM/rustc OOM (`STATUS_STACK_BUFFER_OVERRUN`) as seen in 3.7/3.8, this is a pre-existing Windows resource-exhaustion issue unrelated to this migration (doc-comment-only Rust change) — verify with `cargo check -p domain --all-targets` under `CARGO_BUILD_JOBS=1`, which should pass cleanly, and document the same caveat in the commit/PR description.

- [ ] **Step 3: Mark Phase 3 step 3.9 done**

In `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`, change line 100:

Before:

```markdown
- [ ] **3.9 NLQ/Visualization** — `libs/domain/src/{nlq,visualization,envelope}.rs`, `services/query-api/src/mcp_tools.rs:37-80` (incl. `From` impls at lines 120-174), `apps/frontend/src/api/nlq.ts:3-76`. Likely the most complex (CEL-computed fields, `NlqResponse` union type) — evaluate whether modelable's projection model can represent it before committing to full replacement; otherwise document as a deliberate, named exception.
```

After:

```markdown
- [x] **3.9 NLQ/Visualization** — Generated `value NlqIr`/`NlqFilter`/`NlqTimeRange`/`FieldRole` from `models/nlq.mdl` (see `docs/superpowers/specs/2026-06-15-nlq-visualization-modelable-migration-design.md`). `apps/frontend/src/api/nlq.ts`'s `NlqFilter`/`NlqTimeRange`/`FieldRole` are now re-exports of `apps/frontend/src/api/generated/nlq/nlq.NlqFilter.v0.ts`/`nlq.NlqTimeRange.v0.ts`/`nlq.FieldRole.v0.ts`; `NlqOperation`/`FieldRoleKind` are derived via indexed-access types from the generated `NlqIr`/`FieldRole`. `NlqIr` is the generated `NlqIr` extended with `metric`/`window`/`resolution`/`visualization_hint: T | null` and a narrowed `signals: NlqSignal[]` — both additions needed because of two new gaps (Phase 1 backlog items 8 and 9). `NlqSignal` and `NlqVisualizationHint`/`VisualizationFrameType` (alias) remain hand-written. `VisualizationFrame.nlq_ir`/`time_range`/`signal_types`/`field_roles`/`frame_type` and `NlqIrResponse.ir` retype accordingly; `x_field`/`y_field`/`series_field`/`unit`/`sample_rate` stay `T | null` (pre-existing exception, also backlog item 8). `libs/domain/src/nlq.rs`'s `NlqIr`/`NlqOperation`/`NlqSignal`/`NlqFilter`/`NlqTimeRange` and `visualization.rs`'s `FieldRole`/`FieldRoleKind` get lineage doc comments only. `NlqResponse` (no oneof/union construct), `NlqIrLike`, `envelope.rs` (no frontend consumer), and `mcp_tools.rs`'s `MetricSchema`/`SignalField` (folded into deferred 3.5b) remain hand-written. Fixed `nlq_ir` fixtures in `NlqPanel.test.tsx`/`VisualizationPanel.test.tsx`. New Phase 1 backlog items 8 (`Option<T>` without `skip_serializing_if` can't be generated) and 9 (`array<enum(...))` emits invalid TypeScript). This was the last regular Phase 3 domain — 3.5b Schemas remains separately deferred.
```

- [ ] **Step 4: Record backlog items 8 and 9**

In the same file's "## Phase 1 backlog (modelable gaps discovered during Phase 3)" section (currently ending at line 153 with item 7), append two new items after item 7:

```markdown
8. **`Option<T>` without `skip_serializing_if` (always-serialized `null`) can't be modeled.**
   modelable's only optionality mechanism is `field?: T` (`cli/src/modelable/grammar/modelable.lark`'s
   `optional_marker`), which the TS emitter (`cli/src/modelable/emitters/typescript.py:78`) always
   renders as an *omittable* property (`field?: T`, i.e. `T | undefined`). There is no way to
   express "always present, but may be `null`" (`T | null`). Affects any Rust `Option<T>` field
   without `#[serde(skip_serializing_if = "Option::is_none")]` — e.g.
   `VisualizationFrame.x_field`/`y_field`/`series_field`/`unit`/`sample_rate` (pre-existing
   exception) and `NlqIr.metric`/`window`/`resolution`/`visualization_hint` (3.9). Worked around by
   adding these fields back via TypeScript interface extension (`interface NlqIr extends
   GeneratedNlqIr { metric: string | null; ... }`).
9. **`array<enum(...))` emits invalid TypeScript.** `_type_to_ts`
   (`cli/src/modelable/emitters/typescript.py:229-288`) handles `ArrayType` as
   `f"{_type_to_ts(field_type.item)}[]"`. For `EnumType`, `_type_to_ts` returns an unparenthesized
   union (`'a' | 'b' | 'c'`). Composing these for `array<enum(a,b,c))` produces
   `field: 'a' | 'b' | 'c'[];`, which TypeScript parses as `'a' | 'b' | ('c'[])` — not
   `('a'|'b'|'c')[]` as intended. No existing `.mdl` in this repo or in modelable's own test
   fixtures used `array<enum(...))` before 3.9 (confirmed by grep). Blocked modeling
   `NlqIr.signals: Vec<NlqSignal>` as `array<enum(metrics,traces,logs))`; worked around by modeling
   `signals` as `array<string>` and keeping `NlqSignal` hand-written in `nlq.ts`.
```

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md
git commit -m "docs: mark Phase 3 step 3.9 (NLQ/Visualization) done, record backlog items 8/9"
```

---

## Out of scope

- `NlqResponse` (5-variant discriminated union) — modelable has no `oneof`/union construct (confirmed via grep of `cli/src/modelable/parser/ir.py`). `NlqFrameResponse`/`NlqDeclineResponse`/`NlqInvalidResponse`/`NlqCapabilitiesResponse` remain hand-written and unchanged.
- `NlqIrLike` (`apps/frontend/src/features/nlq/queryFilters.ts`) — deliberately loose, handler-local validation type for `NlqRequest.base_ir`. Unchanged.
- `envelope.rs` (`TelemetryEnvelope`/`EnvelopePayload`) — no frontend TS consumer.
- `mcp_tools.rs`'s `MetricSchema`/`SignalField` (Schema Registry projections) — folded into deferred **3.5b Schemas** (no frontend TS consumer yet).
- `NlqFilterOp` as a real enum — symbol-valued variants aren't valid modelable enum identifiers (Phase 1 backlog item 6). `NlqFilter.op` is `string`.
- Fixing modelable backlog items 5/6/7/8/9 themselves in `C:\git\modelable` — recorded as backlog items for a future modelable release.
- Bumping the modelable pin — `models/requirements.txt` is already `modelable==0.4.0`.
- Any change to `services/query-api/src/mcp_query.rs`/`llm_adapter.rs` handler logic.
