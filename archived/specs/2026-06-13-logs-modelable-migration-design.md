# Logs Domain: Modelable Migration Design

**Date:** 2026-06-13
**Status:** Approved
**Scope:** Phase 3 step 3.1 of `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md` — author `models/logs.mdl` for the `LogRecord` entity and `LogRow` ClickHouse projection, generate and commit Rust and TypeScript artifacts, and wire them into `libs/domain/src/log.rs` and `apps/frontend/src/api/logs.ts`.

## Context

This is the first Phase 3 domain, following the template established by the tracing pilot (Phase 2, `docs/superpowers/specs/2026-06-13-tracing-typescript-field-case-design.md` and the "Modelable Type-Mapping Migration" section of `docs/agent-context.md`).

`libs/domain/src/log.rs` defines a canonical `LogRecord` struct and a `#[cfg(feature = "storage")]` `LogRow` ClickHouse projection, with hand-written `From<LogRecord> for LogRow` / `From<LogRow> for LogRecord` impls that convert enums/JSON to strings — directly analogous to `Span`/`SpanRow` before the tracing migration. `apps/frontend/src/api/logs.ts` hand-writes a `LogRecord` TypeScript interface that has drifted from the Rust struct: several fields that are required/non-optional in Rust (`environment`, `host_id`, `observed_timestamp_unix_nano`, `attributes`, `resource_attributes`) are optional in the TS interface, and `timestamp_unix_nano`/`observed_timestamp_unix_nano` are typed `string` even though `LogRecord`'s actual serde output (no `#[serde(with = ...)]`) serializes `u64` as a JSON number — the same ADR-030 non-compliance already documented for `Span`/tracing.

Unlike tracing, the `observable.logs` table is queried via `SELECT ?fields FROM observable.logs ...` (`services/query-api/src/planner/mod.rs::plan_log_search`), so field reordering in the generated `LogRow` is safe — no `SELECT_COLS`-style fix is needed (this was tracing's step 2.3, which Logs skips).

## Goal

- Add `models/logs.mdl` defining `logs.LogRecord@1` (canonical entity) and `logs.LogRow@1` (ClickHouse projection), mirroring `libs/domain/src/log.rs` field-for-field.
- Generate and commit Rust artifacts (`libs/domain/src/generated/logs/`); make `LogRow` a type alias to the generated row type, same as `SpanRow`/`SpanEventRow`.
- Generate and commit TypeScript artifacts (`apps/frontend/src/api/generated/logs/`); re-export `LogRecord` from `apps/frontend/src/api/logs.ts`.
- Fix the resulting type-tightening fallout (required fields, `number` timestamps) across the frontend test suite.
- One combined PR, since modelable v0.4.0 already has every feature this domain needs (`map<string, json>`, `@wire(json.fieldCase: ...)`, `@wire(rust.type: ...)`, `@wire(clickhouse: ...)`).

## Non-Goals

- Modeling `FacetValue`, `Facets`, `LogListResponse`, `LogHistogramBucket`, `LogHistogramResponse` in modelable — these are handler-level aggregation/wrapper types and stay hand-written, per the per-domain rule.
- Implementing ADR-030's string-encoded-u64 convention for `LogRecord` — the generated TypeScript mirrors the real (non-compliant) wire format, matching the tracing precedent. ADR-030 non-compliance for both `Span` and `LogRecord` remains a documented follow-up, not something this migration fixes.
- Any change to `services/query-api/src/logs.rs` handler logic, SQL, or ClickHouse DDL — this is a type-mapping source-of-truth change only, no behavior change.
- Bumping the modelable pin — `models/requirements.txt` is already `modelable==0.4.0`.

## Design

### 1. `models/logs.mdl`

New file, following the structure of `models/tracing.mdl`:

```
domain logs {
  owner: "platform-team"

  @wire(json.fieldCase: "snake_case")
  entity LogRecord @ 1 (additive) {
    tenantId: uuid
    @key logId: uuid
    @wire(rust.type: "u64")
    timestampUnixNano: int
    @wire(rust.type: "u64")
    observedTimestampUnixNano: int
    @wire(rust.type: "i32")
    severityNumber: int
    severityText: string
    body: json
    traceId?: string
    spanId?: string
    attributes: map<string, json>
    resourceAttributes: map<string, json>
    serviceName: string
    environment: string
    hostId: string
    @wire(rust.type: "u64")
    fingerprint?: int
  }

  projection LogRow @ 1
    from logs.LogRecord @ 1 as l
  {
    @wire(clickhouse: "uuid")
    tenantId <- l.tenantId
    @wire(clickhouse: "uuid")
    logId <- l.logId
    timestampUnixNano <- l.timestampUnixNano
    observedTimestampUnixNano <- l.observedTimestampUnixNano
    severityNumber <- l.severityNumber
    severityText <- l.severityText
    @wire(clickhouse: "string")
    body <- l.body
    traceId <- l.traceId
    spanId <- l.spanId
    @wire(clickhouse: "string")
    attributes <- l.attributes
    @wire(clickhouse: "string")
    resourceAttributes <- l.resourceAttributes
    serviceName <- l.serviceName
    environment <- l.environment
    hostId <- l.hostId
    fingerprint <- l.fingerprint
  }
}

binding ch-observable {
  adapter: clickhouse
}

binding log-binding {
  model: logs.LogRecord @ 1
  adapter: ch-observable
  table: "logs"
}
```

Field-by-field, this mirrors `LogRecord`/`LogRow` in `libs/domain/src/log.rs` exactly: `tenant_id`, `log_id` (both UUID, `@wire(clickhouse: "uuid")` in the projection matching `#[serde(with = "clickhouse::serde::uuid")]`), `timestamp_unix_nano`/`observed_timestamp_unix_nano` (`u64`), `severity_number` (`i32`), `severity_text`, `body` (`serde_json::Value`), `trace_id`/`span_id` (optional strings), `attributes`/`resource_attributes` (`HashMap<String, serde_json::Value>`, `String` in the row via `@wire(clickhouse: "string")` — same pattern as tracing's `attributes`), `service_name`, `environment`, `host_id`, `fingerprint` (`Option<u64>`).

**Two open items to resolve during generation (Task 1 of the implementation plan), not assumed here:**
- Whether `@wire(clickhouse: "string")` works on a bare `json` field (`body`), not just `map<string, json>`. If the generated `LogRowV1.body` isn't `String`, fall back to declaring `body` without the override and keep its JSON-string conversion in `log.rs`'s hand-written `From`/`Into` impls (as is already the case for `attributes`/`resource_attributes` regardless).
- Whether `@wire(rust.type: "i32")` is accepted (only `u32`/`u64` overrides are proven in `tracing.mdl`). If rejected, `severityNumber` stays the modelable default int type and `log.rs` keeps a hand-written cast in its `From` impls (same shape as the current code already has for `i32` ↔ default-int, if any).

If either falls back, this design doc will be updated with the actual outcome before the implementation plan's later tasks depend on it.

### 2. Generated Rust artifacts

New directory `libs/domain/src/generated/logs/`, following the `tracing/` layout exactly:
- `logs.rs` (sibling to `tracing.rs`, declared via `libs/domain/src/generated.rs`: add `pub(crate) mod logs;`)
- `logs/logs_log_record_v1.rs` — `LogsLogRecordV1` (generated, currently unused like `TracingSpanV1`, kept for lineage/dead-code-allowed)
- `logs/logs_log_row_v1.rs` — `LogsLogRowV1` (used)

`logs.rs` header mirrors `tracing.rs`:
```rust
// Generated artifacts for the `logs` domain (models/logs.mdl).
// Regenerate with:
//   modelable compile models --target rust --out <tmp>
// then copy logs.LogRecord.v1.rs / logs.LogRow.v1.rs from <tmp>/logs/ into this
// directory, renaming to snake_case file names. Do not hand-edit the generated files.
#![allow(dead_code, unused_imports, clippy::useless_conversion)]

mod logs_log_record_v1;
#[cfg(feature = "storage")]
mod logs_log_row_v1;

#[cfg(feature = "storage")]
pub(crate) use logs_log_row_v1::LogsLogRowV1;
```

In `libs/domain/src/log.rs`:
- Add `use crate::generated::logs::LogsLogRowV1;` (under `#[cfg(feature = "storage")]`, matching `span.rs`'s import).
- Replace the hand-written `LogRow` struct definition with `pub type LogRow = LogsLogRowV1;`.
- Keep `LogRecord` hand-written (unchanged field set — already matches `.mdl`).
- Keep the hand-written `From<LogRecord> for LogRow` / `From<LogRow> for LogRecord` impls, updated only if the generated `LogsLogRowV1`'s field types differ from today's hand-written `LogRow` (expected: no change, since the `.mdl` projection mirrors the current struct).

### 3. Generated TypeScript artifacts

New directory `apps/frontend/src/api/generated/logs/`, containing `logs.LogRecord.v1.ts` (copied verbatim from `modelable compile models --target typescript`, same regen-header-comment convention as `tracing.Span.v1.ts`).

In `apps/frontend/src/api/logs.ts`, replace the hand-written `LogRecord` interface with:
```typescript
export type { LogRecord } from "./generated/logs/logs.LogRecord.v1";
```

All other exports in `logs.ts` (`FacetValue`, `Facets`, `LogListResponse`, `LogHistogramBucket`, `LogHistogramResponse`, `searchLogs`, `fetchLogHistogram`, `tailLogs`, `getLogContext`) are unchanged.

### 4. Type-fallout fixes

The generated `LogRecord` will (assuming the `.mdl` above generates as expected):
- Type `timestamp_unix_nano` and `observed_timestamp_unix_nano` as `number` (currently `string`/`string?`).
- Make `observed_timestamp_unix_nano`, `environment`, `host_id`, `attributes`, `resource_attributes` required (currently optional).
- Narrow `fingerprint` to `number | undefined` (currently `number | string | null | undefined`).

Fix the following test fixtures (all currently typed `LogRecord` or assigned to a `LogRecord[]`-typed field):

- `apps/frontend/src/pages/LogSearch.test.tsx` — unquote `timestamp_unix_nano`/`observed_timestamp_unix_nano` numeric literals (2 fixtures, already have all other required fields).
- `apps/frontend/src/features/signals/components/LogResultsTable.test.tsx` — same, unquote 2 timestamp fields (already complete otherwise).
- `apps/frontend/src/components/LogCorrelatedList.render.test.tsx` — `traceLog`/`spanLog`: unquote timestamps, add `observed_timestamp_unix_nano`, `environment: "prod"`, `host_id: "node-1"`, `attributes: {}`, `resource_attributes: {}`.
- `apps/frontend/src/components/LogContextView.test.tsx` — `pivotLog`/`beforeLog`/`traceLinkedLog`: same additions.
- `apps/frontend/src/components/shared/LogList.test.tsx` — `log` const: same additions.
- `apps/frontend/src/components/LogCorrelatedList.test.tsx` — `makeLog()` helper: same additions.
- `apps/frontend/src/components/LogLiveTail.test.tsx` — `makeLog(log_id, timestamp_unix_nano)` helper: change `timestamp_unix_nano` param type from `string` to `number`, update call sites to pass numeric literals, add the same missing fields.
- `apps/frontend/src/hooks/useLiveTail.test.ts` — `makeLog(id, timestampNano)` helper: change `timestampNano` param type from `string` to `number`, update the 5 call sites, add the same missing fields.

`apps/frontend/src/App.test.tsx` constructs similar plain objects but only `JSON.stringify`s them into mock fetch responses (not annotated `LogRecord`) — no change needed.

After these fixes, run `npm run typecheck` to catch any remaining fallout and fix it using the same patterns (numeric timestamps, `{}` for attribute maps, realistic `environment`/`host_id` defaults).

## Verification

- Rust: `cargo fmt --all`, `cargo test -p domain`, `cargo test -p query-api` (covers `services/query-api/src/logs.rs`'s `make_log_row` test fixture, which must still construct a valid `LogRow`/`LogsLogRowV1`).
- modelable: `cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable validate C:\git\Observable\models` passes.
- Lineage proof: `modelable lineage logs.LogRecord@1` and `modelable lineage logs.LogRow@1`, included in the PR description.
- Frontend: `npm run typecheck && npm run lint && npm test && npm run build` from `apps/frontend/`.
- Full: `bash scripts/local-ci.sh`.
- Mark Phase 3 step 3.1 done in `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`.
