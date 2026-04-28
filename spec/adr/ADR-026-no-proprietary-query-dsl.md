# ADR-026: No Proprietary Query DSL

**Date:** 2026-04-28
**Status:** Accepted
**Authors:** ktjn
**Deciders:** Project Stakeholders
**Review date:** 2026-10-28

## Context

Every major observability platform that predates the LLM era built a proprietary query language:
Splunk SPL, Datadog DQL, Dynatrace DQL, Elastic KQL/ES|QL, New Relic NRQL, Honeycomb BubbleUp.
Each language is a deliberate lock-in mechanism: operators who invest years learning and building
around a DSL face high switching costs.

Observable is designed from inception as an open-standards platform (ADR-001, ADR-002, ADR-003).
The query layer is no different. A proprietary DSL would:

- contradict the open-standards positioning in every sales conversation
- require a custom parser, custom AST, and custom execution engine — all high-maintenance surfaces
- fail to interoperate with SQL tooling, Arrow/DataFusion connectors, or downstream data systems
- duplicate years of SQL engine maturity that DataFusion already provides
- become legacy the moment NLQ is the primary operator UX

Observable's execution substrate (Arrow/DataFusion, ClickHouse SQL) is already a mature, standard,
interoperable query IR. Introducing a DSL on top would be a layer that generates value for no
user and maintenance burden for every engineer.

## Decision

**Observable will not introduce a proprietary query DSL.**

This is a permanent, first-principles constraint — not a deferral. Specifically:

- No custom query language syntax
- No custom parser or lexer
- No custom AST
- No custom execution engine
- No "Observable Query Language" or equivalent

**SQL is the canonical query IR.** All query surfaces — natural language, UI filter panels, faceted
search, PromQL compatibility, trace explorer, log explorer, metric explorer — compile down to SQL
(ClickHouse SQL transitionally; DataFusion SQL as the target; see ADR-005).

**NLQ is the primary operator UX** (see ADR-021). Natural language replaces the DSL as the
human-facing query interface. It is not a language — it is a compiler front-end that emits a
structured IR, which an MCP translation layer compiles to SQL.

**PromQL compatibility is optional and metrics-only.** If implemented, it is a thin parser façade
inside the MCP translation layer that emits the same structured NLQ IR as natural language input.
It does not introduce a new query engine or execution model. See ADR-021 §PromQL Façade.

## Consequences

**Easier:**
- Interoperability: queries are portable SQL; operators can run them outside Observable (DuckDB,
  ClickHouse CLI, DataFusion REPL).
- Lower onboarding cost: operators who know SQL need zero Observable-specific query training.
- No maintenance surface for a parser, syntax version compatibility, or language migration.
- Cross-signal joins work natively in SQL/DataFusion — this is impossible in PromQL, SPL, or DQL.
- NLQ and SQL can evolve independently; neither depends on a proprietary parser.

**Harder:**
- Observable cannot use query language lock-in as a competitive moat.
- Some observability-specific time-series idioms (rate, irate, range vectors) require verbose SQL
  patterns; these are encapsulated in the MCP translation layer's SQL template library.

**Constrained:**
- Any future query surface proposal must compile to SQL. If a proposed feature cannot be expressed
  in SQL/DataFusion, it must be implemented as a DataFusion custom operator (see spec/03 §6.2),
  not as a new language primitive.
- Extension points (`spec/09-api.md §Extension Points`) may expose "query macros/functions" but
  these are SQL UDFs or DataFusion custom functions — not a separate language.

## Alternatives Considered

### Option A: Build a pipe/filter query language (Splunk/KQL-style)

A pipe-based language (`source | filter x=y | stats count()`) is familiar to operators coming from
Splunk or Azure Monitor.

Rejected because:
- Reproduces the lock-in problem this ADR exists to avoid.
- Requires parser, syntax evolution, and documentation investment that produces no execution value.
- Cross-signal joins are structurally impossible in pipe/filter semantics.
- NLQ makes the need for "a simple query language" irrelevant for casual users; SQL covers power
  users.

### Option B: Adopt PromQL as the unified query language

PromQL is a widely-known time-series language. Adopting it would reduce onboarding friction for
operators from Prometheus backgrounds.

Rejected because:
- PromQL is strictly time-series and metrics-only; it cannot express log search, trace filtering,
  or cross-signal joins.
- PromQL has no joins, no arbitrary predicates, and no general-purpose aggregation.
- It would require a full PromQL execution engine or a translation layer — the translation layer
  is the correct approach (see ADR-021) and does not require adopting PromQL as the primary UX.

### Option C: Expose raw SQL as the primary query UX

Expose a SQL query editor to all users as the primary interface.

Rejected as a *primary* UX because:
- Non-engineer operators (product managers, support, finance) cannot write SQL.
- The correct answer for non-engineers is NLQ (ADR-021), not SQL education.
- SQL remains available as a power-user surface in the Query Workbench (spec/05 §9.3 Phase 2–3).

## Related

- [ADR-001](ADR-001-otel-external-contract.md) — open standards at the ingest layer
- [ADR-005](ADR-005-arrow-datafusion.md) — Arrow/DataFusion as query execution substrate; SQL as IR
- [ADR-021](ADR-021-nl-query-layer.md) — NLQ layer architecture; MCP IR translation; VisualizationFrame
- [spec/03-storage.md §6](../03-storage.md) — DataFusion custom operators (the extension model for
  observability-specific logic)
- [spec/09-api.md §Extension Points](../09-api.md) — query macros/functions as SQL UDFs, not a DSL
- [spec/13-risks-roadmap.md §Risk 5](../13-risks-roadmap.md) — "Custom query DSL too early" risk
  (now resolved by this ADR)
- [spec/00-market-analysis.md §2.2](../00-market-analysis.md) — market positioning and lock-in analysis
