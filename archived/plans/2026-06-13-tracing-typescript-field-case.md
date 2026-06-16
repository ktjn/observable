# Tracing TypeScript Field-Case Hint Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add modelable v0.4.0's `@wire(json.fieldCase: "snake_case")` hint to `tracing.Span@1`/`tracing.SpanEvent@1`, generate and commit TypeScript for those two entities, and wire the generated types into `apps/frontend/src/api/traces.ts` with zero behavior change.

**Architecture:** Bump the modelable pin and annotate `models/tracing.mdl`, validate with the new pin, generate TS via the modelable CLI checkout at `C:\git\modelable\cli\.venv`, commit the generated files under `apps/frontend/src/api/generated/tracing/`, then replace the hand-written `Span`/`SpanEvent` interfaces in `traces.ts` with re-exports and fix the resulting type-fallout.

**Tech Stack:** modelable v0.4.0 (Python CLI, used only to regenerate committed files), TypeScript, Vite/vitest.

---

### Task 1: Bump modelable pin and add `@wire(json.fieldCase: "snake_case")`

**Files:**
- Modify: `models/requirements.txt`
- Modify: `models/tracing.mdl:16` and `models/tracing.mdl:82`

- [ ] **Step 1: Bump the pin**

In `models/requirements.txt`, change:
```
modelable==0.3.0
```
to:
```
modelable==0.4.0
```

- [ ] **Step 2: Add the hint to `tracing.Span@1`**

In `models/tracing.mdl`, find:
```
  // Canonical span entity. Fields are camelCase in the IDL; the Rust emitter
  // converts to snake_case. attributes / resourceAttributes are map<string, json>,
  // matching the hand-authored domain struct's HashMap<String, serde_json::Value>.
  entity Span @ 1 (additive) {
```
Replace with:
```
  // Canonical span entity. Fields are camelCase in the IDL; the Rust emitter
  // converts to snake_case. attributes / resourceAttributes are map<string, json>,
  // matching the hand-authored domain struct's HashMap<String, serde_json::Value>.
  // @wire(json.fieldCase: "snake_case") makes the generated TypeScript fields
  // snake_case too, matching the real (Rust-serialized) JSON wire format.
  @wire(json.fieldCase: "snake_case")
  entity Span @ 1 (additive) {
```

- [ ] **Step 3: Add the hint to `tracing.SpanEvent@1`**

In `models/tracing.mdl`, find:
```
  // Canonical span-event entity. event_index is u32 in the hand-authored struct;
  // @wire(rust.type: "u32") propagates the override to generated Rust.
  entity SpanEvent @ 1 (additive) {
```
Replace with:
```
  // Canonical span-event entity. event_index is u32 in the hand-authored struct;
  // @wire(rust.type: "u32") propagates the override to generated Rust.
  // @wire(json.fieldCase: "snake_case") makes the generated TypeScript fields
  // snake_case too, matching the real (Rust-serialized) JSON wire format.
  @wire(json.fieldCase: "snake_case")
  entity SpanEvent @ 1 (additive) {
```

- [ ] **Step 4: Validate with modelable 0.4.0**

Run:
```bash
cd /c/git/modelable/cli
.venv/Scripts/python.exe -m modelable validate /c/git/Observable/models
```
Expected: `OK ...\tracing.mdl is valid.` (no errors — `json.fieldCase` is valid at entity level per modelable v0.4.0).

- [ ] **Step 5: Commit**

```bash
git add models/requirements.txt models/tracing.mdl
git commit -m "chore(models): bump modelable pin to 0.4.0, add json.fieldCase hint

@wire(json.fieldCase: \"snake_case\") on tracing.Span@1/SpanEvent@1 makes
the generated TypeScript field names match the real (Rust-serialized)
snake_case JSON wire format. TS-emitter-only; no Rust/SQL/lineage change."
```

---

### Task 2: Generate and commit TypeScript for `tracing.Span@1`/`tracing.SpanEvent@1`

**Files:**
- Create: `apps/frontend/src/api/generated/tracing/tracing.Span.v1.ts`
- Create: `apps/frontend/src/api/generated/tracing/tracing.SpanEvent.v1.ts`

- [ ] **Step 1: Generate TypeScript to a scratch directory**

Run:
```bash
cd /c/git/modelable/cli
rm -rf /tmp/tracing-ts-out
.venv/Scripts/python.exe -m modelable compile /c/git/Observable/models --target typescript --out /tmp/tracing-ts-out
```
Expected: writes 4 files — `tracing.Span.v1.ts`, `tracing.SpanEvent.v1.ts`, `tracing.SpanRow.v1.ts`, `tracing.SpanEventRow.v1.ts`. Only the first two are needed (the Row types are ClickHouse storage shapes with no frontend consumer).

- [ ] **Step 2: Verify the generated `tracing.Span.v1.ts` content**

Run:
```bash
cat /tmp/tracing-ts-out/tracing.Span.v1.ts
```
Expected output (all field names snake_case, `span_kind`/`status_code` as literal unions, `attributes`/`resource_attributes` required):
```typescript
/**
 * @modelable domain: tracing
 * @modelable name: Span
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface TracingSpanV1 {
  span_id: string;
  trace_id: string;
  parent_span_id?: string;
  tenant_id: string;
  service_name: string;
  service_namespace: string;
  service_version: string;
  operation_name: string;
  span_kind: 'INTERNAL' | 'SERVER' | 'CLIENT' | 'PRODUCER' | 'CONSUMER';
  start_time_unix_nano: number;
  end_time_unix_nano: number;
  duration_ns: number;
  status_code: 'UNSET' | 'OK' | 'ERROR';
  status_message: string;
  attributes: Record<string, unknown>;
  resource_attributes: Record<string, unknown>;
  environment: string;
  host_id: string;
  workload: string;
  deployment_id: string;
}
export type Span = TracingSpanV1;
```

- [ ] **Step 3: Create `apps/frontend/src/api/generated/tracing/tracing.Span.v1.ts`**

Write the file with the generated content from Step 2, plus a one-line regeneration comment at the top:

```typescript
// Generated by modelable — do not edit by hand.
// Regenerate: cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable compile C:\git\Observable\models --target typescript --out <scratch-dir>
// then copy tracing.Span.v1.ts and tracing.SpanEvent.v1.ts into this directory.
/**
 * @modelable domain: tracing
 * @modelable name: Span
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface TracingSpanV1 {
  span_id: string;
  trace_id: string;
  parent_span_id?: string;
  tenant_id: string;
  service_name: string;
  service_namespace: string;
  service_version: string;
  operation_name: string;
  span_kind: 'INTERNAL' | 'SERVER' | 'CLIENT' | 'PRODUCER' | 'CONSUMER';
  start_time_unix_nano: number;
  end_time_unix_nano: number;
  duration_ns: number;
  status_code: 'UNSET' | 'OK' | 'ERROR';
  status_message: string;
  attributes: Record<string, unknown>;
  resource_attributes: Record<string, unknown>;
  environment: string;
  host_id: string;
  workload: string;
  deployment_id: string;
}
export type Span = TracingSpanV1;
```

- [ ] **Step 4: Verify the generated `tracing.SpanEvent.v1.ts` content**

Run:
```bash
cat /tmp/tracing-ts-out/tracing.SpanEvent.v1.ts
```
Expected output:
```typescript
/**
 * @modelable domain: tracing
 * @modelable name: SpanEvent
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface TracingSpanEventV1 {
  tenant_id: string;
  trace_id: string;
  span_id: string;
  event_index: number;
  name: string;
  timestamp_unix_nano: number;
  attributes: Record<string, unknown>;
}
export type SpanEvent = TracingSpanEventV1;
```

- [ ] **Step 5: Create `apps/frontend/src/api/generated/tracing/tracing.SpanEvent.v1.ts`**

```typescript
// Generated by modelable — do not edit by hand.
// Regenerate: cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable compile C:\git\Observable\models --target typescript --out <scratch-dir>
// then copy tracing.Span.v1.ts and tracing.SpanEvent.v1.ts into this directory.
/**
 * @modelable domain: tracing
 * @modelable name: SpanEvent
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface TracingSpanEventV1 {
  tenant_id: string;
  trace_id: string;
  span_id: string;
  event_index: number;
  name: string;
  timestamp_unix_nano: number;
  attributes: Record<string, unknown>;
}
export type SpanEvent = TracingSpanEventV1;
```

- [ ] **Step 6: Commit**

```bash
git add apps/frontend/src/api/generated/tracing/tracing.Span.v1.ts apps/frontend/src/api/generated/tracing/tracing.SpanEvent.v1.ts
git commit -m "feat(frontend): add modelable-generated tracing Span/SpanEvent types

Generated from models/tracing.mdl via modelable v0.4.0, with
@wire(json.fieldCase: \"snake_case\") so field names match the real
JSON wire format. Not yet wired into traces.ts."
```

---

### Task 3: Wire generated types into `apps/frontend/src/api/traces.ts`

**Files:**
- Modify: `apps/frontend/src/api/traces.ts:1-30`

- [ ] **Step 1: Replace the hand-written `Span` and `SpanEvent` interfaces**

In `apps/frontend/src/api/traces.ts`, the file currently starts with (lines 1-30):
```typescript
export interface Span {
  tenant_id: string;
  trace_id: string;
  span_id: string;
  parent_span_id?: string;
  service_name: string;
  service_namespace: string;
  service_version: string;
  operation_name: string;
  span_kind: string;
  start_time_unix_nano: number;
  end_time_unix_nano: number;
  duration_ns: number;
  status_code: string;
  status_message: string;
  attributes?: Record<string, unknown>;
  resource_attributes?: Record<string, unknown>;
  environment: string;
  host_id: string;
  workload: string;
  deployment_id: string;
}

export interface SpanEvent {
  span_id: string;
  event_index: number;
  name: string;
  timestamp_unix_nano: number;
  attributes?: Record<string, unknown>;
}

export interface TraceResponse {
```

Replace lines 1-30 (everything from `export interface Span {` through the closing `}` of `SpanEvent`, inclusive of the blank line before `export interface TraceResponse {`) with:
```typescript
import type { Span } from "./generated/tracing/tracing.Span.v1";
import type { SpanEvent } from "./generated/tracing/tracing.SpanEvent.v1";

export type { Span, SpanEvent };

export interface TraceResponse {
```

The rest of the file (`TraceResponse`, `FacetValue`, `Facets`, `TraceListResponse`, `tenantHeaders`, `TraceHistogramBucket`, `TraceHistogramResponse`, and the fetch functions) is unchanged — `TraceResponse.spans: Span[]` and `TraceResponse.events: SpanEvent[]` now reference the imported generated types.

- [ ] **Step 2: Confirm the file compiles in isolation (expect new errors — fixed in Task 4)**

Run:
```bash
cd /c/git/Observable/apps/frontend
npm run typecheck
```
Expected: FAILS with type errors in `src/pages/TraceSearch.tsx` (the known fallout from Task 4) — confirms the re-export wiring itself is structurally correct (no errors pointing at `traces.ts` itself or at "module not found").

- [ ] **Step 3: Commit**

```bash
git add apps/frontend/src/api/traces.ts
git commit -m "feat(frontend): wire generated Span/SpanEvent types into traces.ts

Span and SpanEvent are now re-exported from the modelable-generated
apps/frontend/src/api/generated/tracing/ files instead of being
hand-written. TraceResponse/FacetValue/Facets/TraceListResponse remain
hand-written (not modeled in modelable)."
```

---

### Task 4: Fix type fallout from literal-union and required-field changes

**Files:**
- Modify: `apps/frontend/src/pages/TraceSearch.tsx:66-89`

- [ ] **Step 1: Fix `nlqRowToTraceResponse`**

In `apps/frontend/src/pages/TraceSearch.tsx`, find:
```typescript
/** Maps a flat NLQ trace row to the TraceResponse shape used by TraceResultsTable. */
function nlqRowToTraceResponse(row: NlqTraceRow): TraceResponse {
  return {
    trace_id: row.trace_id,
    spans: [
      {
        tenant_id: "",
        trace_id: row.trace_id,
        span_id: "",
        service_name: row.root_service,
        service_namespace: "",
        service_version: "",
        operation_name: row.root_operation,
        span_kind: "",
        start_time_unix_nano: Number(row.start_time_unix_nano),
        end_time_unix_nano: Number(row.start_time_unix_nano) + row.duration_ms * 1_000_000,
        duration_ns: row.duration_ms * 1_000_000,
        status_code: row.status_code,
        status_message: "",
        environment: row.environment ?? "",
        host_id: "",
        workload: "",
        deployment_id: "",
      },
    ],
```

Replace with:
```typescript
/** Maps a flat NLQ trace row to the TraceResponse shape used by TraceResultsTable. */
function nlqRowToTraceResponse(row: NlqTraceRow): TraceResponse {
  return {
    trace_id: row.trace_id,
    spans: [
      {
        tenant_id: "",
        trace_id: row.trace_id,
        span_id: "",
        service_name: row.root_service,
        service_namespace: "",
        service_version: "",
        operation_name: row.root_operation,
        // Synthetic root span has no real span kind; "INTERNAL" is an inert
        // default — the UI doesn't read span_kind for this synthetic span.
        span_kind: "INTERNAL",
        start_time_unix_nano: Number(row.start_time_unix_nano),
        end_time_unix_nano: Number(row.start_time_unix_nano) + row.duration_ms * 1_000_000,
        duration_ns: row.duration_ms * 1_000_000,
        // status_code comes from the same ClickHouse status_code enum column
        // as Span.status_code, so it's always one of the three variants.
        status_code: row.status_code as Span["status_code"],
        status_message: "",
        attributes: {},
        resource_attributes: {},
        environment: row.environment ?? "",
        host_id: "",
        workload: "",
        deployment_id: "",
      },
    ],
```

Note: `Span` must be in scope in this file for the `Span["status_code"]` type reference — check the existing imports at the top of `TraceSearch.tsx` for `import type { ... } from "../api/traces"` and add `Span` to that import list if it's not already there.

- [ ] **Step 2: Run typecheck and fix any remaining fallout**

Run:
```bash
cd /c/git/Observable/apps/frontend
npm run typecheck
```

If this still fails, the remaining errors will point at other places constructing `Span`/`SpanEvent`/`TraceResponse` object literals that are missing `attributes`/`resource_attributes`, or assigning non-enum strings to `span_kind`/`status_code`. Fix each using the same patterns as Step 1:
- Missing `attributes`/`resource_attributes`: add `attributes: {}, resource_attributes: {}` (or real values if available).
- Invalid `span_kind`/`status_code` literal: use a valid enum member (`"INTERNAL"`, `"OK"`, etc.) or `as Span["span_kind"]` / `as Span["status_code"]` if the value is known-valid at runtime but typed as plain `string`.

Expected after fixes: `npm run typecheck` exits 0 with no errors.

- [ ] **Step 3: Run lint and tests**

Run:
```bash
cd /c/git/Observable/apps/frontend
npm run lint
npm test
```
Expected: both exit 0. (`npm test` runs vitest; existing `TraceSearch.test.tsx`, `TraceDetail.test.tsx`, `TraceCompare.test.tsx`, `TraceResultsTable.test.tsx` etc. should pass unchanged since the wire format is identical to before.)

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/pages/TraceSearch.tsx
git commit -m "fix(frontend): fix type fallout from generated Span/SpanEvent types

span_kind/status_code are now literal unions and attributes/
resource_attributes are required, matching the real Rust wire types.
Update the synthetic root span built in nlqRowToTraceResponse
accordingly — no runtime behavior change."
```

(If Step 2 required additional fixes beyond `TraceSearch.tsx`, add those files to this commit too.)

---

### Task 5: Full verification and mark step 2.5 done

**Files:**
- Modify: `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md:83`

- [ ] **Step 1: Confirm Rust output is unchanged (json.fieldCase is TS-only)**

Run:
```bash
cd /c/git/modelable/cli
rm -rf /tmp/tracing-rust-out
.venv/Scripts/python.exe -m modelable compile /c/git/Observable/models --target rust --out /tmp/tracing-rust-out
diff -r /tmp/tracing-rust-out/tracing /c/git/Observable/libs/domain/src/generated/tracing
```
Expected: no diff output (identical files) — confirms `@wire(json.fieldCase: ...)` did not change Rust output.

- [ ] **Step 2: Run full frontend build**

Run:
```bash
cd /c/git/Observable/apps/frontend
npm run build
```
Expected: exits 0.

- [ ] **Step 3: Run local CI**

Run:
```bash
cd /c/git/Observable
bash scripts/local-ci.sh
```
Expected: `modelable validate` step passes (uses the 0.4.0 pin from Task 1; if `modelable` isn't on `PATH` this step is skipped — that's fine, Task 1 Step 4 already validated with the 0.4.0 pin directly).

- [ ] **Step 4: Mark step 2.5 done in the migration plan**

In `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`, find:
```
- [ ] **2.5** Generate TypeScript and replace `apps/frontend/src/api/traces.ts:1-43` (`Span`, `SpanEvent`, `TraceResponse`, `FacetValue`, `Facets`, `TraceListResponse`); confirm with `apps/frontend/package.json` how generated output is wired into the build (new `generated/` import path vs. existing conventions).
```
Replace with:
```
- [x] **2.5** Generated TypeScript for `tracing.Span@1`/`tracing.SpanEvent@1` (enabled by modelable v0.4.0's `@wire(json.fieldCase: "snake_case")` hint — see `docs/superpowers/specs/2026-06-13-tracing-typescript-field-case-design.md`) and committed it under `apps/frontend/src/api/generated/tracing/`. `apps/frontend/src/api/traces.ts`'s `Span`/`SpanEvent` interfaces are now re-exports of the generated types. `TraceResponse`, `FacetValue`, `Facets`, `TraceListResponse` remain hand-written — same rationale as 2.4 (handler-level aggregation/wrapper types with no 1:1 generated equivalent).
```

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md
git commit -m "docs: mark Phase 2 step 2.5 (generated Span/SpanEvent TS types) done"
```
