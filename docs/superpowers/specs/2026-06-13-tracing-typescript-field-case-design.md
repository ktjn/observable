# Tracing Domain: TypeScript Field-Case Hint + Generated Types Design

**Date:** 2026-06-13
**Status:** Approved
**Scope:** Phase 2 step 2.5 of `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md` — adopt modelable v0.4.0's `@wire(json.fieldCase: "snake_case")` hint for `tracing.Span@1`/`tracing.SpanEvent@1`, generate TypeScript for those two entities, and wire the generated types into `apps/frontend/src/api/traces.ts`.

## Context

modelable v0.4.0 (released from `https://github.com/ktjn/modelable`, tag `v0.4.0`) adds a `@wire(json.fieldCase: "<case>")` hint that tells the TypeScript emitter to rename all of a model/projection declaration's fields to a given case, without affecting Rust/JSON-Schema/SQL/lineage output.

`models/tracing.mdl` declares `tracing.Span@1`/`tracing.SpanEvent@1` with camelCase field names (e.g. `spanId`, `startTimeUnixNano`). The Rust emitter converts these to snake_case for the hand-authored `Span`/`SpanEvent` structs in `libs/domain/src/span.rs`, and serde's default casing serializes them to JSON as snake_case — so the real wire format is snake_case (`span_id`, `start_time_unix_nano`), matching the hand-written `apps/frontend/src/api/traces.ts` interfaces today.

Without the new hint, `modelable compile --target typescript` would emit camelCase field names for `tracing.Span@1`/`tracing.SpanEvent@1`, which would not match the real API response. The hint closes this gap.

This is the first time Observable wires modelable-generated TypeScript into the frontend build (Rust generated artifacts already exist and are committed under `libs/domain/src/generated/tracing/`).

## Goal

- Add `@wire(json.fieldCase: "snake_case")` to `tracing.Span@1`/`tracing.SpanEvent@1` so generated TypeScript field names match the real JSON wire format.
- Generate and commit TypeScript interfaces for `Span` and `SpanEvent`, mirroring the committed-generated-Rust convention.
- Replace the hand-written `Span`/`SpanEvent` interfaces in `apps/frontend/src/api/traces.ts` with re-exports of the generated types, with zero behavior change.

## Non-Goals

- Modeling `TraceResponse`, `FacetValue`, `Facets`, `TraceListResponse` in modelable — these are API-response wrappers, not canonical domain entities, and stay hand-written.
- Generating/committing `tracing.SpanRow.v1.ts` / `tracing.SpanEventRow.v1.ts` — these are ClickHouse storage shapes with no frontend consumer.
- Any change to Rust, JSON Schema, SQL, or lineage output — `json.fieldCase` is TS-emitter-only by design.
- Automating TS regeneration in CI — matches the existing manual-regenerate-and-commit convention used for the Rust generated artifacts (no regen-diff check exists for those either).

## Design

### 1. `models/requirements.txt`

Bump the pin:
```
modelable==0.3.0
```
to:
```
modelable==0.4.0
```

### 2. `models/tracing.mdl`

Add `@wire(json.fieldCase: "snake_case")` immediately before `entity Span @ 1 (additive) {` and `entity SpanEvent @ 1 (additive) {`. The `SpanRow@1`/`SpanEventRow@1` projections are unchanged (their TS output isn't consumed, and per the hint's design a projection doesn't inherit its source model's `json.fieldCase`).

### 3. Generated TypeScript artifacts

New directory: `apps/frontend/src/api/generated/tracing/` (committed).

Contents (copied verbatim from `modelable compile models/ --target typescript --out <scratch-dir>`, using modelable v0.4.0 from `C:\git\modelable\cli\.venv`):
- `tracing.Span.v1.ts` — exports `interface TracingSpanV1` (all fields snake_case per the new hint) and `export type Span = TracingSpanV1`.
- `tracing.SpanEvent.v1.ts` — exports `interface TracingSpanEventV1` and `export type SpanEvent = TracingSpanEventV1`.

Each generated file already carries a `@modelable ...` doc comment header (domain, name, owner, kind, version, changeKind) — no additional header needed beyond what the emitter produces.

Regeneration command (documented in a short comment at the top of `apps/frontend/src/api/generated/tracing/tracing.Span.v1.ts`, since this directory has no README of its own):
```
cd C:\git\modelable\cli
.venv\Scripts\python.exe -m modelable compile C:\git\Observable\models --target typescript --out <scratch-dir>
# then copy tracing.Span.v1.ts and tracing.SpanEvent.v1.ts into apps/frontend/src/api/generated/tracing/
```

### 4. `apps/frontend/src/api/traces.ts`

Replace:
```typescript
export interface Span {
  tenant_id: string;
  ...
}
```
and
```typescript
export interface SpanEvent {
  span_id: string;
  ...
}
```
with:
```typescript
export type { Span } from "./generated/tracing/tracing.Span.v1";
export type { SpanEvent } from "./generated/tracing/tracing.SpanEvent.v1";
```

`TraceResponse`, `FacetValue`, `Facets`, `TraceListResponse` are unchanged (still hand-written, still reference `Span`/`SpanEvent`). All existing `import type { Span, SpanEvent, ... } from "../api/traces"` (or relative equivalents) across the frontend keep working unchanged, since `traces.ts` continues to export `Span` and `SpanEvent` by name.

### 5. Type-fallout fixes

Switching `Span`/`SpanEvent` to the generated interfaces tightens two things vs. the current hand-written interfaces:

- **`span_kind` / `status_code`** go from `string` to literal unions: `'INTERNAL' | 'SERVER' | 'CLIENT' | 'PRODUCER' | 'CONSUMER'` and `'UNSET' | 'OK' | 'ERROR'` (driven by the existing `@wire(json.case: "SCREAMING_SNAKE_CASE")` enum hints, independent of this change's `fieldCase` hint — modelable already emitted these as literal unions before this change).
- **`attributes` / `resource_attributes`** go from optional (`attributes?: Record<string, unknown>`) to required (`attributes: Record<string, unknown>`), matching the real Rust struct field (`HashMap<String, serde_json::Value>`, never absent — serializes to `{}` at minimum).

Known fallout, to fix as part of this change:

`apps/frontend/src/pages/TraceSearch.tsx`, function `nlqRowToTraceResponse` (~line 66-89), constructs a synthetic `Span` for a row returned by an NLQ query:
- `span_kind: ""` — not a valid member of the literal union. Replace with a valid placeholder, e.g. `"INTERNAL"` (this synthetic span represents a trace's root span; `"INTERNAL"` is an inert default since the UI doesn't otherwise rely on this synthetic span's `span_kind`).
- `status_code: row.status_code` — `row.status_code` is `string` (from `NlqTraceRow`), not assignable to `'UNSET' | 'OK' | 'ERROR'`. Cast: `status_code: row.status_code as Span["status_code"]` (the NLQ query reads this column from the same ClickHouse `status_code` enum-backed column, so at runtime it's always one of the three values).
- Missing `attributes` and `resource_attributes` — add `attributes: {}, resource_attributes: {}`.

After this fix, run `npm run typecheck` to catch any other fallout from the literal-union/required-field changes across the frontend (tests and components). Fix any additional cases found using the same patterns (valid enum placeholder / cast for enum fields backed by real ClickHouse columns, `{}` for attributes maps).

## Verification

- modelable side: `cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable validate C:\git\Observable\models` passes with the 0.4.0 pin and the new hint.
- Diff-check: `modelable compile C:\git\Observable\models --target rust --out <scratch-dir>` produces output identical to `libs/domain/src/generated/tracing/` (confirms `json.fieldCase` is TS-only, no Rust regen needed).
- Frontend: `npm run typecheck`, `npm run lint`, `npm test`, `npm run build` all pass from `apps/frontend/`.
- Manual spot-check: generated `tracing.Span.v1.ts` / `tracing.SpanEvent.v1.ts` field names match the field names in the pre-change hand-written `Span`/`SpanEvent` interfaces (same wire format, just now generated).
