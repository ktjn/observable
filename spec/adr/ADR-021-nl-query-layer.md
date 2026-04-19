# ADR-021: LLM Natural Language Query Layer

**Date:** 2026-04-19
**Status:** Proposed
**Authors:** ktjn
**Deciders:** Project Stakeholders
**Review date:** 2026-04-19

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

Observable will provide a natural language query interface as a Phase 2 AI capability, built as
an LLM reasoning layer on top of the existing query API and schema infrastructure.

**The approach:**
1. The LLM classifies the question into signal types and time ranges.
2. It generates SQL/DataFusion expressions grounded by Schema Registry semantic annotations
   (`display_name`, `business_description`, `interpretation_rule`, `effective_sample_rate`).
3. Queries execute through the existing query API under the caller's tenant context and RBAC
   permissions — the LLM does not bypass authorization.
4. Where the question admits multiple signal types, the LLM applies **cross-signal triangulation**:
   parallel sub-queries per signal, results compared for convergence or divergence, confidence
   reported accordingly.
5. Every response includes a provenance payload: raw queries issued, signals consulted, effective
   sample rate per signal, and an explicit approximation statement.

**Boundary:** This capability targets operational and tactical questions. It explicitly does not
target regulatory compliance, financial reconciliation, or contractual SLA evidence. The LLM must
decline to answer questions in these categories and explain why.

**Prerequisite:** The Schema Registry semantic annotations layer (see [spec/03 §5.4.1](../03-storage.md))
must be available before the NL query layer can produce reliable results. Without business-meaning
annotations, the LLM must guess at field semantics, producing unreliable query generation.

## Consequences

**Easier:**
- Non-engineers (product managers, finance, support) can query operational data without SQL
  knowledge or analyst intermediation.
- Questions that would never be answered in a BI pipeline get answered in seconds.
- The platform differentiates against traditional observability tools that offer only
  structured query UIs.

**Harder:**
- Users must be educated about the approximation bounds of answers — sample rates, dropped
  events, clock skew — to avoid misusing approximate results in inappropriate contexts.
- The Schema Registry semantic annotations layer requires ongoing operator maintenance to stay
  accurate as instrumentation evolves.
- LLM query generation quality depends on schema annotation completeness; ungrannotated fields
  produce lower-quality queries.

**Constrained:**
- The NL query layer must not be used as a path to bypass RBAC or tenant isolation.
- Answers must always carry a provenance payload and an approximation statement — this is not
  optional.
- Natural language query is advisory output. It must not feed automated alert evaluation,
  billing, or SLA enforcement.

## Alternatives Considered

### Option A: Build a traditional BI / data warehouse integration
Export observability data to a data warehouse (e.g., BigQuery, Snowflake) and build semantic
layers and BI dashboards there.

Rejected because: this recreates the exact cost and delivery problem that motivates this ADR.
ETL pipelines, semantic layer modeling, and BI governance take months per question domain. The
observability store already holds the data; duplicating it into a warehouse adds cost and latency
with no benefit for operational questions.

### Option B: AI-only query layer (replace structured query UI)
Replace the structured query API and UI with LLM-only interaction.

Rejected per ADR-014: "human-readable and deterministic query results are essential for production
observability." The NL query layer is additive — it sits alongside the structured query surfaces,
not in place of them. Engineers and reliability engineers will continue to use structured queries
for alerting, dashboards, and investigation workflows.

### Option C: Defer until Phase 8 (Intelligence)
Follow the "ship AI late" strategy strictly and defer NL query to Phase 8.

Reconsidered: NL query on an existing, stable query substrate is architecturally simpler than
autonomous AI agents or auto-remediation. It is advisory-only, read-only, and bounded to the
existing query API contract. Phase 2 is appropriate given that the query API (Phase 1) and
Schema Registry semantic annotations are prerequisites, not Phase 8 features.

## Related

- [spec/08-ai-ml.md §13.1](../08-ai-ml.md) — NL query spec and cross-signal triangulation
- [spec/03-storage.md §5.4.1](../03-storage.md) — Schema Registry semantic annotations (prerequisite)
- [ADR-014](ADR-014-ai-feature-boundaries.md) — AI Feature Boundaries (still active; this ADR adds a use case within its constraints)
- [ADR-013](ADR-013-schema-governance.md) — Schema Governance (Schema Registry)
- [ADR-005](ADR-005-arrow-datafusion.md) — Arrow/DataFusion Query Layer
- [spec/00-market-analysis.md §2.2 Gap 6](../00-market-analysis.md) — Market positioning rationale
