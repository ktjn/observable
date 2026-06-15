# NLQ/Visualization Domain: Modelable Migration Design

**Date:** 2026-06-15
**Status:** Approved
**Scope:** Phase 3 step 3.9 of `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md` — author `models/nlq.mdl` for `NlqIr` and its sub-types (`NlqFilter`, `NlqTimeRange`, `FieldRole`), generate and commit TypeScript artifacts, and retype `apps/frontend/src/api/nlq.ts`'s `VisualizationFrame`/`NlqIr`/`NlqIrResponse` to use them. Document why `NlqResponse`, `NlqIrLike`, `envelope.rs`, and `mcp_tools.rs`'s `MetricSchema`/`SignalField` remain hand-written, and fix the resulting type fallout in test fixtures.

## Context

`libs/domain/src/nlq.rs` defines the canonical NLQ IR:

- `NlqIr` (`#[derive(Serialize, Deserialize)]`): `operation: NlqOperation`, `signals: Vec<NlqSignal>`, `metric: Option<String>`, `window: Option<String>`, `filters: Vec<NlqFilter>` (`#[serde(default)]`), `group_by: Vec<String>` (`#[serde(default)]`), `resolution: Option<String>`, `time_range: NlqTimeRange`, `visualization_hint: Option<NlqVisualizationHint>`, `percentiles: Option<Vec<String>>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`), `catalog_field: Option<String>` (same), `limit: Option<u32>` (same), `query: Option<String>` (same).
- `NlqOperation` (10 variants, `rename_all = "snake_case"`), `NlqSignal` (3 variants, `rename_all = "lowercase"`).
- `NlqFilter`: `field: String`, `op: NlqFilterOp`, `value: String`. `NlqFilterOp` (8 variants) uses explicit symbol renames (`"="`, `"!="`, `"=~"`, `"!~"`, `">"`, `">="`, `"<"`, `"<="`).
- `NlqTimeRange`: `from: String`, `to: String`.
- `NlqVisualizationHint` (7 variants, `rename_all = "snake_case"`: timeseries, histogram, heatmap, table, topk, flamegraph, distribution).

`libs/domain/src/visualization.rs` defines `VisualizationFrame`: `frame_type: VisualizationFrameType`, `x_field`/`y_field`/`series_field`/`unit: Option<String>`, `suggested_visualization: String`, `field_roles: Vec<FieldRole>` (`#[serde(default)]`), `data: Vec<serde_json::Value>`, `nlq_ir: NlqIr`, `source_sql: String`, `time_range: NlqTimeRange`, `signal_types: Vec<NlqSignal>`, `sample_rate: Option<f64>`, `approximation_statement: String`. `VisualizationFrameType` (7 variants, same value set as `NlqVisualizationHint`, with `impl From<NlqVisualizationHint> for VisualizationFrameType`). `FieldRole`: `name: String`, `role: FieldRoleKind` (5 variants: time, value, bucket, series, label).

`apps/frontend/src/api/nlq.ts` currently hand-writes `FieldRole`, `VisualizationFrame` (with `nlq_ir: Record<string, unknown>`, `x_field`/`y_field`/`series_field`/`unit`/`sample_rate: T | null`), and the 5-variant `NlqResponse` union (`NlqFrameResponse | NlqIrResponse | NlqDeclineResponse | NlqInvalidResponse | NlqCapabilitiesResponse`), where `NlqIrResponse.ir: Record<string, unknown>`.

### Why TS-only ("Option B")

Same rationale as 3.1-3.8: this is purely a frontend-typing improvement. No Rust struct shapes change — `NlqIr`/`NlqFilter`/`NlqTimeRange`/`FieldRole`/etc. get lineage doc comments only.

### NEW backlog item 8: `Option<T>` without `skip_serializing_if` (always-serialized `null`) can't be modeled

modelable's `field_decl: ... optional_marker? ":" type_expr ...` (`cli/src/modelable/grammar/modelable.lark`) has exactly one optionality mechanism: `field?: T`, which the TS emitter (`cli/src/modelable/emitters/typescript.py:78`) always renders as an **omittable** property (`field?: T`, i.e. `T | undefined`). There is no way to express "always present, but may be `null`" (`T | null`).

`NlqIr.metric`, `.window`, `.resolution`, `.visualization_hint` are `Option<T>` **without** `#[serde(skip_serializing_if = "Option::is_none")]` — they always serialize, as `null` when absent (unlike `percentiles`/`catalog_field`/`limit`/`query`, which have `skip_serializing_if` and are genuinely omittable). These 4 fields cannot be part of the generated `NlqIr` value type; they're added back via interface extension (see Design).

This is the same shape as the already-known `VisualizationFrame.x_field`/`y_field`/`series_field`/`unit`/`sample_rate` exception, now also affecting 4 of `NlqIr`'s 13 fields.

### NEW backlog item 9: `array<enum(...)>` emits invalid TypeScript

`_type_to_ts` (`cli/src/modelable/emitters/typescript.py:229-288`) handles `ArrayType` as `f"{_type_to_ts(field_type.item)}[]"`. For `EnumType`, `_type_to_ts` returns an unparenthesized union (`'a' | 'b' | 'c'`). Composing these for `array<enum(a,b,c)>` produces:

```typescript
field: 'a' | 'b' | 'c'[];
```

which TypeScript parses as `'a' | 'b' | ('c'[])` — a 3-member union where only the last member is an array — not `('a'|'b'|'c')[]` as intended. No existing `.mdl` in this repo or in modelable's own test fixtures uses `array<enum(...)>` (confirmed by grep), so this hasn't been hit before.

This blocks modeling `NlqIr.signals: Vec<NlqSignal>` as `array<enum(metrics,traces,logs))`. **Workaround for this migration**: model `signals` as `array<string>` (→ TS `string[]`, correct), and keep `NlqSignal` (`"metrics" | "traces" | "logs"`) hand-written in `nlq.ts` — same treatment as `NlqVisualizationHint`/`VisualizationFrameType` (backlog item 8). The hand-extended `NlqIr` interface then narrows `signals` back to `NlqSignal[]` via a covariant override (valid TS interface-extension).

## Goal

- Add `models/nlq.mdl` defining `value NlqTimeRange`, `value NlqFilter`, `value FieldRole`, `value NlqIr` (9 of its 13 fields).
- Generate and commit TypeScript artifacts under `apps/frontend/src/api/generated/nlq/`.
- In `apps/frontend/src/api/nlq.ts`: re-export `NlqTimeRange`/`NlqFilter`/`FieldRole`; derive `NlqOperation` and `FieldRoleKind` via indexed-access types from the generated `NlqIr`/`FieldRole`; hand-write `NlqSignal` (3-variant) and `NlqVisualizationHint`/`VisualizationFrameType` (7-variant, shared alias); define `NlqIr` as `GeneratedNlqIr` extended with `metric`/`window`/`resolution: string | null`, `visualization_hint: NlqVisualizationHint | null`, and `signals: NlqSignal[]` (narrowed). Retype `VisualizationFrame.nlq_ir`, `.time_range`, `.signal_types`, `.field_roles`, `.frame_type`, and `NlqIrResponse.ir` accordingly.
- Add lineage doc comments to `libs/domain/src/nlq.rs` (`NlqIr`, `NlqOperation`, `NlqSignal`, `NlqFilter`, `NlqTimeRange`) and `libs/domain/src/visualization.rs` (`FieldRole`, `FieldRoleKind`) referencing `models/nlq.mdl`. No Rust code changes.
- Fix type-fallout in test fixtures that construct `nlq_ir`/`VisualizationFrame` literals.
- Record new Phase 1 backlog items 8 and 9 in the migration plan.
- Mark Phase 3 step 3.9 done (the last regular Phase 3 domain; 3.5b Schemas remains separately deferred).

## Non-Goals

- `NlqResponse` (5-variant discriminated union) — modelable has no `oneof`/union construct (confirmed via grep of `cli/src/modelable/parser/ir.py`: no `oneof`/`union`/`discriminat` matches). Stays hand-written; `NlqFrameResponse`/`NlqDeclineResponse`/`NlqInvalidResponse`/`NlqCapabilitiesResponse` are unaffected by this migration (`NlqIrResponse.ir` is retyped, see Design).
- `NlqIrLike` (`apps/frontend/src/features/nlq/queryFilters.ts`) — a deliberately loose, handler-local validation type for `NlqRequest.base_ir`. Stays hand-written.
- `envelope.rs` (`TelemetryEnvelope`/`EnvelopePayload`) — no frontend TS consumer. Out of scope.
- `mcp_tools.rs`'s `MetricSchema`/`SignalField` (Schema Registry projections) — folded into deferred **3.5b Schemas** (no frontend TS consumer yet, same as `schemas.rs`'s `SchemaEntry`/`SemanticAnnotation`).
- `NlqFilterOp` as a real enum — its symbol-valued variants (`=`, `!=`, `=~`, `!~`, `>`, `>=`, `<`, `<=`) aren't valid modelable enum identifiers (Phase 1 backlog item 6 extends to symbols, not just digit-prefixes). `NlqFilter.op` is modeled as `string`.
- Fixing modelable backlog items 5/6/7/8/9 themselves — recorded for a future modelable release.
- Bumping the modelable pin — `models/requirements.txt` is already `modelable==0.4.0` and has everything needed (`value` kind, optional, enum, array, `@wire(json.fieldCase: ...)`).

## Design

### 1. `models/nlq.mdl` (new file, new domain `nlq`)

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

No `projection`/`binding` blocks — compiles standalone.

Field-by-field, generated `NlqIr` mirrors `libs/domain/src/nlq.rs::NlqIr`'s 9 generatable fields: `operation` (enum → TS literal union), `signals` (`string[]`, narrowed to `NlqSignal[]` in the hand-written extension), `filters` (`NlqFilter[]`), `group_by` (`string[]`), `time_range` (`NlqTimeRange`), `percentiles?`/`catalog_field?`/`limit?`/`query?` (all optional, matching their `skip_serializing_if`).

### 2. Generated TypeScript artifacts (predicted output, per the `_type_to_ts`/`_emit_model` logic read above)

New directory `apps/frontend/src/api/generated/nlq/`, containing `nlq.NlqTimeRange.v0.ts`, `nlq.NlqFilter.v0.ts`, `nlq.FieldRole.v0.ts`, `nlq.NlqIr.v0.ts` (generated via `modelable compile` in an isolated scratch workspace, per the dashboards/3.8 precedent).

`nlq.NlqTimeRange.v0.ts` / `nlq.NlqFilter.v0.ts` / `nlq.FieldRole.v0.ts` — no cross-model references, no patch needed:

```typescript
export interface NlqNlqTimeRangeV0 {
  from: string;
  to: string;
}
export type NlqTimeRange = NlqNlqTimeRangeV0;
```

```typescript
export interface NlqNlqFilterV0 {
  field: string;
  op: string;
  value: string;
}
export type NlqFilter = NlqNlqFilterV0;
```

```typescript
export interface NlqFieldRoleV0 {
  name: string;
  role: 'time' | 'value' | 'bucket' | 'series' | 'label';
}
export type FieldRole = NlqFieldRoleV0;
```

`nlq.NlqIr.v0.ts` (patched per Phase 1 backlog item 7: add `import type` lines for `NlqFilter` and `NlqTimeRange`):

```typescript
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

(Each file also carries the standard `@modelable domain/name/owner/kind/version/changeKind` header comment and regeneration instructions, per the dashboards/3.8 precedent — omitted above for brevity.)

### 3. `apps/frontend/src/api/nlq.ts` changes

```typescript
import type { NlqIr as GeneratedNlqIr } from "./generated/nlq/nlq.NlqIr.v0";
import type { NlqFilter } from "./generated/nlq/nlq.NlqFilter.v0";
import type { NlqTimeRange } from "./generated/nlq/nlq.NlqTimeRange.v0";
import type { FieldRole } from "./generated/nlq/nlq.FieldRole.v0";

export type { NlqFilter, NlqTimeRange, FieldRole };

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

// Adds back the 4 fields NlqIr.mdl can't represent (Phase 1 backlog item 8)
// and narrows `signals` from string[] to NlqSignal[] (Phase 1 backlog item 9).
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
```

`NlqIrResponse.ir` retypes from `Record<string, unknown>` to `NlqIr` (the `/v1/nlq` "ir" response wraps a full `NlqIr` per ADR-021).

`NlqFrameResponse`/`NlqDeclineResponse`/`NlqInvalidResponse`/`NlqCapabilitiesResponse`, `NlqResponse`, `NlqRequest`/`NlqIrLike`, `submitNlqQuery` — unchanged.

### 4. Type-fallout fixes

`InterpretedIrPanel`/`QueryFilterInput.tsx` (lines 77/81/117) consume `response.ir` as opaque JSON for display (`JSON.stringify`) and pass-through to `onIr?: (ir: NlqIrLike | Record<string,unknown>) => void` — `NlqIr` is assignable wherever `Record<string, unknown>` was structurally relied upon for display only; no logic changes expected, but verify via typecheck.

Test fixtures constructing `nlq_ir`/`VisualizationFrame` literals must supply the now-required `NlqIr` fields (`operation`, `signals`, `filters`, `group_by`, `time_range`, `metric`, `window`, `resolution`, `visualization_hint`):

- `apps/frontend/src/App.test.tsx` — 8 occurrences: lines 84, 153, 541, 632, 724, 787, 848, 933, 1122 (`nlq_ir: {}` or `nlq_ir: { operation: "inventory" }` / `{ operation: "timeseries", metric: "latency_ms" }`-style partials).
- `apps/frontend/src/features/nlq/NlqPanel.test.tsx:36` — `nlq_ir: { operation: "timeseries", metric: "latency_ms" }`.
- `apps/frontend/src/features/nlq/VisualizationPanel.test.tsx:25` — `nlq_ir: {}`.

Each becomes a complete `NlqIr` literal, e.g.:

```typescript
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
```

(field values per-fixture chosen to match the existing partial's intent, e.g. `operation`/`metric` where already specified, `signals`/`time_range` matching the sibling `signal_types`/`time_range` already present on the same `VisualizationFrame` literal where applicable).

## Rust lineage comments

In `libs/domain/src/nlq.rs`:

```rust
/// Top-level NLQ intermediate representation. Mirrors `nlq.NlqIr` (value) in
/// `models/nlq.mdl` field-for-field, except `metric`/`window`/`resolution`/
/// `visualization_hint` (Phase 1 backlog item 8: `Option<T>` without
/// `skip_serializing_if` can't be generated) and `signals` (Phase 1 backlog
/// item 9: `array<enum(...))` emits invalid TypeScript, modeled as
/// `array<string>`) — see
/// `docs/superpowers/specs/2026-06-15-nlq-visualization-modelable-migration-design.md`.
pub struct NlqIr {
```

```rust
/// Mirrors the inline `enum(...)` used for `nlq.NlqIr.operation` in
/// `models/nlq.mdl`.
pub enum NlqOperation {
```

```rust
/// Mirrors `nlq.NlqIr.signals: array<string>` in `models/nlq.mdl` (Phase 1
/// backlog item 9 — not modeled as `array<enum(...))`, kept as a real Rust
/// enum here).
pub enum NlqSignal {
```

```rust
/// Mirrors `nlq.NlqFilter` (value) in `models/nlq.mdl`, except `op` is
/// `string` not `enum(...)` (Phase 1 backlog item 6 — comparison-symbol
/// variants aren't valid modelable identifiers).
pub struct NlqFilter {
```

```rust
/// Mirrors `nlq.NlqTimeRange` (value) in `models/nlq.mdl` field-for-field.
pub struct NlqTimeRange {
```

In `libs/domain/src/visualization.rs`:

```rust
/// Mirrors `nlq.FieldRole` (value) in `models/nlq.mdl` field-for-field.
pub struct FieldRole {
```

```rust
/// Mirrors the inline `enum(...)` used for `nlq.FieldRole.role` in
/// `models/nlq.mdl`.
pub enum FieldRoleKind {
```

No other Rust changes — `VisualizationFrame`, `VisualizationFrameType`, `NlqVisualizationHint`, `NlqFilterOp`, `EnvelopePayload`/`TelemetryEnvelope` are untouched (each is a named exception per Non-Goals/backlog items above; `VisualizationFrame`/`VisualizationFrameType`/`NlqVisualizationHint` get no comment since they have no `.mdl` counterpart at all).

## Verification

- modelable: `cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable validate C:\git\Observable\models` passes.
- No lineage proof needed — all four `nlq.mdl` types are unversioned `value` kinds (same as `DashboardPanel`/`DashboardPanelLayout` in 3.8).
- Frontend: `npm run typecheck && npm run lint && npm test && npm run build`.
- Rust: `cargo fmt --all && cargo check` for `domain` (doc-comment-only change).
- Full: `bash scripts/local-ci.sh`.
- Mark Phase 3 step 3.9 done in `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`.
- Add backlog items 8 and 9 to the "Phase 1 backlog" section of the migration plan.
