# AI/ML Features

## 13. AI/ML Features

Ship late, not early. Foundational observability (Phases 0–4) must be solid before Phase 8
intelligence features are introduced broadly. See [ADR-014](adr/ADR-014-ai-feature-boundaries.md).

---

### Phase 1

- smart grouping
- query suggestions
- schema hints
- baseline anomalies
- root-cause candidate ranking

### Phase 2

- **natural language query** (see §13.1)
- incident summarization
- topology-aware blast-radius estimation
- remediation suggestions
- dashboard generation from service metadata

---

### 13.1 Natural Language Query

#### Concept

The platform's telemetry store — ClickHouse-backed, Arrow/DataFusion query layer, rich OTel schema —
is already a data platform for operational data. The missing piece is a query UX that makes it
accessible to non-engineers. A natural language interface closes that gap without building a
separate BI pipeline and without introducing a proprietary query DSL (see [ADR-026](adr/ADR-026-no-proprietary-query-dsl.md)).

This capability is positioned not as a replacement for a traditional BI data platform, but as a
**faster, cheaper tier for operational and tactical business questions**. Traditional BI guarantees
correctness but delivers answers slowly and at high cost. For most operational decisions, an
approximate answer at decision time has higher expected value than a precise answer after the
window has closed. Observability data is approximate by design (sampled traces, best-effort
delivery, clock skew), but cross-signal corroboration raises confidence — see §13.2.

#### Architecture

The NLQ layer is a three-stage pipeline:

```
NLQ (user) → LLM → NLQ IR → MCP Server → SQL/DataFusion → VisualizationFrame → UI
Raw NlqIr JSON (user) ───────┘
```

**Stage 1 — LLM → NLQ IR**

The LLM and the MCP server never communicate directly. The **backend mediates everything**:
it fetches metadata, builds the prompt, calls the LLM, validates the response, and — when
necessary — retries with a targeted repair prompt before forwarding the IR to the MCP server.

**Stage 1a — Backend builds the prompt (two layers)**

The system prompt has two layers assembled fresh for every request:

1. **Static instruction layer** (same for every request): the NlqIr JSON schema, the full
   operation guide with examples of valid and invalid IR, time-range parsing rules, metric
   selection rules, filter construction rules, visualization hint rules, advisory boundary
   instructions, and repair-loop instructions.

2. **Dynamic metadata layer** (per-tenant, per-request): the backend fetches live metadata
   from the MCP server's metadata boundary (`GET /v1/nlq/metadata`) and injects it:
   - Available metrics (schema-complete entries: name, type, unit, description, sample rate)
   - Available label keys (top-N distinct keys from the series catalog: `service_name`,
     `environment`, `pod`, `region`, etc.)
   - Service scope, if the question is scoped to a specific service

Without this injection the LLM would hallucinate metric names and guess label keys.
**Metadata injection = LLM's current reality.**

**Stage 1b — LLM generates IR**

The LLM receives the rendered prompt and the user's question. It emits one of:
- `{"type": "ir", "ir": <NlqIr>}` — a valid NLQ IR
- `{"type": "decline", "reason": "..."}` — question is out of scope or unanswerable
- `{"type": "capabilities"}` — user asked a meta-question about the system

The LLM emits IR, never SQL. The IR is stable and testable independent of the LLM.

`POST /v1/nlq` also accepts `mode: "interpret"` for frontend filter replacement. In interpret
mode, natural language is translated and validated into `{"type":"ir","ir":<NlqIr>}` without
executing SQL. If the submitted question is raw `NlqIr` JSON, the backend validates it directly and
does not require LLM configuration. This is the no-LLM fallback; no mini-language or proprietary
query syntax is introduced.

**Stage 1c — Validation and repair loop**

The backend validates the returned IR structurally. If invalid, it attempts one repair:

1. Backend parses and validates the IR.
2. If parsing fails or the IR references an unknown metric, **and** the repair budget is not
   exhausted (`MAX_REPAIR_ATTEMPTS = 1`): backend sends a repair prompt to the LLM containing
   the original question, the faulty response, and the specific error message.
3. LLM emits a corrected IR, or declines if it cannot recover.
4. If the repair also fails, the pipeline returns `InvalidResponse` or `Decline` as appropriate.

This loop is invisible to the user; only the final result is returned.

Example NLQ IR:

```json
{
  "operation": "rate",
  "signals": ["metrics"],
  "metric": "http_requests_total",
  "window": "5m",
  "filters": [{"field": "method", "op": "=", "value": "GET"}],
  "group_by": ["pod"],
  "resolution": "1m",
  "time_range": {"from": "now-1h", "to": "now"},
  "visualization_hint": "timeseries"
}
```

**Stage 2 — MCP Server (IR → SQL/DataFusion)**

The MCP server translates the NLQ IR into SQL using a library of time-series SQL templates.
It encodes all observability-specific semantics in code — not in LLM prompts:

- Selects the correct SQL pattern based on `operation` and `metric_type` from the Schema Registry
- Handles counter-reset detection for `rate`/`irate`/`increase` operations
- Generates `WINDOW` frames for range vectors
- Emits `time_bucket()` clauses for downsampling to requested resolution
- Constructs tenant-scoped SQL (the caller's tenant context is injected; RBAC is not bypassed)
- See §13.3 for the full time-series SQL pattern library

**Stage 3 — VisualizationFrame (auto-graphing)**

The MCP server returns a typed, self-describing **VisualizationFrame** with the query result:

```json
{
  "type": "timeseries",
  "x_field": "bucket",
  "y_field": "rate",
  "series_field": "pod",
  "unit": "req/s",
  "suggested_visualization": "timeseries",
  "field_roles": [
    {"name": "bucket", "role": "timestamp"},
    {"name": "rate", "role": "value"},
    {"name": "pod", "role": "series"}
  ],
  "data": [...]
}
```

Supported VisualizationFrame types: `timeseries`, `histogram`, `table`, `heatmap`, `topk`,
`distribution`, `flamegraph`, `span_waterfall`.

The VisualizationFrame maps to Grafana's `DataFrame` + `PanelData` model consumed by
`@grafana/ui`. The UI auto-selects the correct Grafana panel without guessing.
This is the **auto-graphing** contract (see [ADR-016](adr/ADR-016-grafana-visualization-strategy.md)).

For the full list of supported operations, their SQL patterns, disambiguation rules, and the eval
harness feedback loop, see **§13.4** below.

**Provenance payload**

Every response includes:
- the NLQ IR that was generated
- the raw SQL query executed
- the signals consulted
- the effective sample rate per signal
- an explicit approximation statement

#### Scope

Natural language query answers **operational and tactical** questions well:
- *"Which customers are experiencing errors in the checkout service right now?"*
- *"Did the 14:01 deploy increase p99 latency?"*
- *"Which tenant consumed the most ingest bandwidth this week?"*
- *"What changed around the time error rate spiked?"*
- *"Show me the request rate histogram for GET requests grouped by pod over the last hour"*

It is **not appropriate** for:
- regulatory compliance reporting
- financial reconciliation
- contractual SLA evidence
- any use case where auditability of data completeness is required

The LLM must state this boundary explicitly when the question falls in or near these categories.

#### PromQL Compatibility Façade (optional, metrics-only)

If implemented, a PromQL parser inside the MCP server translates PromQL expressions into the
same NLQ IR. This is a thin front-end, not a new query engine:
- Metrics-only — PromQL cannot express log, trace, or cross-signal queries
- Optional — not required for NLQ or any other platform feature
- The PromQL syntax is contained within the MCP server; no other component is PromQL-aware

#### LLM Backend Configuration

The NLQ pipeline connects to any OpenAI-compatible LLM endpoint. All three settings are
configurable on the Setup page or via env vars:

| Setting | `platform_config` key | Env var | Fallback env var | Default |
|---|---|---|---|---|
| API Key | `llm_api_key` | `LLM_API_KEY` | — | _(none)_ |
| Endpoint URL | `llm_url` | `LLM_URL` | `OPENAI_BASE_URL` | `https://api.openai.com/v1` |
| Model | `llm_model` | `LLM_MODEL` | `OPENAI_MODEL` | `gpt-4o-mini` |

Env vars take priority over Setup page values. Leaving API Key blank is valid for vLLM and
other unauthenticated OpenAI-compatible endpoints (point Endpoint URL at your vLLM server).

The API key is XOR-obfuscated before storage in the `platform_config` PostgreSQL table.
Config is stored in the `platform_config` key-value table; no schema migration is required
for new config keys. See [ADR-027](adr/ADR-027-local-llm-backend.md) for the full decision record.

#### Prerequisite

The Schema Registry semantic annotations layer ([spec/03 §5.4.1](03-storage.md)) must be in place,
including the metric-type extensions (`metric_type`, `timestamp_column`, `recommended_downsampling`),
before the MCP server can generate correct time-series SQL.

---

### 13.2 Cross-Signal Triangulation

Individual observability signals are approximate. Traces are sampled; logs may be dropped under
backpressure; metric counters may miss events near restart boundaries. However, when multiple
independent signals point to the same conclusion, confidence rises significantly — because each
signal has different failure modes.

The LLM reasoning layer should apply cross-signal triangulation as a first-class reasoning pattern:

1. **Decompose** the question into sub-queries per available signal type.
2. **Execute** each sub-query independently.
3. **Compare** the conclusions. Convergence (multiple signals agree) increases confidence.
   Divergence (signals disagree) is an honest uncertainty that must be surfaced.
4. **Report** confidence based on convergence, not on any single signal.

**Example:**

> *"Did the deploy at 14:01 cause the error rate increase?"*
>
> - Traces: error rate rose from 0.3% to 4.1% at 14:03. ✓
> - Metrics: p99 latency increased 40% at 14:02. ✓
> - Logs: `PaymentService` began emitting timeout exceptions at 14:03. ✓
> - Events: deploy marker recorded at 14:01. ✓
>
> **Conclusion:** Four independent signals converge. Confidence: high.
> All figures based on ~5% tail-sampled traces; absolute error counts are estimates.

Cross-signal triangulation is architecturally feasible because Observable's correlation model
([spec/14 §3](14-domain-model.md)) shares stable identity dimensions across all signal types
(`tenant_id`, `service_name`, `trace_id`, `deployment_id`, `environment`).

---

### 13.3 Time-Series Semantics in SQL

The MCP server implements PromQL-level time-series power as SQL patterns. This section documents
the canonical mapping used by the MCP server's SQL template library.

#### Range Vectors → SQL Window Frames

PromQL range vectors (`metric[5m]`) translate to SQL `RANGE` window frames:

```sql
avg(value) OVER (
  PARTITION BY series_id
  ORDER BY timestamp
  RANGE BETWEEN INTERVAL '5 minutes' PRECEDING AND CURRENT ROW
)
```

#### Rate / irate / increase → SQL Diffs + Windowing

Counter-based functions use `lag()` with reset detection:

```sql
SELECT
  series_id,
  timestamp,
  CASE
    WHEN value < lag(value) OVER w THEN value  -- counter reset
    ELSE (value - lag(value) OVER w)
         / EXTRACT(EPOCH FROM (timestamp - lag(timestamp) OVER w))
  END AS rate
FROM metrics
WINDOW w AS (PARTITION BY series_id ORDER BY timestamp)
```

The MCP server reads `metric_type = 'counter'` from the Schema Registry to determine when
counter-reset handling is required.

#### Label Selectors → SQL WHERE Clauses

| PromQL | SQL |
|---|---|
| `{method="GET"}` | `WHERE method = 'GET'` |
| `{status=~"5.."}` | `WHERE status ~ '5..'` |
| `{pod!=""}` | `WHERE pod IS NOT NULL AND pod != ''` |

#### Downsampling → SQL Time Buckets

```sql
SELECT
  series_id,
  time_bucket('1m', timestamp) AS bucket,
  avg(value) AS avg_value
FROM metrics
GROUP BY series_id, bucket
ORDER BY bucket
```

The bucket function is dialect-specific: `time_bucket()` for ClickHouse/TimescaleDB, or
equivalent `toStartOfMinute()` for ClickHouse, or `DATE_TRUNC()` for standard SQL.

#### Histogram Bucket Expansion

```sql
SELECT
  width_bucket(response_time_ms, 0, 2000, 20) AS bucket,
  count(*) AS count
FROM traces
WHERE service = 'checkout-api'
GROUP BY bucket
ORDER BY bucket
```

#### Cross-Signal Joins → SQL JOINs

PromQL vector matching is expressed as SQL joins keyed on timestamp bucket and label columns:

```sql
SELECT a.pod, a.bucket, a.req_rate, b.error_rate
FROM (
  SELECT pod, time_bucket('1m', timestamp) AS bucket, rate(...) AS req_rate FROM metrics ...
) a
JOIN (
  SELECT pod, time_bucket('1m', timestamp) AS bucket, error_rate FROM metrics ...
) b ON a.bucket = b.bucket AND a.pod = b.pod
```

---

### 13.4 NLQ Query Path: Supported Operations, Design Choices, and Capabilities

This section documents the nine NLQ operations, their intended use cases, key SQL design choices,
and the disambiguation rules that guide both the LLM and the SQL template library. It is the
canonical reference for any change to the system prompt, IR schema, or SQL templates.

#### Operation Reference

| Operation | Use case | Metric requirement | Output shape |
|---|---|---|---|
| `timeseries` | Trend chart over time | Any gauge or histogram | Many rows (bucket, value) |
| `rate` | Per-second rate of a counter | Monotonic counter only | Many rows (bucket, rate) |
| `irate` | Instantaneous rate from last two samples | Monotonic counter only | Many rows (bucket, rate) |
| `increase` | Total counter increase over window | Monotonic counter only | One or many rows |
| `histogram` | OTel Histogram bucket distribution | `metric_type = histogram` only | Many rows (bound, count) |
| `topk` | Rank entities by computed metric value | Any | N rows (label, value) |
| `table` | Raw point scan — most recent rows | Any | Up to 1000 rows |
| `distribution` | Scalar stats for a single time window | Any gauge or histogram | One row (p95, avg, …) |
| `catalog` | Enumerate distinct values of a dimension | None (series metadata) | Many rows (field, count) |

#### Operation Design Choices

**`timeseries` vs `distribution`**

The primary disambiguation: does the user want a chart or a single number?

- `timeseries` groups rows by `time_bucket(resolution, timestamp)` and emits one row per bucket.
  The SQL pattern is `GROUP BY bucket ORDER BY bucket`.
- `distribution` aggregates the entire time range into a single row using `quantile()`, `avg()`,
  `min()`, `max()`. No `GROUP BY` on time.

Rule: if the user names a specific stat (`p95`, `average`, `median`, `p99`, `min`, `max`) → `distribution`.
If the user asks for a metric by name with a time range but no specific stat → `timeseries`.

**`histogram` vs `distribution`**

Both operate on latency data but from different OTel instruments:

- `histogram` consumes OTel Histogram bucket columns (`histogram_explicit_bounds`,
  `histogram_bucket_counts`). It uses `arrayDifference` to reconstruct per-bucket counts and
  `arrayJoin` to expand them into rows. Only valid when `metric_type = 'histogram'`.
- `distribution` uses `value_double` / `value_int` scalar values via `quantile()`. Valid for
  gauge and counter metrics. It also works for summary metrics that record pre-computed percentiles.

Rule: use `histogram` only when the metric type is histogram AND the user explicitly asks for
"histogram", "bucket distribution", or "buckets". Use `distribution` for all other percentile/stat
queries.

**`rate` / `irate` / `increase` — counter semantics**

All three require `metric_type = 'counter'` in the Schema Registry:

- `rate` — `(value - lag(value)) / elapsed_seconds` per window, with counter-reset detection.
  When `value < lag(value)`, the counter has reset and the delta is taken as `value` (not negative).
- `irate` — same formula but over only the two most recent samples in the window (instantaneous).
- `increase` — `SUM(delta)` over the window, with the same reset detection logic as `rate`.

None of these functions make sense for gauge metrics. The SQL template selects the correct pattern
based on `metric_type` from the Schema Registry; the LLM only needs to choose the right operation.

**`topk` — ranking by value**

`topk` aggregates all samples in the time range with `avg(value)` per label combination, then
takes the top-N rows with `ORDER BY avg_value DESC LIMIT N`. The `limit` IR field controls N.

Disambiguation from `catalog`: `topk` ranks by a **computed metric value** (requires `metric`
field). `catalog` enumerates **what entities exist** (no metric, no aggregation).

**`catalog` — series metadata enumeration**

`catalog` queries the `metric_series` metadata table, not the `metric_points` data table.
This means it is very fast, contains no time-range logic, and returns results even for
metrics that have not received data recently.

The `catalog_field` field controls which dimension is enumerated:

| User question | `catalog_field` | SQL expression |
|---|---|---|
| "list all services" | `service_name` | `ms.service_name` |
| "what environments exist?" | `environment` | `ms.environment` |
| "what metrics does X emit?" | `metric_name` | `ms.metric_name` |
| "what pods does X use?" | `pod` | `JSONExtractString(ms.attributes, 'pod')` |

First-class columns (`service_name`, `environment`, `metric_name`) are mapped directly. All other
field names fall back to `JSONExtractString(ms.attributes, '<field>')`. If the attributes JSON
does not contain the field, the query returns empty (not an error). This is intentional: the user
can ask for any dimension; the system will truthfully return nothing if the data does not exist.

**`table` — raw point scan**

`table` is a `LIMIT 1000 ORDER BY time_unix_nano DESC` point scan with no aggregation. It is the
escape hatch for users who want to inspect raw data. The `metric` field narrows the scan to a
specific metric; without it the scan is across all series for the tenant.

#### Eval Harness and Regression Gate

The NLQ eval harness (`scripts/nlq-eval.py` + `tests/nlq/cases.json`) is the primary quality
gate for this pipeline. Every test case exercises the full end-to-end path:

```
user question → LLM → NLQ IR → SQL template → ClickHouse → VisualizationFrame → assertions
```

**When the harness must be run and updated:**

Any change to one of these surfaces requires running the eval harness and showing no regressions:

| Surface | Where defined |
|---|---|
| System prompt (static instruction layer) | `services/query-api/src/llm_adapter.rs` `build_system_prompt()` |
| IR schema (`NlqIr` struct or field semantics) | `libs/domain/src/nlq.rs` |
| SQL templates (operation → SQL mapping) | `services/query-api/src/sql_templates.rs` |
| Metadata injection (dynamic layer) | `services/query-api/src/llm_adapter.rs` `fetch_schema_context()` |
| IR parser / repair loop | `services/query-api/src/llm_adapter.rs` `parse_llm_response()` |
| Eval test cases themselves | `tests/nlq/cases.json` |

**Feedback loop:**

```
Run eval
  │
  ├─ All pass → done ✓
  │
  └─ Failures → diagnose via last-run.json
                  │
                  ├─ wrong operation → add/fix disambiguation rule in static instruction layer
                  ├─ invalid_response → add to repair loop examples or fix IR normalization
                  ├─ empty data → fix SQL template or filter logic in sql_templates.rs
                  └─ new test case → add regression case to tests/nlq/cases.json
```

Run the eval with:

```bash
python3 scripts/nlq-eval.py --url http://localhost:8080
```

See `tests/nlq/last-run.json` for the structured per-case results including raw IR and SQL.

---

### Hard Rules

- every model decision needs provenance — expose the raw queries, signals consulted, and sample rates
- no opaque auto-remediation without policy gates
- AI outputs are advisory unless explicitly approved
- every NL query response must include an explicit approximation statement
- the LLM must not bypass tenant isolation or RBAC — all queries execute under the caller's context
- the LLM must decline to answer questions requiring BI-grade correctness and explain why
- any change to the NLQ→IR→SQL pipeline must be covered by the eval harness — see §13.4
