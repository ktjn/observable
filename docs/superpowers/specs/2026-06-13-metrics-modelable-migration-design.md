# Metrics Domain: Modelable Migration Design

**Date:** 2026-06-13
**Status:** Approved
**Scope:** Phase 3 step 3.2 of `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md` — author `models/metrics.mdl` for the `MetricPoint` entity, generate and commit TypeScript artifacts, and wire them into `apps/frontend/src/api/metrics.ts`. Document why `MetricPointRow`, `MetricSeries`/`MetricSeriesRow`, and the query-api aggregation types remain hand-written.

## Context

`libs/domain/src/metric.rs` defines four types relevant to Phase 3 scoping:

- `MetricSeries` / `MetricSeriesRow` — includes `MetricType` and `AggregationTemporality`, real Rust enums with `#[serde(rename_all = "snake_case")]` and ClickHouse string encoding.
- `MetricPoint` / `MetricPointRow` — a clean 1:1 pair. `MetricPoint` has `Option<Vec<u64>>`/`Option<Vec<f64>>` histogram fields; `MetricPointRow` has non-optional `Vec<u64>`/`Vec<f64>` (ClickHouse `Array(UInt64) DEFAULT []` / `Array(Float64) DEFAULT []`), with hand-written `non_empty()`/`unwrap_or_default()` conversions between the two representing "empty array == absent" (locked in by tests at `libs/domain/src/metric.rs:233-286`).

`services/query-api/src/metrics.rs` additionally defines `MetricCatalogEntry`/`MetricCatalogRow` (GROUP BY catalog query) and `MetricGroupPointRow`→`MetricPoint` (aggregation JOIN, not 1:1 — sets `metric_series_id: Uuid::nil()`, drops histogram fields).

`apps/frontend/src/api/metrics.ts` hand-writes a `MetricPoint` interface that has drifted from the Rust struct in the same way `LogRecord` had before 3.1: `time_unix_nano: number | string` and `start_time_unix_nano?: number | string | null` (vs. the real wire format, which is a plain JSON number per ADR-030 non-compliance, same as `Span`/`LogRecord`), and `value_double?`/`value_int?`/`histogram_count?`/`histogram_sum?` all include `| null` (vs. modelable's `?: T` convention for `Option<T>`).

### Why only `MetricPoint`, and only TypeScript

During brainstorming, scope was narrowed in two steps:

1. **`MetricSeries`/`MetricSeriesRow` and the query-api aggregation types are out of scope.** `MetricSeries.metric_type`/`aggregation_temporality` are real Rust enums, but modelable's Rust emitter (`cli/src/modelable/emitters/rust.py`, `_shape_base_annotation`) currently emits all `enum(...)` IDL types as `String`, which would lose type safety. `MetricCatalogEntry`/`MetricGroupPointRow` are handler-level aggregation types with no 1:1 entity/projection equivalent — same rationale as `FacetValue`/`LogListResponse` in 3.1.
2. **`MetricPointRow` cannot be faithfully represented in modelable today**, confirmed via an experimental `.mdl` + compile:
   - `@wire(rust.type: "u64")` on an `array<int>` field (needed for `histogram_bucket_counts: Vec<u64>`) is a **hard validation error** ("only supports rust.type on int fields") — there is no element-type hint for arrays.
   - Even without that hint, a projection field's optionality is inherited from the source entity field. Since `MetricPoint.histogram_bucket_counts` is `Option<Vec<u64>>`, the generated `MetricPointRow.histogram_bucket_counts` comes out as `Option<Vec<i64>>` — not the real non-optional `Vec<u64>` (ClickHouse `Array(UInt64) DEFAULT []`). There is no way to declare a non-optional, default-empty array projection field.

   Defining an inaccurate `MetricPointRow` projection purely for "documentation" would misrepresent the real type, so it is omitted entirely. `MetricPoint` alone — without `histogram_bucket_counts`/`histogram_explicit_bounds` array-hint issues affecting its *entity*-level shape — generates cleanly (see below).

## Goal

- Add `models/metrics.mdl` defining `metrics.MetricPoint@1` (canonical entity), mirroring `libs/domain/src/metric.rs:40-53` field-for-field.
- Generate and commit TypeScript artifacts (`apps/frontend/src/api/generated/metrics/`); re-export `MetricPoint` from `apps/frontend/src/api/metrics.ts`.
- Fix the resulting type-tightening fallout in the frontend.
- Add a doc comment on `libs/domain/src/metric.rs`'s `MetricPoint` cross-referencing `models/metrics.mdl` (`MetricPoint@1`) for lineage tracking. No Rust code changes beyond this comment — `MetricPoint`/`MetricPointRow`/`MetricSeries`/`MetricSeriesRow` and all their `From` impls are unchanged.
- Record the three modelable gaps below as Phase 1 backlog items in the migration plan, as prerequisites for a future `MetricPointRow`/`MetricSeries`/`MetricSeriesRow` migration.

## Non-Goals

- `MetricSeries`/`MetricSeriesRow`, `MetricCatalogEntry`/`MetricCatalogRow`, `MetricGroupPointRow` — all remain hand-written (see Context).
- `MetricPointRow` — remains hand-written; not modeled in `.mdl` this round (see Context).
- Any change to `services/query-api/src/metrics.rs` handler logic, SQL, or ClickHouse DDL.
- Bumping the modelable pin — `models/requirements.txt` is already `modelable==0.4.0` and has everything `MetricPoint@1` needs.

## Design

### 1. `models/metrics.mdl`

New file:

```
domain metrics {
  owner: "platform-team"

  // Canonical metric-data-point entity. Mirrors libs/domain/src/metric.rs's
  // MetricPoint field-for-field. MetricPointRow is intentionally NOT modeled
  // here — see docs/superpowers/specs/2026-06-13-metrics-modelable-migration-design.md
  // for why (array-element rust.type hints and non-optional array projection
  // fields are not yet supported by modelable).
  @wire(json.fieldCase: "snake_case")
  entity MetricPoint @ 1 (additive) {
    tenantId: uuid
    metricSeriesId: uuid
    metricName: string
    serviceName: string
    @wire(rust.type: "u64")
    timeUnixNano: int
    @wire(rust.type: "u64")
    startTimeUnixNano?: int
    valueDouble?: float
    valueInt?: int
    @wire(rust.type: "u64")
    histogramCount?: int
    histogramSum?: float
    histogramBucketCounts?: array<int>
    histogramExplicitBounds?: array<float>
  }
}
```

Field-by-field, this mirrors `MetricPoint` in `libs/domain/src/metric.rs:40-53`: `tenant_id`/`metric_series_id` (UUID), `metric_name`/`service_name` (string), `time_unix_nano` (`u64`), `start_time_unix_nano` (`Option<u64>`), `value_double` (`Option<f64>`), `value_int` (`Option<i64>`, default `int` mapping — no hint needed), `histogram_count` (`Option<u64>`), `histogram_sum` (`Option<f64>`), `histogram_bucket_counts`/`histogram_explicit_bounds` (`Option<Vec<u64>>`/`Option<Vec<f64>>` — no `rust.type` hint on the array fields themselves, since that's the validation error described above; the *entity*-level TypeScript shape for these fields doesn't need the hint, only a future Rust `MetricPointRow` would).

No `projection`/`binding` blocks — `MetricPointRow` is not modeled (see Non-Goals).

**One open item to resolve during generation (Task 1 of the implementation plan):** confirm `histogramBucketCounts?: array<int>` / `histogramExplicitBounds?: array<float>` generate as `histogram_bucket_counts?: number[]` / `histogram_explicit_bounds?: number[]` in TypeScript (expected, since array-element type doesn't depend on the missing `rust.type` hint for the TS emitter). If generation differs, this design doc will be updated with the actual outcome before later tasks depend on it.

### 2. Generated TypeScript artifacts

New directory `apps/frontend/src/api/generated/metrics/`, containing `metrics.MetricPoint.v1.ts` (copied verbatim from `modelable compile models --target typescript`, same regen-header-comment convention as `logs.LogRecord.v1.ts`). Expected content (per the `@wire(rust.type: "u64")` → TS `number` precedent from `logs.LogRecord.v1.ts`/`tracing.Span.v1.ts`):

```typescript
export interface MetricsMetricPointV1 {
  tenant_id: string;
  metric_series_id: string;
  metric_name: string;
  service_name: string;
  time_unix_nano: number;
  start_time_unix_nano?: number;
  value_double?: number;
  value_int?: number;
  histogram_count?: number;
  histogram_sum?: number;
  histogram_bucket_counts?: number[];
  histogram_explicit_bounds?: number[];
}
export type MetricPoint = MetricsMetricPointV1;
```

In `apps/frontend/src/api/metrics.ts`, replace the hand-written `MetricPoint` interface (lines 22-35) with:

```typescript
export type { MetricPoint } from "./generated/metrics/metrics.MetricPoint.v1";
```

All other exports in `metrics.ts` (`MetricCatalogEntry`, `MetricCatalogResponse`, `MetricPointsResponse`, `listMetrics`, `getMetricGroupPoints`) are unchanged.

### 3. Type-fallout fixes

The generated `MetricPoint` drops `| string` from `time_unix_nano`/`start_time_unix_nano` and `| null` from `value_double`/`value_int`/`histogram_count`/`histogram_sum` (matching the `Option<T>` → `?: T` convention used for `LogRecord.fingerprint` in 3.1).

`MetricPoint` is only referenced within `apps/frontend/src/api/metrics.ts` itself (via `MetricPointsResponse.points: MetricPoint[]`) — `grep` confirms no other frontend file imports it. `getMetricGroupPoints`/`listMetrics` both return `res.json()` (typed `any`, not checked against the interface), and the only consumer, `apps/frontend/src/features/metrics/ServiceMetricsWorkspace.tsx:306-307` (`Number(p.time_unix_nano)`, `p.value_double ?? p.value_int ?? 0`), is compatible with both the old and new types (`Number(number)` and `??` both work identically with `T | undefined` as with `T | null | undefined`).

`apps/frontend/src/App.test.tsx`'s `/v1/metrics/points` mock fixture (lines 377-413, including `start_time_unix_nano: null`, `value_int: null`, etc.) is `JSON.stringify`'d into a mock fetch response and is not type-annotated as `MetricPoint` — no change needed (same as the 3.1 precedent for `LogRecord` fixtures in this file).

Expected fallout: **none**, beyond running `npm run typecheck` to confirm.

### 4. Phase 1 backlog (modelable gaps blocking future Metrics work)

Add to `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`, as prerequisites for migrating `MetricPointRow`/`MetricSeries`/`MetricSeriesRow`:

1. **Array-element `rust.type` hints.** `@wire(rust.type: "u64")` on an `array<int>` field should apply to the element type (`Vec<u64>`), not error. Needed for `MetricPointRow.histogram_bucket_counts: Vec<u64>`.
2. **Non-optional, default-empty array projection fields.** A projection field mapped from an optional source field should be able to declare itself non-optional with an implicit empty-array default, to match ClickHouse `Array(T) DEFAULT []` columns. Needed for `MetricPointRow.histogram_bucket_counts`/`histogram_explicit_bounds`.
3. **Real Rust enum emission for `enum(...)` IDL types.** `_shape_base_annotation` currently emits all `enum(...)` shapes as `String`. Needed for `MetricSeries.metric_type: MetricType` / `aggregation_temporality: Option<AggregationTemporality>` to remain real enums with `#[serde(rename_all = "snake_case")]`.

## Verification

- modelable: `cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable validate C:\git\Observable\models` passes.
- Lineage proof: `modelable lineage metrics.MetricPoint@1`, included in the commit/PR description.
- Frontend: `npm run typecheck && npm run lint && npm test && npm run build` from `apps/frontend/`.
- Full: `bash scripts/local-ci.sh`.
- Mark Phase 3 step 3.2 done in `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`, including the Phase 1 backlog note.
