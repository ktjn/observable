# Tracing `attributes`/`resourceAttributes` as `map<string, json>` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Model `tracing.Span@1.attributes`/`resourceAttributes` and `tracing.SpanEvent@1.attributes` as `map<string, json>` in `models/tracing.mdl` (instead of `string`), regenerate the four committed Rust artifacts under `libs/domain/src/generated/tracing/`, and confirm zero change to wire format, ClickHouse storage shape, or runtime behavior.

**Architecture:** This is Phase 2 step 2.4 of `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`. Bump the `models/requirements.txt` modelable pin to `0.3.0` (released in this session, adds the `json` type and `@wire(clickhouse: "string")`). Update `models/tracing.mdl` to use `map<string, json>` for the three attribute fields, adding `@wire(clickhouse: "string")` to the corresponding `SpanRow`/`SpanEventRow` projection fields so the generated Row structs keep `String` fields (unchanged ClickHouse shape). Regenerate and copy the four `.rs` files. The `SpanRow`/`SpanEventRow` type aliases in `libs/domain/src/span.rs` are unaffected — only the previously-unused `TracingSpanV1`/`TracingSpanEventV1` canonical types change shape (their `attributes`/`resourceAttributes` become `HashMap<String, serde_json::Value>`, matching the hand-written `Span`/`SpanEvent` structs).

**Tech Stack:** modelable v0.3.0 (Python CLI at `C:\git\modelable\cli`, used only to regenerate committed files), Rust, `serde_json`.

---

## Context for the engineer

- Read `docs/superpowers/specs/2026-06-12-tracing-attributes-json-type-design.md` for the full design rationale if anything here is unclear.
- All file paths are relative to the Observable repo root (`C:\git\Observable`).
- Branch: `feat/tracing-attributes-json-type` (already exists with the design spec committed — continue on it).
- The exact generated Rust output below was produced by running `modelable compile` against a copy of `tracing.mdl` with the changes from Task 2 applied, using the modelable checkout at `C:\git\modelable` (currently on `main`, commit `3e66ca6`, tag `v0.3.0`). You do not need to run `modelable compile` yourself — Task 3 gives you the exact file contents to write — but Task 5 re-runs it as a verification/diff check.

---

### Task 1: Bump modelable dependency pin to 0.3.0

**Files:**
- Modify: `models/requirements.txt`

- [ ] **Step 1: Update the version pin**

In `models/requirements.txt`, change:
```
modelable==0.2.1
```
to:
```
modelable==0.3.0
```

- [ ] **Step 2: Commit**

```bash
git add models/requirements.txt
git commit -m "chore(models): bump modelable pin to 0.3.0

tracing.mdl will use map<string, json> and @wire(clickhouse: \"string\"),
both added in modelable v0.3.0. 0.2.1 cannot validate this file."
```

---

### Task 2: Update `models/tracing.mdl` to use `map<string, json>`

**Files:**
- Modify: `models/tracing.mdl`

- [ ] **Step 1: Replace the file contents**

Replace the entire contents of `models/tracing.mdl` with:

```
// Tracing domain — spans and span events.
//
// Source of truth for SpanRow / SpanEventRow (ClickHouse storage) and SQL DDL.
// The canonical Rust domain structs (Span, SpanEvent) in libs/domain/src/span.rs
// are hand-authored because modelable does not yet generate native Rust enum types
// (SpanKind / StatusCode) or nested types (Span.events: Vec<SpanEvent>). The entities
// below define field names, types, and wire contracts that the hand-authored structs
// must match; they also govern the generated storage-projection artifacts.

domain tracing {
  owner: "platform-team"

  // Canonical span entity. Fields are camelCase in the IDL; the Rust emitter
  // converts to snake_case. attributes / resourceAttributes are map<string, json>,
  // matching the hand-authored domain struct's HashMap<String, serde_json::Value>.
  entity Span @ 1 (additive) {
    @key spanId: string
    traceId: string
    parentSpanId?: string
    tenantId: uuid
    serviceName: string
    serviceNamespace: string
    serviceVersion: string
    operationName: string
    // Serialises as "INTERNAL" | "SERVER" | "CLIENT" | "PRODUCER" | "CONSUMER"
    @wire(json.case: "SCREAMING_SNAKE_CASE")
    spanKind: enum(Internal, Server, Client, Producer, Consumer)
    // Stored as u64 nanoseconds in Rust / UInt64 in ClickHouse
    @wire(rust.type: "u64")
    startTimeUnixNano: int
    @wire(rust.type: "u64")
    endTimeUnixNano: int
    @wire(rust.type: "u64")
    durationNs: int
    // Serialises as "UNSET" | "OK" | "ERROR"
    @wire(json.case: "SCREAMING_SNAKE_CASE")
    statusCode: enum(Unset, Ok, Error)
    statusMessage: string
    attributes: map<string, json>
    resourceAttributes: map<string, json>
    environment: string
    hostId: string
    workload: string
    deploymentId: string
  }

  // ClickHouse storage projection for Span. Maps 1-to-1; spanKind/statusCode are
  // already String in this layer (the From<Span> impl in span.rs performs the
  // enum→string conversion, which modelable cannot generate today). attributes/
  // resourceAttributes use @wire(clickhouse: "string") to generate as String
  // (JSON-encoded), matching the ClickHouse column type.
  projection SpanRow @ 1
    from tracing.Span @ 1 as s
  {
    @wire(clickhouse: "uuid")
    tenantId <- s.tenantId
    traceId <- s.traceId
    spanId <- s.spanId
    parentSpanId <- s.parentSpanId
    serviceName <- s.serviceName
    serviceNamespace <- s.serviceNamespace
    serviceVersion <- s.serviceVersion
    operationName <- s.operationName
    spanKind <- s.spanKind
    startTimeUnixNano <- s.startTimeUnixNano
    endTimeUnixNano <- s.endTimeUnixNano
    durationNs <- s.durationNs
    statusCode <- s.statusCode
    statusMessage <- s.statusMessage
    @wire(clickhouse: "string")
    attributes <- s.attributes
    @wire(clickhouse: "string")
    resourceAttributes <- s.resourceAttributes
    environment <- s.environment
    hostId <- s.hostId
    workload <- s.workload
    deploymentId <- s.deploymentId
  }

  // Canonical span-event entity. event_index is u32 in the hand-authored struct;
  // @wire(rust.type: "u32") propagates the override to generated Rust.
  entity SpanEvent @ 1 (additive) {
    tenantId: uuid
    traceId: string
    @key spanId: string
    // hand-authored Rust field is u32; ClickHouse DDL uses Int64 (UInt32 support
    // requires a future SQL-emitter enhancement for rust.type → UInt32)
    @wire(rust.type: "u32")
    eventIndex: int
    name: string
    @wire(rust.type: "u64")
    timestampUnixNano: int
    attributes: map<string, json>
  }

  // ClickHouse storage projection for SpanEvent.
  projection SpanEventRow @ 1
    from tracing.SpanEvent @ 1 as e
  {
    @wire(clickhouse: "uuid")
    tenantId <- e.tenantId
    traceId <- e.traceId
    spanId <- e.spanId
    eventIndex <- e.eventIndex
    name <- e.name
    timestampUnixNano <- e.timestampUnixNano
    @wire(clickhouse: "string")
    attributes <- e.attributes
  }
}

// ClickHouse adapter — tables match the observable.* schema.
binding ch-observable {
  adapter: clickhouse
}

binding span-binding {
  model: tracing.Span @ 1
  adapter: ch-observable
  table: "spans"
}

binding span-event-binding {
  model: tracing.SpanEvent @ 1
  adapter: ch-observable
  table: "span_events"
}
```

- [ ] **Step 2: Validate**

```bash
cd /c/git/modelable/cli
.venv/Scripts/python.exe -m modelable validate /c/git/Observable/models
```
Expected: `OK ...\tracing.mdl is valid.`

- [ ] **Step 3: Commit**

```bash
git add models/tracing.mdl
git commit -m "feat(models): model tracing attributes/resourceAttributes as map<string, json>

Closes the documented gap where attributes/resourceAttributes were
typed as string (JSON-encoded) because modelable could not express
HashMap<String, serde_json::Value>. Now uses map<string, json>
(modelable v0.3.0), with @wire(clickhouse: \"string\") on the SpanRow/
SpanEventRow projection fields to preserve the String/JSON-encoded
ClickHouse storage shape."
```

---

### Task 3: Regenerate the four committed Rust artifacts

**Files:**
- Modify: `libs/domain/src/generated/tracing/tracing_span_v1.rs`
- Modify: `libs/domain/src/generated/tracing/tracing_span_event_v1.rs`
- Modify: `libs/domain/src/generated/tracing/tracing_span_row_v1.rs`
- Modify: `libs/domain/src/generated/tracing/tracing_span_event_row_v1.rs`

- [ ] **Step 1 (optional verification): Regenerate via modelable**

```bash
cd /c/git/modelable/cli
.venv/Scripts/python.exe -m modelable compile /c/git/Observable/models --target rust --out /tmp/gen-rust
```
Expected: four `OK` lines (`tracing_span_v1.rs`, `tracing_span_event_v1.rs`, `tracing_span_row_v1.rs`, `tracing_span_event_row_v1.rs`), each ending with a hash. The output under `/tmp/gen-rust/tracing/` should match Step 2-5 below (modulo formatting that `cargo fmt` will fix in Step 6).

- [ ] **Step 2: Replace `libs/domain/src/generated/tracing/tracing_span_v1.rs`**

```rust
// @generated by Modelable
// requires: serde_json (https://docs.rs/serde_json)
// requires: uuid (https://docs.rs/uuid)
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TracingSpanV1 {
    pub span_id: String,
    pub trace_id: String,
    pub tenant_id: uuid::Uuid,
    pub service_name: String,
    pub service_namespace: String,
    pub service_version: String,
    pub operation_name: String,
    pub span_kind: String,
    pub start_time_unix_nano: u64,
    pub end_time_unix_nano: u64,
    pub duration_ns: u64,
    pub status_code: String,
    pub status_message: String,
    pub attributes: HashMap<String, serde_json::Value>,
    pub resource_attributes: HashMap<String, serde_json::Value>,
    pub environment: String,
    pub host_id: String,
    pub workload: String,
    pub deployment_id: String,
    pub parent_span_id: Option<String>,
}
```

- [ ] **Step 3: Replace `libs/domain/src/generated/tracing/tracing_span_event_v1.rs`**

```rust
// @generated by Modelable
// requires: serde_json (https://docs.rs/serde_json)
// requires: uuid (https://docs.rs/uuid)
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TracingSpanEventV1 {
    pub tenant_id: uuid::Uuid,
    pub trace_id: String,
    pub span_id: String,
    pub event_index: u32,
    pub name: String,
    pub timestamp_unix_nano: u64,
    pub attributes: HashMap<String, serde_json::Value>,
}
```

- [ ] **Step 4: Replace `libs/domain/src/generated/tracing/tracing_span_row_v1.rs`**

```rust
// @generated by Modelable
// requires: serde_json (https://docs.rs/serde_json)
// requires: uuid (https://docs.rs/uuid)
// requires: clickhouse (https://docs.rs/clickhouse)
use std::collections::HashMap;

#[cfg(feature = "storage")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, clickhouse::Row)]
pub struct TracingSpanRowV1 {
    #[serde(with = "clickhouse::serde::uuid")]
    pub tenant_id: uuid::Uuid,
    pub trace_id: String,
    pub span_id: String,
    pub service_name: String,
    pub service_namespace: String,
    pub service_version: String,
    pub operation_name: String,
    pub span_kind: String,
    pub start_time_unix_nano: u64,
    pub end_time_unix_nano: u64,
    pub duration_ns: u64,
    pub status_code: String,
    pub status_message: String,
    pub attributes: String,
    pub resource_attributes: String,
    pub environment: String,
    pub host_id: String,
    pub workload: String,
    pub deployment_id: String,
    pub parent_span_id: Option<String>,
}

#[cfg(feature = "storage")]
use super::tracing_span_v1::TracingSpanV1;
#[cfg(feature = "storage")]
impl From<TracingSpanV1> for TracingSpanRowV1 {
    fn from(src: TracingSpanV1) -> Self {
        Self {
            tenant_id: src.tenant_id.into(),
            trace_id: src.trace_id.into(),
            span_id: src.span_id.into(),
            parent_span_id: src.parent_span_id.into(),
            service_name: src.service_name.into(),
            service_namespace: src.service_namespace.into(),
            service_version: src.service_version.into(),
            operation_name: src.operation_name.into(),
            span_kind: src.span_kind.into(),
            start_time_unix_nano: src.start_time_unix_nano.into(),
            end_time_unix_nano: src.end_time_unix_nano.into(),
            duration_ns: src.duration_ns.into(),
            status_code: src.status_code.into(),
            status_message: src.status_message.into(),
            attributes: serde_json::to_string(&src.attributes).unwrap_or_default(),
            resource_attributes: serde_json::to_string(&src.resource_attributes).unwrap_or_default(),
            environment: src.environment.into(),
            host_id: src.host_id.into(),
            workload: src.workload.into(),
            deployment_id: src.deployment_id.into(),
        }
    }
}
```

- [ ] **Step 5: Replace `libs/domain/src/generated/tracing/tracing_span_event_row_v1.rs`**

```rust
// @generated by Modelable
// requires: serde_json (https://docs.rs/serde_json)
// requires: uuid (https://docs.rs/uuid)
// requires: clickhouse (https://docs.rs/clickhouse)
use std::collections::HashMap;

#[cfg(feature = "storage")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, clickhouse::Row)]
pub struct TracingSpanEventRowV1 {
    #[serde(with = "clickhouse::serde::uuid")]
    pub tenant_id: uuid::Uuid,
    pub trace_id: String,
    pub span_id: String,
    pub event_index: u32,
    pub name: String,
    pub timestamp_unix_nano: u64,
    pub attributes: String,
}

#[cfg(feature = "storage")]
use super::tracing_span_event_v1::TracingSpanEventV1;
#[cfg(feature = "storage")]
impl From<TracingSpanEventV1> for TracingSpanEventRowV1 {
    fn from(src: TracingSpanEventV1) -> Self {
        Self {
            tenant_id: src.tenant_id.into(),
            trace_id: src.trace_id.into(),
            span_id: src.span_id.into(),
            event_index: src.event_index.into(),
            name: src.name.into(),
            timestamp_unix_nano: src.timestamp_unix_nano.into(),
            attributes: serde_json::to_string(&src.attributes).unwrap_or_default(),
        }
    }
}
```

- [ ] **Step 6: Run cargo fmt**

```bash
cargo fmt --all
```
Expected: reformats the long `resource_attributes: serde_json::to_string(...)` line in `tracing_span_row_v1.rs` (and possibly others) to fit the 100-column limit. No other files should change.

- [ ] **Step 7: Verify it compiles**

```bash
cargo check -p domain --features storage
```
Expected: succeeds with no errors or new warnings (the crate-level `#![allow(dead_code, unused_imports, clippy::useless_conversion)]` in `libs/domain/src/generated/tracing.rs` already covers `TracingSpanV1`/`TracingSpanEventV1` being unused).

- [ ] **Step 8: Commit**

```bash
git add libs/domain/src/generated/tracing/
git commit -m "feat(domain): regenerate tracing artifacts for map<string, json> attributes

TracingSpanV1/TracingSpanEventV1.attributes and resource_attributes are
now HashMap<String, serde_json::Value> (matching the hand-written Span/
SpanEvent structs). TracingSpanRowV1/TracingSpanEventRowV1 keep String
fields via @wire(clickhouse: \"string\"); the generated From impls now
emit serde_json::to_string(...).unwrap_or_default() for these fields
instead of .into(). SpanRow/SpanEventRow (the type aliases used
throughout Observable) are unchanged."
```

---

### Task 4: Full verification

**Files:** none (verification only)

- [ ] **Step 1: Run domain crate tests**

```bash
cargo test -p domain --features storage
```
Expected: all tests pass unchanged (`SpanRow`/`SpanEventRow` struct shape, field names, field order, and derives are identical to before).

- [ ] **Step 2: Run query-api and storage-writer unit tests**

```bash
cargo test -p query-api -p storage-writer --lib
```
Expected: all tests pass, including `select_cols_names_match_span_row_field_count` and `select_cols_field_count_matches_span_row_struct`.

- [ ] **Step 3: Run the integration test suites (require Docker)**

```bash
cargo test -p query-api -p storage-writer -p alert-evaluator --tests
```
Expected: all pass, including `services/query-api/tests/clickhouse_integration.rs` and `services/query-api/tests/http_api_integration.rs` — these insert/select `SpanRow`/`SpanEventRow` rows against a real ClickHouse container, proving the storage shape is unchanged.

- [ ] **Step 4: Run the full local CI script**

```bash
bash scripts/local-ci.sh
```
Expected: all steps pass. The "Modelable validate" step should run (not `SKIP`) if `modelable` is on `PATH`; otherwise it prints `SKIP` — both are fine, but if it runs it must print `OK`.

---

### Task 5: Document completion and lineage proof

**Files:**
- Modify: `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`

- [ ] **Step 1: Generate the lineage proof for the PR description**

```bash
cd /c/git/modelable/cli
.venv/Scripts/python.exe -m modelable lineage tracing.SpanRow@1
.venv/Scripts/python.exe -m modelable lineage tracing.SpanEventRow@1
```
Run both against `/c/git/Observable/models` (check `--help` for the flag to point at a model directory if not picked up from the current directory). Copy the output of both commands verbatim into the PR description under a "Lineage proof" heading — confirms every field in `SpanRow`/`SpanEventRow` (including `attributes`/`resourceAttributes`) traces back to `tracing.Span@1`/`tracing.SpanEvent@1` as `direct`, with no `type_loss` warnings.

- [ ] **Step 2: Mark step 2.4 done in the migration plan**

In `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`, find the line:

```
- [ ] **2.4** Replace `services/query-api/src/traces.rs:32-50` (`TraceResponse`, `FacetValue`, `TraceListResponse`) with generated `reply`-projection types where they map 1:1; keep handler-local aggregation types (`TraceHistogramResponse`, etc.) hand-written if they don't represent canonical domain data — note why in the PR.
```

Replace it with:

```
- [x] **2.4** `TraceResponse` (`{trace_id, spans: Vec<Span>, events: Vec<SpanEvent>}`), `FacetValue`, and `TraceListResponse` in `services/query-api/src/traces.rs` are handler-level aggregation/wrapper types with no 1:1 generated entity or projection equivalent — per the Phase 3 "per-domain rule," they remain hand-written. The concrete 2.4 deliverable was closing the `attributes`/`resourceAttributes` representation gap in `tracing.mdl` (now `map<string, json>`, generating `HashMap<String, serde_json::Value>` for `TracingSpanV1`/`TracingSpanEventV1`), enabled by modelable v0.3.0's `json` type — see `docs/superpowers/specs/2026-06-12-tracing-attributes-json-type-design.md`. Full entity-level generation of `Span`/`SpanEvent` remains blocked on generating Rust enums (`SpanKind`/`StatusCode`) and nested types (`Span.events: Vec<SpanEvent>`), per the step 2.3 scope note.
```

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md
git commit -m "docs: mark Phase 2 step 2.4 (tracing attributes as map<string,json>) done"
```

---

## Out of scope (do not do these)

- Do not touch `Span`, `SpanEvent`, `SpanKind`, `StatusCode`, `SpanRow`, `SpanEventRow`, or any `From` impl in `libs/domain/src/span.rs`.
- Do not modify `services/query-api/src/traces.rs` or `services/storage-writer/src/spans.rs`.
- Do not modify `apps/frontend/src/api/traces.ts` or any TypeScript generation/wiring — that's step 2.5, a separate plan.
- Do not modify `migrations/clickhouse/001_create_spans.sql` or `002_create_span_events.sql`.
- Do not modify `libs/domain/src/generated/tracing.rs` (the module file) — the existing `#![allow(...)]` already covers the new field types.
