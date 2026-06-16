# Dashboards Domain: Modelable Migration Design

**Date:** 2026-06-14
**Status:** Approved
**Scope:** Phase 3 step 3.8 of `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md` — author `models/dashboards.mdl` for the `DashboardItem`/`DashboardPanelItem` shapes (including the nested `layout` value type), generate and commit TypeScript artifacts, and wire them into `apps/frontend/src/api/dashboards.ts`. Document why `GrantItem`, `DashboardExportPanel`/`DashboardExport`, `DashboardListResponse`, `CreateDashboardRequest`, and `UpdateDashboardRequest` remain hand-written, and fix the resulting type-fallout in `DashboardDetailPage.tsx`.

## Context

`services/query-api/src/dashboards.rs` defines several shapes:

- `DashboardPanelItem` (`#[derive(Serialize, Clone, Debug, PartialEq)]`): `panel_id: Uuid`, `title: String`, `panel_kind: String` (`VALID_PANEL_KINDS = ["query", "text"]`), `query_kind: Option<String>` (`VALID_QUERY_KINDS = ["logs", "traces", "metrics"]`), `service: Option<String>`, `preset: Option<String>` (`VALID_PRESETS = ["5m", "15m", "30m", "1h", "3h", "12h"]`), `filters: serde_json::Value`, `query_text: Option<String>`, `content: Option<String>`, `layout: serde_json::Value` (in practice always `{x, y, w, h}`), `time_range: serde_json::Value` (a 3-way discriminated union: `{mode: "global"}` | `{mode: "preset", preset}` | `{mode: "absolute", from_ms, to_ms}`).
- `DashboardItem` (`#[derive(Serialize, Clone, Debug, PartialEq)]`): `dashboard_id: Uuid`, `name: String`, `visibility: String` (validated as `"public"|"private"`), `panels: Vec<DashboardPanelItem>`, `created_at: DateTime<Utc>`. This is the API response shape for the dashboards list and single-dashboard fetch.
- `DashboardListResponse { items: Vec<DashboardItem> }` — list wrapper.
- `GrantItem` (`#[derive(Serialize, sqlx::FromRow)]`): `user_id: Uuid`, `relation: String` (validated as `"owner"|"editor"|"viewer"`), `granted_at: DateTime<Utc>`. Returned from `GrantListResponse { grants: Vec<GrantItem> }`.
- `AddGrantRequest`, `DashboardExportPanel`/`DashboardExport` (hand-written export/import format), `DashboardRow`/`DashboardPanelRow` (`#[derive(sqlx::FromRow)]` Postgres projections) — out of scope, see Non-Goals.

`apps/frontend/src/api/dashboards.ts` hand-writes matching `DashboardPanel` (lines 7-27, including `DashboardPanelLayout` and the `DashboardPanelTimeRange` union) and `Dashboard` (lines 29-35 — note: currently has **no** `visibility` field, despite Rust having one) interfaces, plus `DashboardListResponse`, `CreateDashboardRequest`, `UpdateDashboardRequest`, `DashboardExportPanel`, `DashboardExport`.

`apps/frontend/src/router.ts:33` defines `export type Preset = "5m" | "15m" | "30m" | "1h" | "3h" | "12h";`, used by `DashboardPanel.preset` and `DashboardPanelTimeRange`'s `{mode: "preset", preset: Preset}` variant.

### Why TS-only ("Option B") — reusing Phase 1 backlog item 5

`panel_kind`/`query_kind`/`visibility` are plain `String` in Rust, validated against fixed sets at the handler level. `created_at`/`granted_at` are `chrono::DateTime<Utc>`, required for `sqlx::FromRow` row-mapping (`DashboardRow`/`DashboardPanelRow`, not modeled here). modelable's `timestamp` primitive emits as Rust `String` (Phase 1 backlog item 5). So, same as 3.2-3.7, this migration is **TS-only**: generate TypeScript artifacts, re-export them into `dashboards.ts`, and add lineage doc comments to the Rust `DashboardItem`/`DashboardPanelItem` without changing their code.

### Scope: `DashboardItem` + `DashboardPanelItem` (incl. nested `layout`) only

- `DashboardListResponse` is a list wrapper (same precedent as `AlertRuleListResponse`/`SloListResponse`/etc.) — stays hand-written.
- `CreateDashboardRequest`/`UpdateDashboardRequest` are request bodies — stay hand-written. `UpdateDashboardRequest.panels[]` keeps its own hand-written shape (it's the request-side mirror of `DashboardPanelItem`, used as the cast target for `panelToUpdate` in the fallout below).
- `DashboardExportPanel`/`DashboardExport` are a hand-written export/import file format, intentionally decoupled from the API response shape — stay hand-written.
- `GrantItem` has **zero frontend consumers** (no `Grant`/`grant` reference anywhere in `apps/frontend/src`). Deferred, same as 3.5b Schemas — revisit if/when a frontend consumer appears. No doc comment added (consistent with other untouched structs in prior phases).

`DashboardItem` has a natural unique key (`dashboard_id`), modeled as `entity` kind. `DashboardPanelItem` and its `layout` field have no `@key` and are always embedded — both modeled as `value` kind (per `commerce.mdl`'s `value Item { ... }` + `array<Item>` precedent), referenced via `Dashboard.panels: array<DashboardPanel>` and `DashboardPanel.layout: DashboardPanelLayout`.

### `panel_kind`/`query_kind`/`visibility` as `enum(...)`, `preset` as `string` (NEW backlog item)

- `panel_kind: enum(query, text)`, `query_kind: enum(logs, traces, metrics)`, `visibility: enum(public, private)` — all values are valid modelable identifiers (start with a letter), matching the existing hand-written TS unions and the Rust `VALID_PANEL_KINDS`/`VALID_QUERY_KINDS`/visibility validation. Same precedent as 3.7's `operator`/`state` enums.
- `preset` **cannot** be `enum(5m, 15m, 30m, 1h, 3h, 12h)`. modelable's grammar (`cli/src/modelable/grammar/modelable.lark`: `enum_type: "enum" "(" IDENT ("," IDENT)* ")"`, `IDENT: /[A-Za-z_][A-Za-z0-9_-]*/`) requires every enum member to start with a letter or underscore — `5m`/`15m`/`1h`/`3h`/`12h`/`30m` all start with digits and are rejected by the parser. **New Phase 1 backlog item 6**: modelable's `enum(...)` cannot represent numeric-prefixed string-literal unions (like `Preset`). `preset` is modeled as `preset?: string` instead, with the fallout noted below.

### NEW backlog item 7: TS emitter doesn't import cross-model `NamedType` field references

A scratch compile of the `.mdl` design below (`modelable compile --target typescript`) succeeds but emits `WARN [EMIT003] Missing metadata required by target: dashboards.DashboardPanel.layout` and produces:

```typescript
// dashboards.Dashboard.v1.ts
export interface DashboardsDashboardV1 {
  dashboard_id: string;
  name: string;
  visibility: 'public' | 'private';
  panels: DashboardPanel[];   // <- DashboardPanel referenced but not imported
  created_at: string;
}
export type Dashboard = DashboardsDashboardV1;
```

```typescript
// dashboards.DashboardPanel.v0.ts
export interface DashboardsDashboardPanelV0 {
  ...
  layout: DashboardPanelLayout;   // <- DashboardPanelLayout referenced but not imported
  ...
}
export type DashboardPanel = DashboardsDashboardPanelV0;
```

Both files reference a sibling-model type by name with no `import` statement — as generated, neither file compiles standalone. This is a genuinely new gap (the Rust emitter's analogous limitation for `Vec<SpanEvent>`-style nested types was already documented in Phase 2; this is the TS-side equivalent, not previously hit because no prior domain's `.mdl` referenced another model from a field). **New Phase 1 backlog item 7**: the TS emitter should emit `import type { X } from "./<domain>.X.v<N>"` for every field whose type is a `NamedType`.

**Workaround for this migration**: after generation, manually prepend one `import type` line to each affected file (`dashboards.Dashboard.v1.ts` imports `DashboardPanel`; `dashboards.DashboardPanel.v0.ts` imports `DashboardPanelLayout`). This is a one-line, mechanical, easily-re-applied-on-regen patch — documented inline in the generated files' header comment alongside the existing regen-header convention.

## Goal

- Add `models/dashboards.mdl` defining `dashboards.Dashboard@1` (entity, mirrors `DashboardItem`), `value DashboardPanel` (mirrors `DashboardPanelItem`), and `value DashboardPanelLayout` (mirrors the `{x,y,w,h}` shape of `DashboardPanelItem.layout`).
- Generate and commit TypeScript artifacts (`apps/frontend/src/api/generated/dashboards/`), with the one-line import patch per file described above; re-export `Dashboard`/`DashboardPanel`/`DashboardPanelLayout` from `apps/frontend/src/api/dashboards.ts` under their existing names (no aliasing needed — generated names match existing TS interface names exactly).
- Fix the resulting fallout in `apps/frontend/src/pages/DashboardDetailPage.tsx` (`preset`, `time_range`, `filters` casts).
- Add doc comments on `services/query-api/src/dashboards.rs`'s `DashboardItem` and `DashboardPanelItem` cross-referencing `models/dashboards.mdl`. No Rust code changes beyond these comments.
- Record new Phase 1 backlog items 6 (enum identifier grammar constraint) and 7 (TS emitter cross-model import gap) in `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`.
- Mark Phase 3 step 3.8 done in the migration plan.

## Non-Goals

- `GrantItem`, `AddGrantRequest`, `GrantListResponse`, `DashboardRow`, `DashboardPanelRow`, `DashboardListResponse`, `CreateDashboardRequest`, `UpdateDashboardRequest`, `DashboardExportPanel`, `DashboardExport` — all remain entirely hand-written (see Context/Scope).
- Any change to `services/query-api/src/dashboards.rs` handler logic, SQL, or Postgres DDL/migrations.
- Bumping the modelable pin — `models/requirements.txt` is already `modelable==0.4.0` and has everything needed (entity/value kinds, optional, timestamp, enum, array<T> of a value type, json, `@wire(json.fieldCase: ...)`).
- Modeling `time_range` as a proper discriminated union — modelable has no union/oneof construct; `time_range: json` → TS `unknown`, fallout accepted (see below).
- Fixing the modelable TS emitter gaps (backlog items 6, 7) themselves — recorded as backlog items for a future modelable release, worked around manually for this migration.

## Design

### 1. `models/dashboards.mdl` (new file, new domain `dashboards`)

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

Field-by-field, `Dashboard` mirrors `DashboardItem`: `dashboard_id` (UUID), `name` (string), `visibility` (enum → TS `"public" | "private"`, a NEW field on the generated type not present in the current hand-written `Dashboard` interface — additive, no existing code reads `.visibility`), `panels` (`array<DashboardPanel>` → TS `DashboardPanel[]`), `created_at` (`timestamp` → TS `string`). `DashboardPanel` mirrors `DashboardPanelItem`: `panel_id` (UUID), `title` (string), `panel_kind` (enum → TS `"query" | "text"`), `query_kind` (optional enum → TS `"logs" | "traces" | "metrics" | undefined`), `service`/`query_text`/`content` (optional strings), `preset` (optional string), `filters`/`time_range` (`json` → TS `unknown`), `layout` (`DashboardPanelLayout` → TS `{x,y,w,h}` numbers).

No `projection`/`binding` blocks.

### 2. Generated TypeScript artifacts

New directory `apps/frontend/src/api/generated/dashboards/`, containing `dashboards.Dashboard.v1.ts`, `dashboards.DashboardPanel.v0.ts`, `dashboards.DashboardPanelLayout.v0.ts` (generated via `modelable compile` in an isolated scratch workspace — `dashboards.mdl` has no `binding`/`projection` blocks, compiles standalone). Confirmed exact output via scratch compile:

`dashboards.DashboardPanelLayout.v0.ts` (no patch needed — only primitive fields):
```typescript
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

`dashboards.DashboardPanel.v0.ts` (patched: add the `import type` line for `DashboardPanelLayout` after the header comment):
```typescript
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

`dashboards.Dashboard.v1.ts` (patched: add the `import type` line for `DashboardPanel`):
```typescript
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

In `apps/frontend/src/api/dashboards.ts`, replace the hand-written `DashboardPanelLayout`, `DashboardPanel`, and `Dashboard` interfaces (current lines 7-9, 14-27, 29-35) with:

```typescript
import type { DashboardPanelLayout } from "./generated/dashboards/dashboards.DashboardPanelLayout.v0";
export type { DashboardPanelLayout };

import type { DashboardPanel } from "./generated/dashboards/dashboards.DashboardPanel.v0";
export type { DashboardPanel };

import type { Dashboard } from "./generated/dashboards/dashboards.Dashboard.v1";
export type { Dashboard };
```

`DashboardPanelTimeRange` (the hand-written 3-way union, still needed as the cast target for `panel.time_range`), `DashboardPanelKind`, `DashboardQueryKind`, `DashboardListResponse`, `CreateDashboardRequest`, `UpdateDashboardRequest`, `DashboardExportPanel`, `DashboardExport`, and all API functions are unchanged.

### 3. Type-fallout fixes in `apps/frontend/src/pages/DashboardDetailPage.tsx`

- **Line 53** (`panelToUpdate`, `preset: panel.preset`): `panel.preset` is now `string | undefined`; the target `UpdateDashboardRequest["panels"][number]["preset"]` is `Preset | null`. Fix:
  ```typescript
  preset: (panel.preset as Preset | undefined) ?? null,
  ```
- **Line 674** (`resolvePanelTimeRange(panel.time_range, globalRange)`): `panel.time_range` is now `unknown`; `resolvePanelTimeRange` expects `DashboardPanelTimeRange`. Fix:
  ```typescript
  const resolved = resolvePanelTimeRange(panel.time_range as DashboardPanelTimeRange, globalRange);
  ```
- **Line 681** (`dashboardFiltersToNlqFilters(panel.filters)`): `panel.filters` is now `unknown`; `dashboardFiltersToNlqFilters` expects `Record<string, unknown>`. Fix:
  ```typescript
  filters: signal === "metrics" ? dashboardFiltersToNlqFilters(panel.filters as Record<string, unknown>) : [],
  ```
- **Line 675** (`panel.query_kind ?? "logs"`): no change — `??` handles both `null` and `undefined`.
- **Lines 161, 183** (`preset: null,` in new-panel construction for `CreateDashboardRequest`/local state): no change — these construct hand-written request/local types (`Preset | null`), not the generated `DashboardPanel`.
- `DashboardsPage.tsx` and `PanelTemplateLibrary.tsx`: no changes — they only reference type names (`Dashboard`, `DashboardPanelKind`, `DashboardQueryKind`), which are unaffected.

## Rust lineage comments

In `services/query-api/src/dashboards.rs`, add doc comments above `DashboardItem` and `DashboardPanelItem`:

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

No other Rust changes — `GrantItem`, `DashboardRow`, `DashboardPanelRow`, `DashboardListResponse`, and all handlers/functions are untouched.

## Verification

- modelable: `cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable validate C:\git\Observable\models` passes.
- Lineage proof: `modelable lineage dashboards.Dashboard@1`, included in the commit/PR description (`DashboardPanel`/`DashboardPanelLayout` are unversioned `value` kinds — no lineage to prove).
- Frontend: `npm run typecheck && npm run lint && npm test && npm run build`.
- Rust: `cargo fmt --all && cargo check` for `query-api` (doc-comment-only change).
- Full: `bash scripts/local-ci.sh` (if the LLVM/rustc OOM from 3.7 recurs, verify with `cargo check -p query-api --all-targets` under `CARGO_BUILD_JOBS=1` per the 3.7 precedent).
- Mark Phase 3 step 3.8 done in `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`.
- Add backlog items 6 and 7 to the "Phase 1 backlog" section of `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`.
