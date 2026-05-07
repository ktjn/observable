# NLQ IR Reference

This document is the canonical reference for Observable's Natural Language Query (NLQ)
Intermediate Representation (IR): the DSL grammar, all semantic rules, the system prompt
architecture, and the metadata injection contract.

**Ground-truth sources:** [`libs/domain/src/nlq.rs`](../libs/domain/src/nlq.rs),
[`services/query-api/src/llm_adapter.rs`](../services/query-api/src/llm_adapter.rs),
[`services/query-api/src/sql_templates.rs`](../services/query-api/src/sql_templates.rs)

**Related ADRs:** [ADR-021](adr/ADR-021-nl-query-layer.md) (NL query layer),
[ADR-026](adr/ADR-026-no-proprietary-query-dsl.md) (no proprietary DSL),
[ADR-027](adr/ADR-027-local-llm-backend.md) (LLM backend config),
[ADR-029](adr/ADR-029-simple-ir-shorthand.md) (shorthand / deterministic fallback),
[ADR-030](adr/ADR-030-timestamp-representation.md) (timestamp format)

---

## §1  Introduction

### §1.1  Pipeline Overview

```
NLQ (user)         → LLM → NlqIr → MCP server → SQL → VisualizationFrame → UI
Raw NlqIr JSON ────┘
Simple IR Shorthand (/) → NlqIr (deterministic, no LLM)
```

Observable exposes a natural language query interface backed by a three-stage pipeline:

1. **Stage 1 — LLM → NlqIr.** The backend mediates all LLM interaction: it fetches tenant
   metadata, builds the system prompt, calls the LLM, validates the response, and optionally
   retries with a targeted repair prompt before proceeding.

2. **Stage 2 — MCP server (NlqIr → SQL).** The MCP server translates the validated IR into
   tenant-scoped ClickHouse SQL. All time-series semantics live here — not in the prompt.

3. **Stage 3 — VisualizationFrame → UI.** The MCP server returns a typed, self-describing
   `VisualizationFrame` that the UI auto-renders without guessing the panel type.

### §1.2  Design Invariants

- The LLM emits **IR, never SQL.** SQL generation is deterministic and lives in the MCP server.
- The IR is **stable and independently versioned** from the LLM and SQL dialect.
- Every response is **advisory only.** Results are approximate and must never be used for
  billing, SLA enforcement, contractual compliance, or regulatory reporting.
- Every response carries a **provenance payload:** the NlqIr that was executed, the raw SQL,
  the signals consulted, the effective sample rate per signal, and an approximation statement.
- All generated SQL is **SELECT-only.** The handler generates no mutations.
- Every query executes under the caller's tenant context. The LLM and MCP server cannot
  bypass tenant isolation or RBAC.

---

## §2  NlqIr Schema (the DSL Grammar)

The Rust struct `NlqIr` in `libs/domain/src/nlq.rs` is the authoritative schema.

```json
{
  "operation":          "<NlqOperation>",
  "signals":            ["<NlqSignal>", ...],
  "metric":             "<string>" | null,
  "query":              "<string>" | null,
  "window":             "<duration>" | null,
  "filters":            [{ "field": "<string>", "op": "<NlqFilterOp>", "value": "<string>" }],
  "group_by":           ["<string>"],
  "resolution":         "<duration>" | null,
  "time_range":         { "from": "<time_expr>", "to": "<time_expr>" },
  "visualization_hint": "<NlqVisualizationHint>" | null,
  "percentiles":        ["<stat_name>"] | null,
  "catalog_field":      "<string>" | null,
  "limit":              <integer> | null
}
```

### §2.1  `operation` — NlqOperation

| Value | Use case | Metric required | Output shape |
|---|---|---|---|
| `timeseries` | Trend chart of a gauge over time | Yes | Many rows (bucket, value) |
| `rate` | Per-second rate of a monotonic counter, reset-aware | Yes, counter type | Many rows (bucket, rate) |
| `irate` | Instantaneous rate from the two most recent samples | Yes, counter type | Many rows (bucket, rate) |
| `increase` | Total counter increase over the window | Yes, counter type | One or many rows |
| `histogram` | OTel Histogram bucket expansion | Yes, histogram type | Many rows (bound, count) |
| `topk` | Rank entities by average metric value | Yes | N rows (label, value) |
| `table` | Raw point scan, most recent rows | Optional | Up to 1000 rows |
| `distribution` | Scalar stats for a single time window | Yes | One row (p95, avg, …) |
| `catalog` | Enumerate distinct values of a dimension | No | Many rows (value, count) |
| `inventory` | Filter infrastructure entity table by attribute predicates | No | Many rows (entity summary) |

### §2.2  `signals` — NlqSignal

Controls which storage backend the MCP server queries.

| Value | Backend |
|---|---|
| `"metrics"` | `observable.metric_points` + `observable.metric_series` |
| `"logs"` | `observable.logs` |
| `"traces"` | `observable.spans` |

Serialized as lowercase. The array almost always contains exactly one element.
Normalisation rules: unknown values are replaced with `"metrics"`; an empty or missing
array defaults to `["metrics"]`.

### §2.3  `metric`

The metric name as stored in `observable.metric_series.metric_name`. Must exactly match a
known metric from the Schema Registry. Null is valid only for log queries, trace queries,
`catalog`, and `inventory` operations.

### §2.4  `query`

Free-text body search term for log and trace queries. Applied as a case-insensitive substring
match on the log `body` column (or trace `operation` field). Null for all metric operations.

### §2.5  `window`

Lookback window for range-vector operations (`rate`, `irate`, `increase`). Duration string:
`"5m"`, `"1h"`, `"30s"`, etc. Not used by `timeseries`, `distribution`, `topk`, or `table`.

### §2.6  `filters`

Array of field-level predicates. Each filter has three fields:

- **`field`** — column or attribute name (e.g. `"service_name"`, `"environment"`, `"pod"`)
- **`op`** — one of the `NlqFilterOp` values below
- **`value`** — comparison value as a string; the MCP server casts appropriately

#### NlqFilterOp values

| Op | Meaning |
|---|---|
| `"="` | Exact equality |
| `"!="` | Inequality |
| `"=~"` | Regex match |
| `"!~"` | Regex non-match |
| `">"` | Greater than |
| `">="` | Greater than or equal |
| `"<"` | Less than |
| `"<="` | Less than or equal |

Filters with unknown or invalid `op` values (e.g. `"range"`) are stripped during IR
normalisation and never reach SQL generation.

### §2.7  `group_by`

Array of label column names to group by (e.g. `["pod", "region"]`). Used by `timeseries`,
`rate`, `irate`, `increase`, and `topk`. Ignored for `distribution` and `catalog`.

### §2.8  `resolution`

Time-bucket width for downsampling: `"1m"`, `"5m"`, `"1h"`. Controls the `time_bucket()`
granularity in `timeseries`, `rate`, `irate`, and `increase` SQL patterns.

### §2.9  `time_range`

Query time range with two sub-fields:

- **`from`** — start of the range
- **`to`** — end of the range (usually `"now"`)

Two formats are accepted (see [ADR-030](adr/ADR-030-timestamp-representation.md)):

| Format | Example | Notes |
|---|---|---|
| Relative expression | `"now-1h"`, `"now-30m"`, `"now-7d"`, `"now-30s"` | LLM preferred format |
| Unix nanoseconds | `"1746274719123000000"` | Frontend converts from ms: `String(BigInt(Math.floor(ms)) * 1_000_000n)` |

ISO-8601 strings (e.g. `"2026-05-01T12:00:00Z"`) are **explicitly rejected**. Callers must
convert to Unix nanoseconds before including in the IR.

Null `from` or `to` values are patched with `"now-24h"` and `"now"` respectively during
IR normalisation.

### §2.10  `visualization_hint` — NlqVisualizationHint

Preferred panel type for the UI's auto-graphing layer. Maps to Grafana panel types
(see [ADR-016](adr/ADR-016-grafana-visualization-strategy.md)).

| Value | Grafana panel |
|---|---|
| `"timeseries"` | Time series |
| `"histogram"` | Histogram |
| `"heatmap"` | Heatmap |
| `"table"` | Table |
| `"topk"` | Bar gauge / stat |
| `"flamegraph"` | Flamegraph |
| `"distribution"` | Stat / bar |

### §2.11  `percentiles`

For `distribution` operations only: the exact stats the user requested, in order.
When absent or empty the SQL template defaults to `["p50", "p90", "p95", "p99", "min", "max"]`.
See §9.2 for the complete stat name → SQL expression mapping.

### §2.12  `catalog_field`

For `catalog` operations: the dimension to enumerate. First-class column names (`service_name`,
`environment`, `metric_name`) map directly to `ms.<col>`; all other values fall back to
`JSONExtractString(ms.attributes, '<field>')`.

| User question | `catalog_field` |
|---|---|
| "list all services" | `"service_name"` |
| "what environments exist?" | `"environment"` |
| "what metrics does X emit?" | `"metric_name"` |
| "list pods for X" | `"pod"` |
| "list all regions" | `"region"` |

### §2.13  `limit`

For `topk` operations only: how many top results to return. Defaults to 10 when absent.
Must be `null` for all other operations.

---

## §3  Semantic Rules

### §3.1  Operation Disambiguation

**`timeseries` vs `distribution`**

The primary disambiguation: does the user want a chart or a single number?

- **Rule:** if the user names a specific stat (`p50`, `p75`, `p90`, `p95`, `p99`, `p999`,
  `average`, `mean`, `median`, `min`, `max`) → `distribution`
- **Rule:** if the user asks for a metric by name with a time range but no specific stat →
  `timeseries`

| User phrasing | Operation |
|---|---|
| "p95 latency for the last hour" | `distribution` |
| "average latency" / "mean request duration" | `distribution` |
| "request latency over the last hour" | `timeseries` |
| "latency over time" / "show me a graph of latency" | `timeseries` |
| "how has latency changed" / "request duration" | `timeseries` |

**`histogram` vs `distribution`**

Both operate on latency data but from different OTel instruments:

- `histogram` — OTel Histogram bucket columns (`histogram_explicit_bounds`,
  `histogram_bucket_counts`). Use ONLY when `metric_type = 'histogram'` AND the user
  explicitly says "histogram", "bucket distribution", or "buckets".
- `distribution` — scalar `value_double` / `value_int` via `quantile()`. Valid for gauge,
  counter, and summary metrics.

**`topk` vs `catalog`**

- `catalog` = "list what exists" — no aggregation, no ranking by value
- `topk` = "which entities have the highest/lowest metric value" — requires metric aggregation

| User phrasing | Operation |
|---|---|
| "top 5 services by latency" | `topk` |
| "which 3 services have the most errors?" | `topk` |
| "list all services" | `catalog` |
| "what metrics does checkout emit?" | `catalog` |

**`table` operation**

Escape hatch for raw data inspection. Always set `metric` when scoping to a specific series.
Without `metric`, the scan covers all series for the tenant.

**`inventory` operation**

Use only when the user is on an entity-table page and is filtering by entity attributes.
Valid filter fields: `entity_type` (host/cluster/namespace/pod/container), `environment`,
`service_name`, `display_name` (free-text search). Do not set `metric` or `catalog_field`.

### §3.2  Signal Routing Rules

- Log queries: always `signals: ["logs"]`, `operation: "table"`, `metric: null`
- Trace queries: always `signals: ["traces"]`, `operation: "table"`, `metric: null`
- Metric queries: `signals: ["metrics"]`
- `catalog` and `inventory` operations: `signals: ["metrics"]` or `signals: []`
- Normalisation: unknown signal values → `"metrics"`; empty/missing array → `["metrics"]`

### §3.3  Filter Rules

- **Never** add a filter with an empty-string value. If the value is unknown, omit the filter.
- **Never** put time constraints in the `filters` array. All temporal bounds go in `time_range`.
- Only add filters for values explicitly stated in the user's question.
- Valid `op` values: `=`, `!=`, `=~`, `!~`, `>`, `>=`, `<`, `<=`.
  The op `"range"` does not exist and will be stripped during normalisation.
- Filters with unknown ops are silently removed before SQL generation.

### §3.4  Time Range Rules

- Use relative expressions for LLM-generated IRs: `"now-1h"`, `"now-30m"`, etc.
- Frontend pages convert the global date picker to Unix nanosecond integer strings
  before sending to the backend.
- ISO-8601 is explicitly rejected; the backend returns a 400 for such values.
- Missing or null `from`/`to` are patched to `"now-24h"` / `"now"` during normalisation.

### §3.5  Advisory Boundary

**Decline required** for questions that explicitly involve:

- Billing, invoicing, or financial reconciliation
- SLA evidence, contractual compliance, or service level objectives used as contracts
- Regulatory compliance: GDPR, HIPAA, SOX, audit trails
- Any use case requiring BI-grade correctness guarantees

**Never decline** operational or observability questions. The deny gate is belt-and-suspenders:
the LLM prompt instructs the model to decline, and the server-side gate in `llm_adapter.rs`
(`server_side_deny_gate`) enforces it independently — prompt-only enforcement is insufficient
(see [ADR-014](adr/ADR-014-ai-feature-boundaries.md)).

Server-side denied keywords (case-insensitive substring match): `"billing"`, `"invoice"`,
`"sla"`, `"contractual"`, `"regulatory"`, `"compliance report"`, `"audit trail"`,
`"gdpr"`, `"hipaa"`, `"sox"`.

### §3.6  Percentiles Rules

- Set `percentiles` to exactly what the user asked for — no extras, no omissions.
- Valid stat names: `"p{N}"` (N = 1–999), `"median"`, `"average"`, `"mean"`, `"min"`, `"max"`
- `"limit"` is ONLY for `topk`. For all other operations, `limit` must be `null`.

---

## §4  System Prompt Architecture

The system prompt is assembled fresh for every NLQ request from two layers in
`build_system_prompt()` (`services/query-api/src/llm_adapter.rs`).

### §4.1  Static Instruction Layer

Constant across all requests. Contains:

1. **Role definition** — "You are an observability query assistant…"
2. **Output format** — the three valid JSON response shapes (`ir`, `decline`, `capabilities`)
3. **NlqIr schema block** — the full field list with allowed values (verbatim from §2 above)
4. **Operation guide** — all 10 operations with a one-line description and when to use each
5. **Operation examples** — complete IR examples for `table`, `catalog`, `inventory`, and
   log search, with correct and incorrect phrasings
6. **topk vs catalog disambiguation block** — explicit counter-examples
7. **timeseries vs distribution disambiguation block** — rules with examples
8. **Log search section** — `signals: ["logs"]` semantics, valid filter fields, examples
9. **Filter rules (CRITICAL)** — no empty values, no time constraints in filters, valid ops
10. **`percentiles` field rules** — valid stat names, must-include-all, must-not-add-extras
11. **`limit` field rules** — topk only
12. **topk usage section** — ranking by value, must set `metric`, default limit=10
13. **Advisory boundary** — explicit decline list + "never decline" list with examples

### §4.2  Dynamic Metadata Layer

Assembled per-tenant, per-request.

#### Available metrics

Fetched via `fetch_schema_context(db, ch, tenant_id, limit=20)`:

- Queries `list_signal_fields(db, tenant_id, "metrics")` from the PostgreSQL Schema Registry
- Filters to `schema_complete = true` (both `metric_type` and `timestamp_column` annotated)
- Sorted by annotation richness descending: metrics with `business_description` + `display_name`
  appear first (token budget awareness)
- Truncated to 20 entries

Each entry is rendered as:

```
- **{field_name}** ({display_name}) [type: {metric_type}] [unit: {unit}]: {business_description}. Interpretation: {interpretation_rule}. [sampled at {sample_rate}%]
```

Fields that are `null` are omitted from the rendered line.
When no annotated metrics exist: `(no annotated metrics available for this tenant)`

#### Available label keys

Fetched via `fetch_label_keys(ch, tenant_id, limit=20)` from ClickHouse `metric_series`:

- Returns the top-N distinct attribute key names present in the series catalog for the tenant
- Rendered as a comma-separated list
- Instruction: "Use these keys in filters and group_by. Do not invent label keys not listed."
- Exception: for `catalog` operations the user may use any label name as `catalog_field` even
  if not listed; the SQL will return empty results if the field does not exist.

When empty: `(no label keys discovered for this tenant)`

Both fetches are **non-fatal**: on error a warning is logged and an empty list is returned so
the pipeline continues unimpeded.

#### Service scope section

Injected when `service_name` is set in the request:

```
## Service scope

This query is scoped to service `{service_name}`.
You do not need to add a service_name filter — it is enforced automatically.
```

Service scope enforcement is also applied **server-side** (`enforce_service_scope()`)
after the LLM returns its IR — independent of whether the model obeyed the prompt instruction.

#### Page context section

Derived from `base_ir.operation` + `base_ir.signals` (no separate `surface_hint` string).
Injected when `base_ir` is present in the request:

| `base_ir` condition | Context injected |
|---|---|
| `base_ir.operation == "inventory"` | Infrastructure inventory page — always use `operation: "inventory"` |
| `base_ir.signals` contains `"logs"` | Log search page — always use `operation: "table"`, `signals: ["logs"]` |
| `base_ir.signals` contains `"traces"` | Trace search page — always use `operation: "table"`, `signals: ["traces"]` |
| `base_ir.operation == "catalog"` | Services topology page — always use `operation: "catalog"`, `catalog_field: "service_name"` |
| any other combination | *(no page context injected)* |

---

## §5  Metadata Injection Contract

### §5.1  Boundary: `GET /v1/nlq/metadata`

Stable API boundary used by `build_system_prompt()` and intended for future MCP server
separation. Returns:

- Schema-complete metrics for the tenant
- Available label keys (top-N from series catalog)
- Supported aggregations
- Common time range presets

When the MCP server moves to a separate process, only this call crosses the boundary.

### §5.2  SignalField Schema

Fields returned per metric from the Schema Registry (`mcp_tools::SignalField`):

| Field | Type | Notes |
|---|---|---|
| `field_name` | `String` | Metric name as in `metric_series.metric_name` |
| `display_name` | `Option<String>` | Human-readable name |
| `metric_type` | `Option<String>` | `"gauge"`, `"counter"`, `"histogram"`, `"summary"` |
| `unit` | `Option<String>` | Unit string (e.g. `"ms"`, `"bytes"`, `"req/s"`) |
| `business_description` | `Option<String>` | Plain-language description for the LLM |
| `interpretation_rule` | `Option<String>` | How to read / interpret this metric |
| `effective_sample_rate` | `Option<f64>` | 0.0–1.0; shown as `%` if < 1.0 |
| `schema_complete` | `bool` | True iff `metric_type` + `timestamp_column` both set |

Only `schema_complete = true` entries reach the LLM prompt.

---

## §5a  Page-Level `base_ir` Catalogue

All IR-driven pages ship a hardcoded `base_ir` constant that is merged with any user query
before execution. The table below is a complete inventory.

| Page / Component | Constant | `operation` | `signals` | Special notes |
|---|---|---|---|---|
| Infrastructure Inventory | `INFRA_BASE_IR` | `"inventory"` | `["metrics"]` | triggers "infrastructure inventory" page context |
| Log Search | `LOG_BASE_IR` | `"table"` | `["logs"]` | triggers "log search" page context |
| Log Live Tail | `LIVE_LOGS_BASE_IR` | `"table"` | `["logs"]` | `time_range.from = "now-5m"` (shorter live window) |
| Trace Search | `TRACE_BASE_IR` | `"table"` | `["traces"]` | triggers "trace search" page context |
| Service Topology | `TOPOLOGY_BASE_IR` | `"catalog"` | `["metrics"]` | triggers "services topology" page context |
| Product Area (Services list) | `SERVICES_BASE_IR` | `"catalog"` | `["metrics"]` | triggers "services topology" page context |
| Service Metrics Workspace | `METRICS_BASE_IR` | `"catalog"` | `["metrics"]` | no `catalog_field`; LLM infers it from question |

All constants share the same base skeleton (except `LIVE_LOGS_BASE_IR`):

```json
{
  "operation": "<see table>",
  "signals":   ["<see table>"],
  "filters":   [],
  "time_range": { "from": "now-1h", "to": "now" }
}
```

`LIVE_LOGS_BASE_IR` uses `"from": "now-5m"` instead of `"now-1h"`.

**Runtime override:** the frontend replaces `from`/`to` with Unix-nanosecond strings computed
from the global date picker before sending the request:

```ts
const from = String(BigInt(Math.floor(fromMs)) * 1_000_000n);
const to   = String(BigInt(Math.floor(toMs))   * 1_000_000n);
base_ir: { ...PAGE_BASE_IR, time_range: { from, to } }
```

---

## §6  Simple IR Shorthand (ADR-029)

A deterministic, zero-latency path that constructs `NlqIr` without calling the LLM.
Implemented in `parse_shorthand_ir()` in `llm_adapter.rs`.

### §6.1  Activation

| Condition | Behaviour |
|---|---|
| Query starts with `/` | Explicit LLM bypass — shorthand always used |
| No LLM configured | Automatic fallback — shorthand used for all queries |
| Query starts with `!` | Reserved: force LLM (not yet implemented) |

The leading `/` is stripped before tokenisation.

### §6.2  Token Syntax

Tokens are whitespace-separated. Double-quoted phrases are treated as a single token.

| Token pattern | Target field | Example | Result |
|---|---|---|---|
| `m:<name>` | `metric` | `m:http_requests` | `metric = "http_requests"` |
| `f:<field>:<val>` | `filters` (explicit prefix) | `f:service:checkout` | `{field: "service", op: "=", value: "checkout"}` |
| `op:<type>` | `operation` | `op:topk` | `operation = "topk"` |
| `<field>:<val>` | `filters` (shorthand) | `env:prod` | `{field: "env", op: "=", value: "prod"}` |
| `"quoted text"` | `query` | `"timeout error"` | `query = "timeout error"` |
| unquoted word | `query` | `checkout` | appended to `query` |

All shorthand filters use `op: "="` (equality only). Regex and comparison operators require
the full NlqIr JSON or LLM interpretation.

### §6.3  Examples

| User input | Interpretation |
|---|---|
| `/error` | `query: "error"` (within current page context) |
| `/m:request_latency service:checkout p99` | `metric: "request_latency"`, filter `service=checkout`, `query: "p99"` |
| `/op:catalog service_name` | `operation: "catalog"`, `query: "service_name"` |
| `/env:prod severity_text:ERROR` | filters `env=prod`, `severity_text=ERROR` |

### §6.4  Merge with `base_ir`

`apply_shorthand_to_ir(base, sh)` merges the parsed shorthand into the page's `base_ir`:

- Shorthand filters **override** base filters for the same field (case-insensitive key match);
  new fields are appended.
- Shorthand `metric`, `operation`, and `query` override base values when explicitly set.
- All other base fields (`signals`, `window`, `group_by`, `resolution`, `catalog_field`,
  `visualization_hint`, `percentiles`, `limit`, `time_range`) are **preserved unchanged**.

When no `base_ir` is available, `ShorthandIr::into_nlq_ir()` provides sensible defaults:
`operation: "timeseries"`, `time_range: { "from": "now-1h", "to": "now" }`, and
`signals: ["metrics"]` (or `["logs"]` when `query` is set and `metric` is not).

---

## §7  `base_ir` Protocol and IR Merge

### §7.1  The Three `base_ir` Modes

The request body field `base_ir` drives three execution patterns:

| Condition | Behaviour |
|---|---|
| `base_ir` set, no `question` | Execute `base_ir` directly (page-load pattern; no LLM call) |
| `base_ir` set, `question` present, `mode: "execute"` | Interpret question → `merge_irs(base_ir, user_ir)` → execute merged IR |
| `base_ir` set, `question` present, `mode: "interpret"` | `base_ir` guides LLM context only; raw interpreted IR returned, no merge, no execution |

### §7.2  `merge_irs` Rules

`merge_irs(base, user)` applies the following field-level merge strategy:

| Field | Rule |
|---|---|
| `operation` | Always preserved from base |
| `signals` | Always preserved from base |
| `catalog_field` | Always preserved from base |
| `filters` | Base filters first; user filters for the same field (case-insensitive) replace the base filter; user filters for new fields are appended |
| `time_range` | User `time_range` takes precedence when `from` is non-empty; otherwise base is used |
| `query` | Preserved from user IR |
| `metric` | Always `null` in merged result (base operation context governs) |
| `window`, `resolution`, `group_by`, `visualization_hint`, `percentiles`, `limit` | Always `null` / `[]` in merged result |

---

## §8  Validation and Repair Loop

### §8.1  IR Normalisation (`normalize_nlq_ir`)

Applied to every LLM-emitted IR value before `serde` deserialisation:

1. **Signal normalisation** (`normalize_nlq_signals`):
   - Unknown signal values → `"metrics"`
   - Empty or missing `signals` array → `["metrics"]`
2. **Null array coercion**: `null` values for `filters`, `group_by`, `percentiles` → `[]`
3. **Invalid op stripping**: filters with `op` not in `["=", "!=", "=~", "!~", ">", ">=", "<", "<="]`
   are silently removed (common case: LLM emits `"range"` for temporal constraints)
4. **Time range defaults**: missing or null `time_range` → `{"from": "now-24h", "to": "now"}`;
   null `from`/`to` sub-fields patched individually

### §8.2  Repair Loop

```
LLM call → parse_llm_response
  ├─ Ok  → proceed
  └─ InvalidResponse + repair_budget > 0
       → build_repair_prompt(original_question, error, raw_response)
       → LLM call (second turn; system prompt unchanged)
            ├─ Ok  → proceed with corrected IR
            └─ InvalidResponse → return NlqQueryResponse::InvalidResponse (HTTP 200)
```

- `MAX_REPAIR_ATTEMPTS = 1` (single retry)
- The repair prompt is a user-turn message appended to the same system prompt; the system
  prompt is not re-sent.
- Repair failures are logged at `warn!` level with the original question, error, raw response,
  and repair count.
- `InvalidResponse` is returned as HTTP 200 with the raw LLM output attached so clients can
  display it for debugging. It is treated as expected control flow, not a server error.

### §8.3  LLM Response Envelope Fallbacks (`parse_llm_response`)

The parser handles several non-canonical response shapes for compatibility with small/local LLMs:

| Shape | Description |
|---|---|
| `{"type":"ir","ir":{…}}` | Canonical form — always preferred |
| `{"type":"<op>","ir":{…}}` | Hybrid: type carries an operation name AND `ir` key present |
| `{"type":"<op>",…}` (bare) | LLM omitted envelope; `type` promoted to `operation` when it is a valid op name |
| `{"type":"response","content":{…}}` | Chat-wrapper: phi3.5-style; IR extracted from `content`/`result`/`data`/`output` |
| `{"type":"decline","reason":"…"}` | Canonical decline |
| `{"type":"capabilities"}` | Canonical capabilities short-circuit |

Valid operation names for envelope fallback: `timeseries`, `rate`, `irate`, `increase`,
`histogram`, `topk`, `table`, `distribution`, `catalog`.

---

## §9  SQL Template Library

All SQL generation is in `services/query-api/src/sql_templates.rs`. The contract:

- **Deterministic:** identical `NlqIr` always produces identical SQL
- **Tenant-scoped:** every generated query carries `tenant_id = '<uuid>'` in the WHERE clause
- **Filter values escaped** (untrusted); tenant ID and metric name are inlined directly
  (trusted, from validated context)
- **SELECT-only:** no mutations

### §9.1  Operation → SQL Pattern

| Operation | SQL pattern | Key tables |
|---|---|---|
| `timeseries` | `GROUP BY time_bucket(resolution, timestamp) ORDER BY bucket` | `metric_points` + `metric_series` |
| `rate` | `lag()` delta / elapsed seconds per window; counter-reset: if `value < lag(value)` use `value` | `metric_points` + `metric_series` |
| `irate` | Same as `rate` but only two most-recent samples | `metric_points` + `metric_series` |
| `increase` | `SUM(delta)` over window with reset detection | `metric_points` + `metric_series` |
| `histogram` | `arrayDifference(histogram_bucket_counts)` + `arrayJoin` to expand buckets | `metric_points` + `metric_series` |
| `topk` | `avg(value) per label GROUP BY … ORDER BY avg_value DESC LIMIT N` | `metric_points` + `metric_series` |
| `table` | `SELECT … ORDER BY time_unix_nano DESC LIMIT 1000` | `metric_points` + `metric_series` |
| `distribution` | `quantile(q)(value)` / `avg(value)` / `min` / `max` — no `GROUP BY` on time | `metric_points` + `metric_series` |
| `catalog` | `SELECT DISTINCT col, count() … FROM metric_series GROUP BY … ORDER BY count DESC LIMIT 100` | `metric_series` only (no time clause) |
| `inventory` | Infrastructure entity fetch with IR-derived filters | `infrastructure_entities` + recent metrics |

See [spec/08-ai-ml.md §13.3](08-ai-ml.md) for the full PromQL-equivalent SQL patterns.

### §9.2  Distribution Functions — `percentiles` → SQL

The `stat_to_sql_expr(stat, val)` function maps each stat name to a ClickHouse expression.
The value column is `coalesce(mp.value_double, toFloat64(mp.value_int))`.

| `percentiles` entry | ClickHouse expression | Output column alias |
|---|---|---|
| `"p{N}"` where N = 1–99 | `quantile(N/100.0)(value)` | `p{N}` |
| `"p{N}"` where N = 100–999 | `quantile(N/1000.0)(value)` | `p{N}` |
| `"p0"` or N > 999 | *(invalid — silently skipped)* | — |
| `"median"` | `quantile(0.50)(value)` | `median` |
| `"average"` | `avg(value)` | `average` |
| `"mean"` | `avg(value)` | `mean` |
| `"min"` | `min(value)` | `min` |
| `"max"` | `max(value)` | `max` |
| `"min_val"` | `min(value)` | `min_val` *(legacy alias)* |
| `"max_val"` | `max(value)` | `max_val` *(legacy alias)* |
| any other string | *(silently skipped)* | — |

**Default stats** (when `percentiles` is `null` or empty):
`["p50", "p90", "p95", "p99", "min", "max"]`

If all requested stats are skipped (e.g. all unrecognised), the template falls back to
`quantile(0.99)(value) AS p99` to guarantee a non-empty SELECT list.

### §9.3  Catalog Field Mapping

For `catalog` operations, `catalog_field` is mapped to a ClickHouse column expression
via `map_filter_field(field)`:

| `catalog_field` value | ClickHouse expression |
|---|---|
| `"service_name"` | `ms.service_name` |
| `"environment"` | `ms.environment` |
| `"metric_name"` | `ms.metric_name` |
| *(any other value)* | `JSONExtractString(ms.attributes, '<field>')` |

If the `attributes` JSON does not contain the requested field, the query returns empty rows
(not an error). This is intentional — the user can ask for any dimension.

---

## §10  VisualizationFrame Contract

The MCP server returns a typed, self-describing `VisualizationFrame` alongside every result:

```json
{
  "type": "timeseries | histogram | table | heatmap | topk | distribution | flamegraph | span_waterfall",
  "x_field": "<field_name>",
  "y_field": "<field_name>",
  "series_field": "<field_name>",
  "unit": "ms | req/s | bytes | ...",
  "suggested_visualization": "<panel_type>",
  "field_roles": [
    { "name": "<field>", "role": "timestamp | value | series | bucket | count | label" }
  ],
  "data": [...]
}
```

The `type` and `field_roles` map directly to Grafana's `DataFrame` + `PanelData` model
(see [ADR-016](adr/ADR-016-grafana-visualization-strategy.md)). The UI auto-selects the
correct panel type from `type` / `suggested_visualization` without manual configuration.

Every frame carries the provenance payload:
`nlq_ir`, `source_sql`, `time_range`, `signal_types`, `sample_rate`, `approximation_statement`.

---

## §11  Eval Harness and Quality Gate

The NLQ eval harness (`scripts/nlq-eval.py` + `tests/nlq/cases.json`) is the primary
regression gate for the full pipeline. See [spec/08-ai-ml.md §13.4](08-ai-ml.md) for the
complete operation reference, feedback loop, and instructions for running the eval.

**Any change to the following surfaces requires running the eval harness and showing no regressions:**

| Surface | Location |
|---|---|
| System prompt (static instruction layer) | `services/query-api/src/llm_adapter.rs` `build_system_prompt()` |
| IR schema (`NlqIr` struct or field semantics) | `libs/domain/src/nlq.rs` |
| SQL templates | `services/query-api/src/sql_templates.rs` |
| Metadata injection (dynamic layer) | `services/query-api/src/llm_adapter.rs` `fetch_schema_context()` |
| IR parser / repair loop | `services/query-api/src/llm_adapter.rs` `parse_llm_response()` |
| Eval test cases themselves | `tests/nlq/cases.json` |

---

## §12  Advisory Constraints Summary

These constraints are permanent and cannot be relaxed (see [ADR-014](adr/ADR-014-ai-feature-boundaries.md)):

1. Results are **approximate** by design (sampled traces, best-effort delivery, clock skew).
   Every response must include an explicit `approximation_statement`.
2. Results **must not** be used for billing, SLA enforcement, contractual compliance, or
   regulatory reporting.
3. All queries execute under the **caller's tenant context**. The LLM and MCP server cannot
   bypass tenant isolation or RBAC.
4. Every response carries a **provenance payload**: NlqIr, raw SQL, signals consulted,
   effective sample rate per signal, and approximation statement.
5. No AI-initiated writes, alerts, or auto-remediation without explicit policy gates and
   human-in-the-loop approval.
6. The LLM must **state the advisory boundary** when a question falls near billing, SLA,
   or regulatory categories — even when declining.
