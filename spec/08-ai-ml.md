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
```

**Stage 1 — LLM → NLQ IR**

1. **Intent classification** — the LLM classifies the question into one or more signal types
   (traces, metrics, logs, events, topology) and time ranges.
2. **Schema grounding** — the LLM calls MCP server tools (`get_metric_schema`, `list_signal_fields`,
   `resolve_label_to_column`) to resolve field names and types from the Schema Registry.
3. **IR generation** — the LLM emits a structured **NLQ IR** (not SQL), capturing operation,
   metric, filters, grouping, time window, resolution, and visualization hint. The IR is stable
   and testable independent of the LLM.

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

### Hard Rules

- every model decision needs provenance — expose the raw queries, signals consulted, and sample rates
- no opaque auto-remediation without policy gates
- AI outputs are advisory unless explicitly approved
- every NL query response must include an explicit approximation statement
- the LLM must not bypass tenant isolation or RBAC — all queries execute under the caller's context
- the LLM must decline to answer questions requiring BI-grade correctness and explain why
