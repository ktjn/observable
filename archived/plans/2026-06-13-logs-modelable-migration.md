# Logs Domain Modelable Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `models/logs.mdl` the source of truth for `LogRecord`/`LogRow`, replacing the hand-written `LogRow` struct in `libs/domain/src/log.rs` with a generated type alias and the hand-written `LogRecord` TypeScript interface in `apps/frontend/src/api/logs.ts` with a generated re-export, with zero behavior change.

**Architecture:** Follows the Phase 2 tracing template (`docs/agent-context.md` "Modelable Type-Mapping Migration" section): author `models/logs.mdl`, commit modelable-generated Rust artifacts under `libs/domain/src/generated/logs/`, alias `LogRow` to the generated row type, commit a generated TypeScript artifact under `apps/frontend/src/api/generated/logs/`, re-export `LogRecord` from `apps/frontend/src/api/logs.ts`, then fix the resulting type-tightening fallout (required fields, `number` timestamps) across the frontend.

**Tech Stack:** Rust (`clickhouse` crate v0.15, `serde`, `serde_json`), TypeScript/Vitest, modelable v0.4.0 (Python CLI at `C:\git\modelable\cli\.venv`, used only to regenerate committed files).

---

## Context for the engineer

- This is **Phase 3 step 3.1** of `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`.
- Design doc: `docs/superpowers/specs/2026-06-13-logs-modelable-migration-design.md` — read it if anything here is unclear.
- All file paths are relative to the Observable repo root (`C:\git\Observable`).
- Branch: `feat/logs-modelable-migration` (already exists with the design doc committed — continue on it).
- The `.mdl` content and generated Rust/TypeScript output below have already been verified by running `modelable validate` and `modelable compile` against this exact `.mdl` — both the `@wire(clickhouse: "string")` on a bare `json` field and `@wire(rust.type: "i32")` work as designed, so there are **no open risks/fallbacks** to resolve. Use the content below verbatim.
- `models/requirements.txt` already pins `modelable==0.4.0` — no version bump needed.
- Unlike tracing, `observable.logs` is queried via `SELECT ?fields FROM observable.logs ...` (`services/query-api/src/planner/mod.rs::plan_log_search`), so there is **no `SELECT_COLS` reordering task** — field order in the generated `LogRow` doesn't need to match a hardcoded column list.

---

### Task 1: Author `models/logs.mdl`

**Files:**
- Create: `models/logs.mdl`

- [ ] **Step 1: Create `models/logs.mdl`**

```
domain logs {
  owner: "platform-team"

  // Canonical log-record entity. Mirrors libs/domain/src/log.rs's LogRecord
  // field-for-field. @wire(json.fieldCase: "snake_case") makes the generated
  // TypeScript fields snake_case, matching the real (Rust-serialized) JSON
  // wire format (LogRecord has no #[serde(rename_all = ...)], so serde's
  // default snake_case applies).
  @wire(json.fieldCase: "snake_case")
  entity LogRecord @ 1 (additive) {
    tenantId: uuid
    @key logId: uuid
    // Stored as u64 nanoseconds in Rust / UInt64 in ClickHouse
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

  // ClickHouse storage projection for LogRecord. body/attributes/
  // resourceAttributes use @wire(clickhouse: "string") to generate as String
  // (JSON-encoded), matching the ClickHouse column types. tenantId/logId use
  // @wire(clickhouse: "uuid") for #[serde(with = "clickhouse::serde::uuid")].
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

// ClickHouse adapter — tables match the observable.* schema.
binding ch-observable {
  adapter: clickhouse
}

binding log-binding {
  model: logs.LogRecord @ 1
  adapter: ch-observable
  table: "logs"
}
```

- [ ] **Step 2: Validate**

```bash
cd /c/git/modelable/cli
.venv/Scripts/python.exe -m modelable validate /c/git/Observable/models
```
Expected: `OK ... logs.mdl is valid.` and `OK ... tracing.mdl is valid.` (two lines).

- [ ] **Step 3: Commit**

```bash
git add models/logs.mdl
git commit -m "feat(models): author logs.mdl (LogRecord entity + LogRow projection)

Defines logs.LogRecord@1 and logs.LogRow@1, mirroring
libs/domain/src/log.rs's LogRecord/LogRow field-for-field, per Phase 3
step 3.1 of the modelable type-mapping migration plan."
```

---

### Task 2: Add generated Rust artifacts for the logs domain

**Files:**
- Create: `libs/domain/src/generated/logs.rs`
- Create: `libs/domain/src/generated/logs/logs_log_record_v1.rs`
- Create: `libs/domain/src/generated/logs/logs_log_row_v1.rs`
- Modify: `libs/domain/src/generated.rs`

- [ ] **Step 1 (optional verification): Regenerate via modelable**

```bash
cd /c/git/modelable/cli
.venv/Scripts/python.exe -m modelable compile /c/git/Observable/models --target rust --out /tmp/gen-rust
```
Expected: two `OK` lines for `logs/logs_log_record_v1.rs` and `logs/logs_log_row_v1.rs`, each ending with a hash. Contents should match Steps 2-3 below.

- [ ] **Step 2: Create `libs/domain/src/generated/logs/logs_log_record_v1.rs`**

```rust
// @generated by Modelable
// requires: serde_json (https://docs.rs/serde_json)
// requires: uuid (https://docs.rs/uuid)
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LogsLogRecordV1 {
    pub tenant_id: uuid::Uuid,
    pub log_id: uuid::Uuid,
    pub timestamp_unix_nano: u64,
    pub observed_timestamp_unix_nano: u64,
    pub severity_number: i32,
    pub severity_text: String,
    pub body: serde_json::Value,
    pub attributes: HashMap<String, serde_json::Value>,
    pub resource_attributes: HashMap<String, serde_json::Value>,
    pub service_name: String,
    pub environment: String,
    pub host_id: String,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub fingerprint: Option<u64>,
}
```

- [ ] **Step 3: Create `libs/domain/src/generated/logs/logs_log_row_v1.rs`**

```rust
// @generated by Modelable
// requires: serde_json (https://docs.rs/serde_json)
// requires: uuid (https://docs.rs/uuid)
// requires: clickhouse (https://docs.rs/clickhouse)
use std::collections::HashMap;

#[cfg(feature = "storage")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, clickhouse::Row)]
pub struct LogsLogRowV1 {
    #[serde(with = "clickhouse::serde::uuid")]
    pub tenant_id: uuid::Uuid,
    #[serde(with = "clickhouse::serde::uuid")]
    pub log_id: uuid::Uuid,
    pub timestamp_unix_nano: u64,
    pub observed_timestamp_unix_nano: u64,
    pub severity_number: i32,
    pub severity_text: String,
    pub body: String,
    pub attributes: String,
    pub resource_attributes: String,
    pub service_name: String,
    pub environment: String,
    pub host_id: String,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub fingerprint: Option<u64>,
}

#[cfg(feature = "storage")]
use super::logs_log_record_v1::LogsLogRecordV1;
#[cfg(feature = "storage")]
impl From<LogsLogRecordV1> for LogsLogRowV1 {
    fn from(src: LogsLogRecordV1) -> Self {
        Self {
            tenant_id: src.tenant_id.into(),
            log_id: src.log_id.into(),
            timestamp_unix_nano: src.timestamp_unix_nano.into(),
            observed_timestamp_unix_nano: src.observed_timestamp_unix_nano.into(),
            severity_number: src.severity_number.into(),
            severity_text: src.severity_text.into(),
            body: serde_json::to_string(&src.body).unwrap_or_default(),
            trace_id: src.trace_id.into(),
            span_id: src.span_id.into(),
            attributes: serde_json::to_string(&src.attributes).unwrap_or_default(),
            resource_attributes: serde_json::to_string(&src.resource_attributes).unwrap_or_default(),
            service_name: src.service_name.into(),
            environment: src.environment.into(),
            host_id: src.host_id.into(),
            fingerprint: src.fingerprint.into(),
        }
    }
}
```

- [ ] **Step 4: Create `libs/domain/src/generated/logs.rs`**

This is the module file for `crate::generated::logs`, mirroring `libs/domain/src/generated/tracing.rs`. `LogsLogRecordV1` and its `From` impl are kept (unedited, matching modelable's output) but not re-exported — they're unused by Observable today, same as `TracingSpanV1`. The whole module is marked `allow(dead_code, unused_imports, clippy::useless_conversion)` to silence warnings about the unused type and each generated file's unused `use std::collections::HashMap;`.

```rust
// Generated artifacts for the `logs` domain (models/logs.mdl).
// Regenerate with:
//   modelable compile models --target rust --out <tmp>
// then copy logs.LogRecord.v1.rs / logs.LogRow.v1.rs from <tmp>/logs/ into this
// directory, renaming to snake_case file names. Do not hand-edit the generated
// files themselves.
#![allow(dead_code, unused_imports, clippy::useless_conversion)]

mod logs_log_record_v1;
#[cfg(feature = "storage")]
mod logs_log_row_v1;

#[cfg(feature = "storage")]
pub(crate) use logs_log_row_v1::LogsLogRowV1;
```

- [ ] **Step 5: Wire the new module into `libs/domain/src/generated.rs`**

Current content:
```rust
pub(crate) mod tracing;
```

Replace with:
```rust
pub(crate) mod logs;
pub(crate) mod tracing;
```

- [ ] **Step 6: Verify it compiles**

```bash
cargo check -p domain --features storage
```
Expected: succeeds with no errors. `LogsLogRecordV1`/`LogsLogRowV1` are unused at this point — that's fine, `#![allow(dead_code)]` covers it.

- [ ] **Step 7: Run cargo fmt**

```bash
cargo fmt --all
```

- [ ] **Step 8: Commit**

```bash
git add libs/domain/src/generated.rs libs/domain/src/generated/logs.rs libs/domain/src/generated/logs/
git commit -m "feat(domain): add modelable-generated logs Rust artifacts

Commits the Rust artifacts modelable compile produces for
models/logs.mdl (LogsLogRecordV1/LogsLogRowV1) into
libs/domain/src/generated/logs/, per Phase 3 step 3.1. Not yet wired
up — log.rs still defines its own LogRow."
```

---

### Task 3: Wire `LogRow` as a generated type alias in `libs/domain/src/log.rs`

**Files:**
- Modify: `libs/domain/src/log.rs`

- [ ] **Step 1: Add the import**

`libs/domain/src/log.rs` currently starts:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogRecord {
```

Change to add the generated-type import:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[cfg(feature = "storage")]
use crate::generated::logs::LogsLogRowV1;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogRecord {
```

- [ ] **Step 2: Replace the hand-written `LogRow` struct with a type alias**

Find this block (currently right after `LogRecord` closes):

```rust
#[cfg(feature = "storage")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, clickhouse::Row)]
pub struct LogRow {
    #[serde(with = "clickhouse::serde::uuid")]
    pub tenant_id: Uuid,
    #[serde(with = "clickhouse::serde::uuid")]
    pub log_id: Uuid,
    pub timestamp_unix_nano: u64,
    pub observed_timestamp_unix_nano: u64,
    pub severity_number: i32,
    pub severity_text: String,
    pub body: String,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub attributes: String,
    pub resource_attributes: String,
    pub service_name: String,
    pub environment: String,
    pub host_id: String,
    pub fingerprint: Option<u64>,
}

#[cfg(feature = "storage")]
impl From<LogRecord> for LogRow {
```

Replace the struct definition (everything from the first `#[cfg(feature = "storage")]` through the closing `}` of `pub struct LogRow { ... }`, but NOT the `impl From<LogRecord> for LogRow {` line that follows) with a type alias:

```rust
#[cfg(feature = "storage")]
pub type LogRow = LogsLogRowV1;

#[cfg(feature = "storage")]
impl From<LogRecord> for LogRow {
```

The `impl From<LogRecord> for LogRow` and `impl From<LogRow> for LogRecord` bodies below are unchanged — they construct/destructure `LogRow`/`LogsLogRowV1` by field name (`Self { tenant_id: ..., log_id: ..., ... }`), and `LogsLogRowV1` has the exact same field names (just a different declaration order — `trace_id`/`span_id` are declared after `resource_attributes`/`service_name`/`environment`/`host_id` instead of before `attributes`), so the existing conversion code compiles as-is against the type alias.

- [ ] **Step 3: Run cargo fmt**

```bash
cargo fmt --all
```

- [ ] **Step 4: Run domain crate tests**

```bash
cargo test -p domain --features storage
```
Expected: all tests pass.

- [ ] **Step 5: Run query-api unit tests**

```bash
cargo test -p query-api --lib
```
Expected: all tests pass, including `services/query-api/src/logs.rs`'s `make_log_row` fixture and `log_rows_validate_for_same_tenant` / `make_log_row_with_trace_context` / log-context tests — `make_log_row` constructs `LogRow { tenant_id, log_id, ..., fingerprint: None }` by field name, which compiles unchanged against `LogsLogRowV1`.

- [ ] **Step 6: Commit**

```bash
git add libs/domain/src/log.rs
git commit -m "refactor(domain): LogRow is now a generated type alias

LogRow = LogsLogRowV1 (generated from models/logs.mdl). Field names are
unchanged so the existing From<LogRecord> for LogRow / From<LogRow> for
LogRecord conversions compile unchanged. LogRecord remains hand-written."
```

---

### Task 4: Generate and wire TypeScript `LogRecord`

**Files:**
- Create: `apps/frontend/src/api/generated/logs/logs.LogRecord.v1.ts`
- Modify: `apps/frontend/src/api/logs.ts`

- [ ] **Step 1 (optional verification): Regenerate via modelable**

```bash
cd /c/git/modelable/cli
.venv/Scripts/python.exe -m modelable compile /c/git/Observable/models --target typescript --out /tmp/gen-ts
```
Expected: `OK ... logs.LogRecord.v1.ts ...` and `OK ... logs.LogRow.v1.ts ...`. Only `logs.LogRecord.v1.ts` is committed (no frontend consumer reads `LogRow`/ClickHouse-row shapes, same as tracing's `SpanRow`/`SpanEventRow`).

- [ ] **Step 2: Create `apps/frontend/src/api/generated/logs/logs.LogRecord.v1.ts`**

```typescript
// Generated by modelable — do not edit by hand.
// Regenerate: cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable compile C:\git\Observable\models --target typescript --out <scratch-dir>
// then copy logs.LogRecord.v1.ts into this directory.
/**
 * @modelable domain: logs
 * @modelable name: LogRecord
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface LogsLogRecordV1 {
  tenant_id: string;
  log_id: string;
  timestamp_unix_nano: number;
  observed_timestamp_unix_nano: number;
  severity_number: number;
  severity_text: string;
  body: unknown;
  trace_id?: string;
  span_id?: string;
  attributes: Record<string, unknown>;
  resource_attributes: Record<string, unknown>;
  service_name: string;
  environment: string;
  host_id: string;
  fingerprint?: number;
}
export type LogRecord = LogsLogRecordV1;
```

- [ ] **Step 3: Replace the hand-written `LogRecord` interface in `apps/frontend/src/api/logs.ts`**

Current content (lines 5-21):

```typescript
export interface LogRecord {
  tenant_id: string;
  log_id: string;
  timestamp_unix_nano: string;
  observed_timestamp_unix_nano?: string;
  severity_number: number;
  severity_text: string;
  body: unknown;
  trace_id?: string;
  span_id?: string;
  service_name: string;
  environment?: string;
  host_id?: string;
  fingerprint?: number | string | null;
  attributes?: Record<string, unknown>;
  resource_attributes?: Record<string, unknown>;
}
```

Replace with:

```typescript
export type { LogRecord } from "./generated/logs/logs.LogRecord.v1";
```

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/api/generated/logs/ apps/frontend/src/api/logs.ts
git commit -m "feat(frontend): generate LogRecord from logs.mdl

apps/frontend/src/api/logs.ts now re-exports LogRecord from
apps/frontend/src/api/generated/logs/logs.LogRecord.v1.ts (generated
from models/logs.mdl). This tightens timestamp_unix_nano/
observed_timestamp_unix_nano from string to number (matching
LogRecord's actual u64 serde wire format) and makes
observed_timestamp_unix_nano/environment/host_id/attributes/
resource_attributes required. Fallout fixed in follow-up commits."
```

(This commit is expected to leave `npm run typecheck` failing — fixed by Tasks 5 and 6.)

---

### Task 5: Fix `useLiveTail.ts` cursor-tracking type fallout

**Files:**
- Modify: `apps/frontend/src/hooks/useLiveTail.ts`

**Why:** `useLiveTail`'s polling loop tracks the newest `timestamp_unix_nano` seen so far in a `useRef<string>` cursor (since the `tailLogs` request param `since_unix_nano` is `string`). With `LogRecord.timestamp_unix_nano` now `number`, the `reduce` below returns `number | string`, which isn't assignable to the `string`-typed accumulator/ref.

- [ ] **Step 1: Update the reduce to stringify the numeric timestamp**

Find (around line 58-65):

```typescript
          const newest = res.logs.reduce(
            (max, l) =>
              BigInt(l.timestamp_unix_nano) > BigInt(max)
                ? l.timestamp_unix_nano
                : max,
            cursorRef.current
          );
          cursorRef.current = newest;
```

Replace with:

```typescript
          const newest = res.logs.reduce(
            (max, l) =>
              BigInt(l.timestamp_unix_nano) > BigInt(max)
                ? String(l.timestamp_unix_nano)
                : max,
            cursorRef.current
          );
          cursorRef.current = newest;
```

`BigInt()` accepts both `string` and `number`, so the comparison is unchanged; only the assigned/returned value is now always a `string`, matching `cursorRef: useRef<string>` and `since_unix_nano?: string`.

- [ ] **Step 2: Commit**

```bash
git add apps/frontend/src/hooks/useLiveTail.ts
git commit -m "fix(frontend): stringify numeric log timestamp in live-tail cursor

LogRecord.timestamp_unix_nano is now number (generated from
logs.mdl); useLiveTail's cursorRef/since_unix_nano remain string, so
wrap the candidate in String() before assigning."
```

---

### Task 6: Fix frontend test fixture fallout

**Files:**
- Modify: `apps/frontend/src/pages/LogSearch.test.tsx`
- Modify: `apps/frontend/src/features/signals/components/LogResultsTable.test.tsx`
- Modify: `apps/frontend/src/components/LogCorrelatedList.render.test.tsx`
- Modify: `apps/frontend/src/components/LogContextView.test.tsx`
- Modify: `apps/frontend/src/components/shared/LogList.test.tsx`
- Modify: `apps/frontend/src/components/LogCorrelatedList.test.tsx`
- Modify: `apps/frontend/src/components/LogLiveTail.test.tsx`
- Modify: `apps/frontend/src/hooks/useLiveTail.test.ts`

- [ ] **Step 1: `apps/frontend/src/pages/LogSearch.test.tsx`**

Find (around line 34-35):
```typescript
    timestamp_unix_nano: "1700000000000000000",
    observed_timestamp_unix_nano: "1700000000000000100",
```
Replace with:
```typescript
    timestamp_unix_nano: 1700000000000000000,
    observed_timestamp_unix_nano: 1700000000000000100,
```

Find (around line 51-52):
```typescript
    timestamp_unix_nano: "1700000900000000000",
    observed_timestamp_unix_nano: "1700000900000000100",
```
Replace with:
```typescript
    timestamp_unix_nano: 1700000900000000000,
    observed_timestamp_unix_nano: 1700000900000000100,
```

- [ ] **Step 2: `apps/frontend/src/features/signals/components/LogResultsTable.test.tsx`**

Find (around line 24-25):
```typescript
    timestamp_unix_nano: "1700000000000000000",
    observed_timestamp_unix_nano: "1700000000000000100",
```
Replace with:
```typescript
    timestamp_unix_nano: 1700000000000000000,
    observed_timestamp_unix_nano: 1700000000000000100,
```

- [ ] **Step 3: `apps/frontend/src/components/LogCorrelatedList.render.test.tsx`**

Find (lines 20-41):
```typescript
const traceLog = {
  tenant_id: "t1",
  log_id: "trace-log-1",
  timestamp_unix_nano: "1000000000",
  severity_number: 5,
  severity_text: "INFO",
  body: "trace level message",
  trace_id: "trace-abc",
  service_name: "checkout",
};

const spanLog = {
  tenant_id: "t1",
  log_id: "span-log-1",
  timestamp_unix_nano: "2000000000",
  severity_number: 9,
  severity_text: "WARN",
  body: "span level message",
  trace_id: "trace-abc",
  span_id: "span-111",
  service_name: "checkout",
};
```

Replace with:
```typescript
const traceLog = {
  tenant_id: "t1",
  log_id: "trace-log-1",
  timestamp_unix_nano: 1000000000,
  observed_timestamp_unix_nano: 1000000000,
  severity_number: 5,
  severity_text: "INFO",
  body: "trace level message",
  trace_id: "trace-abc",
  attributes: {},
  resource_attributes: {},
  service_name: "checkout",
  environment: "prod",
  host_id: "node-1",
};

const spanLog = {
  tenant_id: "t1",
  log_id: "span-log-1",
  timestamp_unix_nano: 2000000000,
  observed_timestamp_unix_nano: 2000000000,
  severity_number: 9,
  severity_text: "WARN",
  body: "span level message",
  trace_id: "trace-abc",
  span_id: "span-111",
  attributes: {},
  resource_attributes: {},
  service_name: "checkout",
  environment: "prod",
  host_id: "node-1",
};
```

- [ ] **Step 4: `apps/frontend/src/components/LogContextView.test.tsx`**

Find (lines 20-50):
```typescript
const pivotLog = {
  tenant_id: "t1",
  log_id: "pivot-id",
  timestamp_unix_nano: "1000000000",
  severity_number: 9,
  severity_text: "WARN",
  body: "pivot message",
  service_name: "checkout",
};

const beforeLog = {
  tenant_id: "t1",
  log_id: "before-id",
  timestamp_unix_nano: "500000000",
  severity_number: 5,
  severity_text: "INFO",
  body: "before message",
  service_name: "checkout",
};

const traceLinkedLog = {
  tenant_id: "t1",
  log_id: "trace-linked-id",
  timestamp_unix_nano: "1500000000",
  severity_number: 5,
  severity_text: "INFO",
  body: "trace linked message",
  trace_id: "trace-abc",
  span_id: "span-xyz",
  service_name: "checkout",
};
```

Replace with:
```typescript
const pivotLog = {
  tenant_id: "t1",
  log_id: "pivot-id",
  timestamp_unix_nano: 1000000000,
  observed_timestamp_unix_nano: 1000000000,
  severity_number: 9,
  severity_text: "WARN",
  body: "pivot message",
  attributes: {},
  resource_attributes: {},
  service_name: "checkout",
  environment: "prod",
  host_id: "node-1",
};

const beforeLog = {
  tenant_id: "t1",
  log_id: "before-id",
  timestamp_unix_nano: 500000000,
  observed_timestamp_unix_nano: 500000000,
  severity_number: 5,
  severity_text: "INFO",
  body: "before message",
  attributes: {},
  resource_attributes: {},
  service_name: "checkout",
  environment: "prod",
  host_id: "node-1",
};

const traceLinkedLog = {
  tenant_id: "t1",
  log_id: "trace-linked-id",
  timestamp_unix_nano: 1500000000,
  observed_timestamp_unix_nano: 1500000000,
  severity_number: 5,
  severity_text: "INFO",
  body: "trace linked message",
  trace_id: "trace-abc",
  span_id: "span-xyz",
  attributes: {},
  resource_attributes: {},
  service_name: "checkout",
  environment: "prod",
  host_id: "node-1",
};
```

- [ ] **Step 5: `apps/frontend/src/components/shared/LogList.test.tsx`**

Find (lines 6-15):
```typescript
const log: LogRecord = {
  tenant_id: "t1",
  log_id: "log-1",
  timestamp_unix_nano: "1700000000000000000",
  severity_number: 9,
  severity_text: "INFO",
  body: "checkout completed",
  trace_id: "trace-abc",
  service_name: "svc",
};
```

Replace with:
```typescript
const log: LogRecord = {
  tenant_id: "t1",
  log_id: "log-1",
  timestamp_unix_nano: 1700000000000000000,
  observed_timestamp_unix_nano: 1700000000000000000,
  severity_number: 9,
  severity_text: "INFO",
  body: "checkout completed",
  trace_id: "trace-abc",
  attributes: {},
  resource_attributes: {},
  service_name: "svc",
  environment: "prod",
  host_id: "node-1",
};
```

- [ ] **Step 6: `apps/frontend/src/components/LogCorrelatedList.test.tsx`**

Find (lines 5-17):
```typescript
function makeLog(log_id: string, span_id?: string): LogRecord {
  return {
    tenant_id: "00000000-0000-0000-0000-000000000001",
    log_id,
    timestamp_unix_nano: "100",
    severity_number: 9,
    severity_text: "WARN",
    body: "message",
    trace_id: "trace-1",
    span_id,
    service_name: "checkout",
  };
}
```

Replace with:
```typescript
function makeLog(log_id: string, span_id?: string): LogRecord {
  return {
    tenant_id: "00000000-0000-0000-0000-000000000001",
    log_id,
    timestamp_unix_nano: 100,
    observed_timestamp_unix_nano: 100,
    severity_number: 9,
    severity_text: "WARN",
    body: "message",
    trace_id: "trace-1",
    span_id,
    attributes: {},
    resource_attributes: {},
    service_name: "checkout",
    environment: "prod",
    host_id: "node-1",
  };
}
```

- [ ] **Step 7: `apps/frontend/src/components/LogLiveTail.test.tsx`**

Find (the whole file):
```typescript
import { expect, test } from "vitest";
import type { LogRecord } from "../api/logs";
import { mergeLogs } from "./LogLiveTail";

function makeLog(log_id: string, timestamp_unix_nano: string): LogRecord {
  return {
    tenant_id: "00000000-0000-0000-0000-000000000001",
    log_id,
    timestamp_unix_nano,
    severity_number: 9,
    severity_text: "WARN",
    body: "message",
    service_name: "checkout",
  };
}

test("mergeLogs deduplicates and preserves ascending timestamp order", () => {
  const current = [makeLog("b", "200")];
  const incoming = [makeLog("a", "100"), makeLog("b", "200"), makeLog("c", "300")];

  const merged = mergeLogs(current, incoming);

  expect(merged.map((log) => log.log_id)).toEqual(["a", "b", "c"]);
});
```

Replace with:
```typescript
import { expect, test } from "vitest";
import type { LogRecord } from "../api/logs";
import { mergeLogs } from "./LogLiveTail";

function makeLog(log_id: string, timestamp_unix_nano: number): LogRecord {
  return {
    tenant_id: "00000000-0000-0000-0000-000000000001",
    log_id,
    timestamp_unix_nano,
    observed_timestamp_unix_nano: timestamp_unix_nano,
    severity_number: 9,
    severity_text: "WARN",
    body: "message",
    attributes: {},
    resource_attributes: {},
    service_name: "checkout",
    environment: "prod",
    host_id: "node-1",
  };
}

test("mergeLogs deduplicates and preserves ascending timestamp order", () => {
  const current = [makeLog("b", 200)];
  const incoming = [makeLog("a", 100), makeLog("b", 200), makeLog("c", 300)];

  const merged = mergeLogs(current, incoming);

  expect(merged.map((log) => log.log_id)).toEqual(["a", "b", "c"]);
});
```

- [ ] **Step 8: `apps/frontend/src/hooks/useLiveTail.test.ts`**

Find (lines 25-35):
```typescript
function makeLog(id: string, timestampNano: string): LogRecord {
  return {
    tenant_id: "t1",
    log_id: id,
    timestamp_unix_nano: timestampNano,
    severity_number: 9,
    severity_text: "INFO",
    body: {},
    service_name: "svc",
  };
}
```

Replace with:
```typescript
function makeLog(id: string, timestampNano: number): LogRecord {
  return {
    tenant_id: "t1",
    log_id: id,
    timestamp_unix_nano: timestampNano,
    observed_timestamp_unix_nano: timestampNano,
    severity_number: 9,
    severity_text: "INFO",
    body: {},
    attributes: {},
    resource_attributes: {},
    service_name: "svc",
    environment: "prod",
    host_id: "node-1",
  };
}
```

Find (line 62):
```typescript
      logs: [makeLog("1", "1000"), makeLog("2", "2000")],
```
Replace with:
```typescript
      logs: [makeLog("1", 1000), makeLog("2", 2000)],
```

Find (lines 76-79):
```typescript
    const ts1 = String(Date.now() * 1_000_000 + 1_000_000);
    const ts2 = String(Date.now() * 1_000_000 + 5_000_000);
    vi.spyOn(logsApi, "tailLogs").mockResolvedValue({
      logs: [makeLog("1", ts1), makeLog("2", ts2)],
```
Replace with:
```typescript
    const ts1 = Date.now() * 1_000_000 + 1_000_000;
    const ts2 = Date.now() * 1_000_000 + 5_000_000;
    vi.spyOn(logsApi, "tailLogs").mockResolvedValue({
      logs: [makeLog("1", ts1), makeLog("2", ts2)],
```

Find (line 90):
```typescript
    expect(calls[1][1]).toMatchObject({ since_unix_nano: ts2 });
```
Replace with:
```typescript
    expect(calls[1][1]).toMatchObject({ since_unix_nano: String(ts2) });
```

Find (lines 94-97):
```typescript
    const make300 = (offset: number) =>
      Array.from({ length: 300 }, (_, i) =>
        makeLog(String(offset + i), String(offset + i + 1))
      );
```
Replace with:
```typescript
    const make300 = (offset: number) =>
      Array.from({ length: 300 }, (_, i) =>
        makeLog(String(offset + i), offset + i + 1)
      );
```

Find (line 116):
```typescript
      logs: [makeLog("1", "1000")],
```
Replace with:
```typescript
      logs: [makeLog("1", 1000)],
```

- [ ] **Step 9: Commit**

```bash
git add apps/frontend/src/pages/LogSearch.test.tsx \
  apps/frontend/src/features/signals/components/LogResultsTable.test.tsx \
  apps/frontend/src/components/LogCorrelatedList.render.test.tsx \
  apps/frontend/src/components/LogContextView.test.tsx \
  apps/frontend/src/components/shared/LogList.test.tsx \
  apps/frontend/src/components/LogCorrelatedList.test.tsx \
  apps/frontend/src/components/LogLiveTail.test.tsx \
  apps/frontend/src/hooks/useLiveTail.test.ts
git commit -m "test(frontend): fix LogRecord fixtures for generated type

timestamp_unix_nano/observed_timestamp_unix_nano are now numbers, and
observed_timestamp_unix_nano/environment/host_id/attributes/
resource_attributes are required, per the generated LogRecord type
from logs.mdl."
```

---

### Task 7: Full verification, lineage proof, mark 3.1 done

**Files:**
- Modify: `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`

- [ ] **Step 1: Frontend typecheck/lint/test/build**

```bash
cd apps/frontend
npm run typecheck && npm run lint && npm test && npm run build
```
Expected: all pass. If `npm run typecheck` surfaces any fallout beyond Task 5/6's fixes, fix it using the same patterns (numeric timestamps, `{}` for attribute maps, `"prod"`/`"node-1"` or similarly realistic defaults for `environment`/`host_id`) before continuing.

- [ ] **Step 2: Rust tests**

```bash
cd /c/git/Observable
cargo fmt --all
cargo test -p domain -p query-api -p storage-writer --features storage
```
Expected: all pass.

- [ ] **Step 3: Full local CI**

```bash
bash scripts/local-ci.sh
```
Expected: all steps pass (frontend typecheck/lint/build/test, Rust fmt/clippy/unit/integration tests, Docker image build, smoke test).

- [ ] **Step 4: Generate the lineage proof for the PR description**

```bash
cd /c/git/modelable/cli
.venv/Scripts/python.exe -m modelable lineage logs.LogRecord@1
.venv/Scripts/python.exe -m modelable lineage logs.LogRow@1
```
Run both against `/c/git/Observable/models`. Copy the output of both commands verbatim into the PR description under a "Lineage proof" heading — evidence that every field in `LogRow`/`LogRecord` traces back to `logs.LogRecord@1` with no `type_loss` warnings.

- [ ] **Step 5: Mark step 3.1 done in the migration plan**

In `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`, change:
```
- [ ] **3.1 Logs** — `libs/domain/src/log.rs:6-70` (`LogRecord`/`LogRow`), `services/query-api/src/logs.rs:16-41`, `apps/frontend/src/api/logs.ts:5-36`
```
to:
```
- [x] **3.1 Logs** — Generated `logs.LogRecord@1`/`logs.LogRow@1` from `models/logs.mdl` (see `docs/superpowers/specs/2026-06-13-logs-modelable-migration-design.md`). `LogRow` in `libs/domain/src/log.rs` is now `pub type LogRow = LogsLogRowV1` (generated, `libs/domain/src/generated/logs/`); `LogRecord` remains hand-written. `apps/frontend/src/api/logs.ts`'s `LogRecord` is now a re-export of `apps/frontend/src/api/generated/logs/logs.LogRecord.v1.ts`. `services/query-api/src/logs.rs`'s handler types (`FacetValue`, `LogListResponse`, `LogHistogramBucket`, `LogHistogramResponse`) remain hand-written — handler-level aggregation/wrapper types, same rationale as 2.4/2.5.
```

- [ ] **Step 6: Commit**

```bash
git add docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md
git commit -m "docs: mark Phase 3 step 3.1 (Logs domain) done"
```

---

## Out of scope (do not do these)

- Do not touch `services/query-api/src/logs.rs` handler logic, `FacetValue`, `LogListResponse`, `LogHistogramBucket`, `LogHistogramResponse`, or the `searchLogs`/`fetchLogHistogram`/`tailLogs`/`getLogContext` functions in `apps/frontend/src/api/logs.ts` — these are handler-level aggregation/wrapper types and stay hand-written.
- Do not modify `migrations/clickhouse/002_create_logs.sql` — the generated SQL DDL is not adopted in this step.
- Do not implement ADR-030's string-encoded-u64 convention for `LogRecord` — the generated TypeScript mirrors the real (non-compliant) wire format, matching the tracing precedent.
- Do not change `apps/frontend/src/App.test.tsx` — its log-shaped objects are only `JSON.stringify`'d into mock fetch responses, not type-checked against `LogRecord`.
