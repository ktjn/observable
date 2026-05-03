# ADR-021: LLM Natural Language Query Layer

**Date:** 2026-04-19 (updated 2026-05-01)
**Status:** Proposed
**Authors:** ktjn
**Deciders:** Project Stakeholders
**Review date:** 2026-10-28

## Context

Observable's telemetry store is already a data platform for operational data: ClickHouse-backed
columnar storage, Arrow/DataFusion query execution, a rich OTel-aligned schema, multi-signal
correlation through stable identity dimensions. The missing piece is a query UX that makes this
accessible to non-engineers without requiring knowledge of SQL, DataFusion expressions, or the
OTel attribute model.

Traditional BI data platforms solve a related problem but with a different cost/correctness
trade-off:
- BI guarantees certified, governed answers — but requires weeks to months of pipeline,
  modeling, and QA work per question, and the majority of operational questions are never answered
  because the cost exceeds the decision value.
- The observability store delivers approximate answers in seconds at near-zero marginal cost per
  question — because the data is already collected, indexed, and queryable.

For most operational and tactical business decisions, an approximate answer at decision time has
higher expected value than a precise answer delivered after the decision window has closed.
Observability data is approximate by design (sampled traces, best-effort delivery, clock skew),
but cross-signal corroboration — traces, metrics, logs, events, and topology independently
converging on the same conclusion — raises confidence without requiring a curated warehouse.

This ADR captures the decision to position natural language query as a first-class query UX in
Observable, and to define the architectural approach and constraints.

## Decision

Observable will provide a natural language query interface as a Phase 8 AI capability, built as a
three-stage pipeline:

```
NLQ (user) → LLM → NLQ IR → MCP Server → SQL/DataFusion → VisualizationFrame → UI
Raw NlqIr JSON (user) ───────┘
```

No proprietary query DSL will be introduced at any stage. SQL/DataFusion is the canonical query
IR. See [ADR-026](ADR-026-no-proprietary-query-dsl.md).

The same query input is also the primary filter UX in the frontend. Selector-style filters compile
to or are replaced by `NlqIr` filters. When no LLM is configured, users may paste raw `NlqIr` JSON
as the deterministic intermediate-format fallback; the backend validates and executes it without
calling an LLM.

### Stage 1 — NLQ → LLM → NLQ IR

The LLM and the MCP server never communicate directly. The **backend mediates all
communication**: it fetches metadata, builds the prompt, calls the LLM, validates the
response, and — when necessary — retries with a targeted repair prompt.

#### Two-layer prompt construction

Every NLQ request produces a fresh prompt with two layers:

**Static instruction layer** (constant across requests):
- NlqIr JSON schema with all fields, valid values, and constraints
- Operation guide with examples of valid IR for each operation type
- Examples of invalid IR (hallucinated metrics, wrong signals field, etc.)
- Time-range parsing rules and aggregation rules
- Advisory boundary instructions (when to emit `decline`)
- Repair-loop instructions (what to do when asked to correct an error)

**Dynamic metadata layer** (per-tenant, per-request, fetched via `GET /v1/nlq/metadata`):
- Available metrics: schema-complete entries (name, type, unit, business description,
  sample rate). Capped at 20 entries sorted by annotation richness to respect token budget.
- Available label keys: top-N distinct keys present in the series catalog for this tenant
  (e.g. `service_name`, `environment`, `pod`, `region`, `namespace`). Prevents hallucination
  of label names.
- Service scope: if the question is scoped to a service, injected as a constraint.

Without the dynamic layer the LLM would hallucinate metric names and guess label keys.

#### LLM response types

The LLM emits exactly one of three JSON shapes:
- `{"type": "ir", "ir": <NlqIr>}` — a valid NLQ IR
- `{"type": "decline", "reason": "..."}` — question is out of scope or unanswerable
- `{"type": "capabilities"}` — user asked a meta-question about system capabilities

For UI filter replacement, `POST /v1/nlq` supports an `interpret` mode that returns
`{"type":"ir","ir":<NlqIr>}` after validation and service-scope enforcement without executing the
MCP query. Raw `NlqIr` JSON is accepted in both `interpret` and execute modes before LLM
configuration is checked.

A `base_ir` field in the request body provides page-level context for both the LLM and query
execution. When set, the backend derives surface context directly from `base_ir` — for example,
`base_ir.signals == ["logs"]` means log surface, `base_ir.operation == "inventory"` means
infrastructure surface — without requiring a separate `surface_hint` string.

The `base_ir` field serves three purposes:

1. **Page load data source**: `mode: "execute"` with no `question` → execute `base_ir` directly
2. **Merge base**: `mode: "execute"` with `question` → interpret → `merge_irs(base_ir, user_ir)` → execute
3. **LLM context signal**: `build_system_prompt` derives page context from `base_ir` instead of a
   separate string hint

#### Validation and repair loop

After the LLM responds, the backend validates and optionally repairs:

1. Parse and structurally validate the response.
2. If invalid and repair budget > 0 (`MAX_REPAIR_ATTEMPTS = 1`): send a **repair prompt**
   to the LLM containing the original question, the faulty response, and the specific error.
   The LLM is asked to correct only the failing field, keeping valid parts unchanged.
3. If repair succeeds: proceed with the corrected IR.
4. If repair also fails: return `InvalidResponse` or `Decline` to the caller.

The repair loop is invisible to the user; only the final result is returned.
Repair attempts are logged at `warn!` level for quality tracking.

#### MCP metadata boundary

`GET /v1/nlq/metadata` is the stable interface through which the backend fetches tenant
metadata for prompt construction. It returns metrics, label keys, supported aggregations,
and common time range presets. This boundary enables future MCP server separation: if the
MCP server moves to a separate process, only this call crosses the boundary.

### Stage 2 — MCP Server (IR → SQL/DataFusion)

An **MCP server** (Model Context Protocol server) receives the NLQ IR and translates it to
SQL/DataFusion plans. The MCP server:

- Encodes all time-series semantics in code, not in LLM prompts or SQL templates passed to the LLM
- Knows metric types (counter, gauge, histogram, summary) from the Schema Registry
- Selects the correct SQL pattern for each operation type (see §Time-Series Semantics below)
- Generates tenant-scoped SQL with the caller's tenant context — it does not bypass authorization
- Returns a typed **VisualizationFrame** (see §VisualizationFrame Contract below)
- Can cache intermediate results and push down filters for cardinality reduction

The MCP server is the abstraction boundary between the LLM reasoning layer and the execution
substrate. If the execution substrate changes (ClickHouse SQL → DataFusion SQL → Arrow compute
kernels), the NLQ IR and the MCP server API remain stable.

The MCP server exposes two external boundaries used by the backend:
- `GET /v1/nlq/metadata` — fetches tenant metadata for prompt construction (metrics, label keys)
- `POST /v1/mcp/query` — executes a validated NlqIr and returns a VisualizationFrame

The MCP server does NOT expose tools that the LLM calls directly. All LLM interaction is
mediated through the backend.

### Stage 3 — VisualizationFrame → UI

The MCP server returns a typed, self-describing **VisualizationFrame** alongside the query result:

```json
{
  "type": "histogram | timeseries | table | heatmap | topk | flamegraph | distribution",
  "x_field": "<field_name>",
  "y_field": "<field_name>",
  "series_field": "<field_name>",
  "unit": "ms | req/s | bytes | ...",
  "suggested_visualization": "<panel_type>",
  "field_roles": [
    {"name": "le", "role": "bucket"},
    {"name": "count", "role": "value"}
  ],
  "data": [...]
}
```

The VisualizationFrame maps directly to Grafana's `DataFrame` + `PanelData` model used by
`@grafana/ui` (see [ADR-016](ADR-016-grafana-visualization-strategy.md)). The UI auto-selects
the correct Grafana panel type based on `type`/`suggested_visualization` without guessing.
This is the **auto-graphing** contract.

### Cross-signal triangulation

Where the question admits multiple independent signals, the LLM generates one NLQ IR per signal.
The MCP server executes them in parallel and the LLM compares results for convergence or
divergence. See [spec/08 §13.2](../08-ai-ml.md).

### Entity Inventory Queries

The `inventory` NLQ operation enables pure entity-attribute filtering on entity-table pages (e.g.,
the infrastructure inventory page). Unlike time-series operations, `inventory` requires no metric:

```json
{"operation": "inventory", "filters": [{"field": "entity_type", "op": "=", "value": "pod"}]}
```

The MCP server executes inventory queries by calling the infrastructure ClickHouse SQL with
IR-derived filters and returning a `VisualizationFrame(table)` whose rows are serialised
`InfrastructureEntitySummary` objects. Server-side filterable fields: `environment`, `entity_type`,
`service_name`, and free-text search on `display_name`/`name`. The `health_state` field is computed
post-query from `error_rate` and remains a client-side filter only.

Entity-table pages and all signal-browsing pages are fully IR-driven via server-side merge:
the frontend sends `base_ir` (a page-specific constant) and an optional `question` string.
The backend calls `merge_irs(base_ir, user_ir)` — preserving base `operation` and `signals`,
letting user filters override same-field base filters, and using the user's `time_range` when
present — before executing the merged IR. This removes all `surface_hint` coupling from the
protocol. The `question` field is optional: when absent, `base_ir` is executed directly for
page-load queries.

### Time-Series Semantics

PromQL-level time-series power is implemented as SQL patterns inside the MCP server, not as a
new language:

| PromQL concept | MCP server SQL pattern |
|---|---|
| `metric[5m]` range vector | `RANGE BETWEEN INTERVAL '5 minutes' PRECEDING AND CURRENT ROW` window frame |
| `rate(counter[5m])` | `(value - lag(value) OVER w) / epoch_diff` with counter-reset handling |
| `irate(counter[1m])` | Same as `rate` but over the two most-recent samples in the window |
| `increase(counter[1h])` | `SUM(delta) OVER w` with reset detection |
| `label_selector{k=v}` | `WHERE k = 'v'` or `WHERE k ~ 'pattern'` |
| `sum by (label)` | `GROUP BY label` |
| Downsampling / resolution | `time_bucket('1m', timestamp)` or equivalent `GROUP BY` time bucket |
| Histogram bucket expansion | `width_bucket()` / explicit `le` column expansion |
| Vector matching / join | SQL `JOIN` on timestamp + label columns |

### PromQL Compatibility Façade (optional, metrics-only)

If implemented, a PromQL parser inside the MCP server translates PromQL expressions into the
same NLQ IR. This is a thin front-end, not a new query engine. It is:

- Metrics-only (PromQL cannot express log, trace, or cross-signal queries)
- Optional (not required for NLQ or any other platform feature)
- Contained within the MCP server; no other component is aware of PromQL syntax

### Boundary

This capability targets operational and tactical questions. It explicitly does not target
regulatory compliance, financial reconciliation, or contractual SLA evidence. The LLM must
decline to answer questions in these categories and explain why.

### Prerequisite

The Schema Registry semantic annotations layer (see [spec/03 §5.4.1](../03-storage.md)) must be
available, including the metric-type extensions (`metric_type`, `timestamp_column`,
`recommended_downsampling`), before the MCP server can generate correct time-series SQL.

## Consequences

**Easier:**
- Non-engineers (product managers, finance, support) can query operational data without SQL
  knowledge or analyst intermediation.
- Questions that would never be answered in a BI pipeline get answered in seconds.
- The platform differentiates against traditional observability tools that offer only structured
  query UIs.
- Auto-graphing eliminates panel type selection from operator cognitive load.
- PromQL-level time-series semantics are available without adopting PromQL as a language.
- The IR is stable and versioned independently of the LLM and of the SQL dialect.
- The MCP server can be tested deterministically — given an IR, assert the SQL output.

**Harder:**
- Users must be educated about the approximation bounds of answers — sample rates, dropped
  events, clock skew — to avoid misusing approximate results in inappropriate contexts.
- The Schema Registry semantic annotations require ongoing operator maintenance to stay accurate
  as instrumentation evolves.
- LLM query generation quality depends on annotation completeness; unannotated fields produce
  lower-quality IR.
- The MCP server must maintain a library of correct SQL templates for each time-series operation.

**Constrained:**
- The NL query layer must not be used as a path to bypass RBAC or tenant isolation.
- All SQL executed by the MCP server must carry the caller's tenant context.
- Answers must always carry a provenance payload and an approximation statement — this is not
  optional.
- The LLM must never communicate with the MCP server directly. The backend mediates all
  communication: metadata fetch, prompt construction, IR validation, and repair.
- The repair loop is capped at `MAX_REPAIR_ATTEMPTS = 1` to bound LLM call latency.
- Repair attempts must be logged (at `warn!` level) for quality tracking and future improvement.
- Natural language query is advisory output. It must not feed automated alert evaluation,
  billing, or SLA enforcement.
- The LLM must decline questions requiring BI-grade correctness and explain why.
- The NLQ eval harness (`scripts/nlq-eval.py` + `tests/nlq/cases.json`) is a protected
  regression gate for the NLQ pipeline. Any change to the system prompt, IR schema, SQL
  templates, metadata injection, or repair loop must include updated test cases and a recorded
  eval run showing no regressions. See [spec/10-process.md §16.7](../10-process.md) for the
  mandatory NLQ Quality Gate rule and [spec/08-ai-ml.md §13.4](../08-ai-ml.md) for the full
  operation reference, design rationale, and feedback loop.

## Alternatives Considered

### Option A: Direct LLM → SQL generation (no IR, no MCP server)

The LLM generates SQL strings directly from natural language. Simpler to prototype.

Rejected because:
- SQL generation is brittle — small schema or dialect changes break the LLM's output.
- Time-series semantics (rate, irate, counter resets, histogram buckets) are extremely hard to
  encode reliably in LLM prompts. The MCP server's code-based templates are deterministic.
- No stable IR means the generated SQL cannot be tested, versioned, or audited independently.
- Auto-graphing is impossible without a structured output contract.
- Optimization, caching, and pushdown cannot happen without an IR.

### Option B: Build a traditional BI / data warehouse integration

Export observability data to a data warehouse (e.g., BigQuery, Snowflake) and build semantic
layers and BI dashboards there.

Rejected because: this recreates the exact cost and delivery problem that motivates this ADR.
ETL pipelines, semantic layer modeling, and BI governance take months per question domain. The
observability store already holds the data; duplicating it into a warehouse adds cost and latency
with no benefit for operational questions.

### Option C: AI-only query layer (replace structured query UI)

Replace the structured query API and UI with LLM-only interaction.

Rejected per ADR-014: "human-readable and deterministic query results are essential for production
observability." The NL query layer is additive — it sits alongside the structured query surfaces,
not in place of them.

### Option D: Adopt PromQL as the primary query language

Use PromQL as the unified operator query UX, with a translation layer to SQL.

Rejected: PromQL is time-series and metrics-only; it cannot express log search, trace filtering,
or cross-signal joins. See [ADR-026](ADR-026-no-proprietary-query-dsl.md).

## Related

- [spec/08-ai-ml.md §13.1](../08-ai-ml.md) — NL query spec, MCP architecture, time-series semantics
- [spec/03-storage.md §5.4.1](../03-storage.md) — Schema Registry semantic annotations (prerequisite)
- [ADR-027](ADR-027-local-llm-backend.md) — Local LLM backend (vLLM); extends the `LlmCaller` trait with a vLLM caller
- [ADR-026](ADR-026-no-proprietary-query-dsl.md) — No proprietary query DSL
- [ADR-016](ADR-016-grafana-visualization-strategy.md) — Grafana visualization; VisualizationFrame maps to `@grafana/ui` PanelData
- [ADR-014](ADR-014-ai-feature-boundaries.md) — AI Feature Boundaries (advisory-only, read-only, provenance required)
- [ADR-013](ADR-013-schema-governance.md) — Schema Governance (Schema Registry)
- [ADR-005](ADR-005-arrow-datafusion.md) — Arrow/DataFusion Query Layer; SQL as IR
- [spec/00-market-analysis.md §2.2 Gap 6](../00-market-analysis.md) — Market positioning rationale
