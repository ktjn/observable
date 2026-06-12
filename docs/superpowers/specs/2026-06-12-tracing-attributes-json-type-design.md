# Phase 2 Step 2.4: Tracing `attributes`/`resourceAttributes` as `map<string, json>`

## Context

This is Phase 2, step 2.4 of `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`. Step 2.3 (generated `SpanRow`/`SpanEventRow` type aliases) is done (PR #401).

`models/tracing.mdl` currently models `Span.attributes`, `Span.resourceAttributes`, and `SpanEvent.attributes` as `string` (JSON-encoded), with a comment explaining this is a workaround: modelable could not express `HashMap<String, serde_json::Value>` map values. The hand-written domain structs (`libs/domain/src/span.rs`) use `HashMap<String, serde_json::Value>` directly for these fields and hand-roll the `serde_json::to_string`/`from_str` conversions in `From<Span> for SpanRow` / `From<SpanRow> for Span`.

modelable v0.3.0 (just released, github.com/ktjn/modelable PR #46) adds a `json` primitive type, `map<K, json>` support (→ `HashMap<String, serde_json::Value>` in Rust, `Record<string, unknown>`/`unknown` in TypeScript), and a `@wire(clickhouse: "string")` projection-field hint that causes a `map<K, json>` projection field to generate as `String` with an auto-generated `serde_json::to_string(&src.field).unwrap_or_default()` conversion. This closes the exact gap `tracing.mdl` documents.

## Goal

Update `tracing.mdl` to model `attributes`/`resourceAttributes` accurately using `map<string, json>`, regenerate the Rust artifacts, and confirm zero change to wire format, ClickHouse storage shape, or runtime behavior. Document why `TraceResponse`/`FacetValue`/`TraceListResponse` (the files named in the original 2.4 bullet) are not touched.

## Changes

### 1. `models/requirements.txt`

Bump the pin:
```
modelable==0.2.1
```
→
```
modelable==0.3.0
```

### 2. `models/tracing.mdl`

**`entity Span @ 1`:**
- `attributes: string` → `attributes: map<string, json>`
- `resourceAttributes: string` → `resourceAttributes: map<string, json>`
- Remove/update the comment above these fields that says modelable cannot express `serde_json::Value` map values (no longer true).

**`entity SpanEvent @ 1`:**
- `attributes: string` → `attributes: map<string, json>`
- Same comment cleanup.

**`projection SpanRow @ 1`:**
- Add `@wire(clickhouse: "string")` immediately before `attributes <- s.attributes`.
- Add `@wire(clickhouse: "string")` immediately before `resourceAttributes <- s.resourceAttributes`.

**`projection SpanEventRow @ 1`:**
- Add `@wire(clickhouse: "string")` immediately before `attributes <- e.attributes`.

### 3. Regenerate `libs/domain/src/generated/tracing/*.rs`

Run `modelable compile models --target rust --out <tmp>` (using the now-pinned v0.3.0 checkout) and copy the four regenerated files over the committed ones, same as the process used in step 2.3.

Expected diffs:
- **`tracing_span_v1.rs`** (`TracingSpanV1`): `attributes`/`resourceAttributes` fields change from `pub attributes: String` / `pub resource_attributes: String` to `pub attributes: HashMap<String, serde_json::Value>` / `pub resource_attributes: HashMap<String, serde_json::Value>`. File gains a `// requires: serde_json (https://docs.rs/serde_json)` header comment. `use std::collections::HashMap;` becomes a real (non-dead) import.
- **`tracing_span_event_v1.rs`** (`TracingSpanEventV1`): same change for `attributes`.
- **`tracing_span_row_v1.rs`** (`TracingSpanRowV1`): struct fields **unchanged** (`attributes: String`, `resource_attributes: String`). The generated `impl From<TracingSpanV1> for TracingSpanRowV1` changes the `attributes`/`resource_attributes` lines from `src.attributes.into()` to `serde_json::to_string(&src.attributes).unwrap_or_default()`. File gains `// requires: serde_json` header if not already present.
- **`tracing_span_event_row_v1.rs`** (`TracingSpanEventRowV1`): same change for `attributes` in struct (unchanged) and `From` impl (changed).

**No other files change.** `SpanRow`/`SpanEventRow` (the `pub type` aliases in `libs/domain/src/span.rs`) point at `TracingSpanRowV1`/`TracingSpanEventRowV1`, whose struct shape, field names, field order, and `clickhouse::Row`/`serde` derives are unchanged — so `span.rs`, `services/query-api/src/traces.rs`, `services/storage-writer/src/spans.rs`, and all existing tests compile and behave identically. `TracingSpanV1`/`TracingSpanEventV1` remain unused (`#![allow(dead_code)]` in `generated/tracing.rs` already covers this).

### 4. Documentation: scope note for `TraceResponse`/`FacetValue`/`TraceListResponse`

In `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`, mark step 2.4 `[x]` and add a note:

> `TraceResponse` (`{trace_id, spans: Vec<Span>, events: Vec<SpanEvent>}`), `FacetValue`, and `TraceListResponse` in `services/query-api/src/traces.rs` are handler-level aggregation/wrapper types with no 1:1 generated entity or projection equivalent — per the Phase 3 "per-domain rule," they remain hand-written. The concrete 2.4 deliverable is closing the `attributes`/`resourceAttributes` representation gap in `tracing.mdl` (now `map<string, json>`, generating `HashMap<String, serde_json::Value>` for the canonical entities), enabled by modelable v0.3.0's `json` type — this also brings `TracingSpanV1`/`TracingSpanEventV1` a field closer to the hand-written `Span`/`SpanEvent` structs, though full entity-level generation remains blocked on generating Rust enums (`SpanKind`/`StatusCode`) and nested types (`Span.events: Vec<SpanEvent>`), per the step 2.3 scope note.

## Out of scope

- `Span`, `SpanEvent`, `SpanKind`, `StatusCode`, and their hand-written `From` impl bodies in `libs/domain/src/span.rs` — unchanged.
- `services/query-api/src/traces.rs` (`TraceResponse`, `FacetValue`, `TraceListResponse`, `SELECT_COLS`) — unchanged.
- `apps/frontend/src/api/traces.ts` and any TypeScript generation/wiring — that's step 2.5, a separate brainstorm/plan.
- `migrations/clickhouse/001_create_spans.sql` / `002_create_span_events.sql` — unchanged (storage shape is identical).

## Verification

- `modelable compile models --target rust --out <tmp>` then diff against committed files — confirm only the expected lines (above) differ.
- `cargo fmt --all`
- `cargo test -p domain --features storage`
- `cargo test -p query-api -p storage-writer --lib`
- `bash scripts/local-ci.sh`
- `modelable lineage tracing.SpanRow@1` and `modelable lineage tracing.SpanEventRow@1` — paste into PR description, confirm no `type_loss` warnings on `attributes`/`resourceAttributes`.
