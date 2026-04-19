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
separate BI pipeline.

This capability is positioned not as a replacement for a traditional BI data platform, but as a
**faster, cheaper tier for operational and tactical business questions**. Traditional BI guarantees
correctness but delivers answers slowly and at high cost. For most operational decisions, an
approximate answer at decision time has higher expected value than a precise answer after the
window has closed. Observability data is approximate by design (sampled traces, best-effort
delivery, clock skew), but cross-signal corroboration raises confidence — see §13.2.

#### Architecture

1. **Intent classification** — the LLM classifies the question into one or more signal types
   (traces, metrics, logs, events, topology) and time ranges.
2. **Query generation** — the LLM generates SQL or DataFusion query expressions against the known
   schema. The Schema Registry's semantic annotations (see [spec/03 §5.4](03-storage.md)) provide
   field descriptions, units, interpretation rules, and effective sample rates to ground the
   generated query.
3. **Execution** — queries execute through the existing query API under the caller's tenant context
   and RBAC permissions. The LLM does not bypass authorization.
4. **Cross-signal corroboration** — where the question admits multiple independent signals, the LLM
   issues parallel sub-queries per signal and reports whether the results converge or diverge
   (see §13.2).
5. **Result narration** — the LLM narrates the results in natural language, including confidence
   qualifications derived from sample rates and signal convergence.
6. **Provenance payload** — every response includes the raw queries issued, the signals consulted,
   the effective sample rate for each, and an explicit approximation statement.

#### Scope

Natural language query answers **operational and tactical** questions well:
- *"Which customers are experiencing errors in the checkout service right now?"*
- *"Did the 14:01 deploy increase p99 latency?"*
- *"Which tenant consumed the most ingest bandwidth this week?"*
- *"What changed around the time error rate spiked?"*

It is **not appropriate** for:
- regulatory compliance reporting
- financial reconciliation
- contractual SLA evidence
- any use case where auditability of data completeness is required

The LLM must state this boundary explicitly when the question falls in or near these categories.

#### Prerequisite

The Schema Registry semantic annotations layer ([spec/03 §5.4](03-storage.md)) must be in place
before the NL query layer can reason reliably. Without business-meaning annotations, the LLM
must guess at field semantics, which produces unreliable query generation.

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

### Hard Rules

- every model decision needs provenance — expose the raw queries, signals consulted, and sample rates
- no opaque auto-remediation without policy gates
- AI outputs are advisory unless explicitly approved
- every NL query response must include an explicit approximation statement
- the LLM must not bypass tenant isolation or RBAC — all queries execute under the caller's context
- the LLM must decline to answer questions requiring BI-grade correctness and explain why
