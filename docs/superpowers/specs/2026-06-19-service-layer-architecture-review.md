# Service Layer Architecture Review

**Date:** 2026-06-19
**Status:** Informational — findings feed Deferred-tier backlog items in
`docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md` §7. Not an implementation plan.

## Scope

Review of internal layering and inter-service boundaries across the six Rust services
(`ingest-gateway`, `query-api`, `storage-writer`, `stream-processor`, `alert-evaluator`,
`auth-service`) plus their communication topology. Excludes the frontend and testbench services.

## Current Topology

- **Data plane (queue-decoupled, healthy):** `ingest-gateway` → Redpanda (`telemetry.raw`) →
  `stream-processor` → `storage-writer` → ClickHouse. OTLP is the external contract (ADR-001);
  Redpanda is the resilience backbone (ADR-009).
- **Control plane (tightly coupled via shared PostgreSQL):** every service reads/writes
  PostgreSQL directly for its own control-plane state (API keys, schemas, alert rules, sessions)
  with no schema versioning that spans services.
- **Read path:** `query-api` is the sole façade for ClickHouse/PostgreSQL reads consumed by the
  frontend and `alert-evaluator`.

## Findings

### 1. query-api is an oversized service

`query-api` is ~15.4K LoC covering traces, logs, metrics, discovery/topology, dashboards, alerts,
incidents, SLOs, schemas, admin members, notifications, and NLQ/AI (`llm_adapter.rs`, 2.8K lines).
All domains share one `AppState` (ClickHouse + PostgreSQL + LLM client), and each handler
re-derives tenant/auth context independently.

**Risk:** every change to any one domain (e.g., adding a notification channel) requires
rebuilding and redeploying the same binary that serves trace/log/metric reads — the platform's
highest-traffic, latency-sensitive path. The NLQ/AI subsystem in particular couples an LLM
provider dependency to that same blast radius.

**Recommendation:** extract NLQ/AI into its own service (or at minimum an independently
versioned crate with a narrow trait boundary) first, since ADR-014/ADR-021 already treat it as
advisory-only and optional — it's the cleanest cut line. Dashboards/alerts/incidents are
candidates for a second extraction once the NLQ cut proves the pattern, but don't do both at once.

### 2. No repository/data-access abstraction

Every handler across every service builds and runs SQL inline against `state.ch` (ClickHouse) or
`state.db` (PostgreSQL) — there is no shared layer that owns query construction, row mapping, or
tenant-scoping enforcement. This is consistent *within* a service (e.g., `query-api`'s handlers
all do it the same way) but means tenant-id binding, error mapping, and row-to-domain conversion
are each reimplemented per handler.

**Risk:** the multi-tenant isolation guarantee (ADR-007) lives in handler-author discipline, not
in a single enforced choke point — a missed `WHERE tenant_id = ?` in a new handler is a tenant
data leak, not a compile error.

**Recommendation:** introduce a thin repository module per service (not a generic cross-service
ORM) that wraps ClickHouse/PostgreSQL access and bakes in tenant scoping at the type level (e.g.,
a `TenantScopedQuery` builder that cannot be constructed without a tenant id). Apply to
`query-api` first since it has the most handlers and the most tenant-scoped data.

### 3. stream-processor → storage-writer is a synchronous HTTP coupling

Unlike `ingest-gateway` → `stream-processor` (queue-based via Redpanda), `stream-processor`
forwards normalized batches to `storage-writer` via a blocking HTTP POST.

**This is an ADR-009 compliance gap, not just a risk.** ADR-009 (Queue/Stream Backbone, Accepted)
states "all incoming...data will be written to the queue before being consumed by downstream
processors (storage writers, enrichment engines, alert evaluators)" — `storage writers` is named
explicitly. The current synchronous HTTP handoff means `storage-writer` is not, in fact, consuming
via the queue, contradicting an already-accepted decision rather than merely falling short of a
stylistic ideal. If `storage-writer` is slow or unavailable, `stream-processor` blocks, backing up
Redpanda consumer lag — the failure mode ADR-009 was adopted specifically to prevent.

**Recommendation:** route `stream-processor` → `storage-writer` through a second Redpanda topic
(or reuse `telemetry.raw` with a post-normalization marker) instead of HTTP, matching the
established pattern. `storage-writer` already buffers internally, so this mainly changes the
producer side.

### 4. alert-evaluator conflates four concerns in one loop

The evaluator's worker loop fetches rules from PostgreSQL, queries ClickHouse for metrics,
evaluates threshold/SLO-burn/composite/deadman logic, and writes incidents/notifications — all
inline in one pass with no boundary between rule sourcing and evaluation.

**Risk:** changing how rules are sourced (e.g., the planned Prometheus Alert Rule Importer in
roadmap Tier 2) or adding a new rule type touches the same code path as the query/evaluate/write
logic, increasing regression risk on a system that pages humans.

**Recommendation:** split into a `RuleSource` (PostgreSQL fetch + the future Prometheus importer
path) and an `Evaluator` (pure function: rules + metric data → incidents), with the worker loop
reduced to orchestration. This is a internal refactor, not a new service.

### 5. Duplicate domain types across service boundaries

`alert-evaluator::evaluator::ThresholdOperator`/`ThresholdCondition` and similar types are
redefined rather than shared with `query-api`'s `alerts.rs`, relying on JSON serde round-tripping
to stay in sync instead of a single source of truth.

**Risk:** a field rename or enum variant added on one side silently fails to round-trip instead of
failing to compile.

**Recommendation (corrected — see note below):** these are canonical domain/wire types, so per
ADR-032 they belong in `models/alerts.mdl` (which already exists for `AlertRule`/`Firing`), not a
hand-written shared crate — extend that `.mdl` file with the threshold/condition types and
generate both services' Rust bindings from it, rather than introducing a parallel
`observable-alert-types` crate that would itself become a second source of truth ADR-032 was
adopted to eliminate.

**Caveat:** ADR-032's Known Limitations list `enum(...)` always emitting Rust `String` rather than
a real enum as an open emitter gap (Phase 1 backlog item 3). Until that's fixed,
`ThresholdOperator` generated via Modelable would round-trip as a string with the same
no-compile-time-safety property it has today — the fix removes the *duplication*, not the
*type-safety gap*, until the emitter gap closes. Worth doing anyway since duplication is the more
immediate correctness risk; flag the enum gap as a dependency, not a blocker.

### 6. No shared error-handling crate

Each service maps its internal errors to HTTP responses independently — there's no common
problem+json shape, status-code mapping convention, or tracing/error-context integration shared
across services. Originally noted under Non-findings as awareness-only (fail-open patterns in
`ingest-gateway`, swallowed errors in `query-api` facet aggregation), but the underlying gap —
no shared error-handling crate — is itself a low-risk, low-coupling improvement worth promoting,
distinct from the per-service data-access work in Finding 2.

**Risk:** inconsistent error shapes/status codes across services make client-side (frontend,
external API consumers) error handling harder, and each service re-solves the same
error-to-response mapping problem.

**Recommendation:** extract a shared `observable-error` crate (HTTP error mapping, problem+json
shape, tracing integration) consumed by all services — pure utility code with no tenant-scoping
or domain concerns, so it carries none of the leaky-abstraction risk that a shared *data-access*
crate would (see Finding 2, which deliberately recommends per-service repository modules instead
of a cross-service one).

### 7. Duplicated /readyz and /metrics scaffolding

`auth-service/src/observability.rs`, `storage-writer/src/observability.rs`, and
`query-api/src/observability.rs` each define a near-byte-identical Prometheus `Registry` +
`HistogramVec`/`IntCounterVec` setup (same `linear_buckets(0.005, 0.005, 20)`, same
`["method", "status"]` labels) and an HTTP-metrics-recording middleware — only the service-name
prefix differs (~270 lines total). `/readyz` handlers across `ingest-gateway`, `storage-writer`,
`alert-evaluator`, `query-api`, and `stream-processor` independently reimplement the same
Postgres/ClickHouse-ping-and-log-warning pattern (`stream-processor`'s Redpanda broker-metadata
check is the one genuinely different variant).

**Risk:** low individually, but six copies of the same dashboard-relevant metrics scaffolding mean
a fix to one (e.g., a bucket boundary correction) doesn't propagate, and new services start from
copy-paste rather than a tested primitive.

**Recommendation:** extract an `observable-observability` crate (new workspace member) providing
a generic `HttpMetricsCollector` (builder for the shared Prometheus registry/histogram pattern)
and a `ReadyzProbe` enum (`Postgres`, `ClickHouse`, `Redpanda` variants) that each service composes
for its own dependency set. Medium effort (existing six call sites need a mechanical swap), low
risk since each call site's external behavior (the `/metrics` and `/readyz` response shape)
doesn't change.

### 8. Duplicated ClickHouse/PostgreSQL client construction

`storage-writer`, `alert-evaluator`, and `query-api` each build a ClickHouse client with the same
`Client::default().with_url(...).with_user(...).with_password(...).with_database("observable")`
call reading `CLICKHOUSE_URL`/`CLICKHOUSE_USER`/`CLICKHOUSE_PASSWORD` with identical defaults
(~24 lines duplicated across 3-4 call sites); `ingest-gateway`, `auth-service`, `alert-evaluator`,
and `query-api` each build a `PgPool` from `DATABASE_URL` with the same connect/max-connections
pattern (~16 lines duplicated).

**Risk:** low — small, mechanical duplication, but six copies of connection-string parsing is six
places a default or error-handling change has to land identically.

**Recommendation:** add `create_clickhouse_client()` and `create_postgres_pool()` to `libs/domain`
(already a shared workspace member — no new crate needed). Low effort, low risk.

### 9. Config/env-var loading pattern (standardize, don't extract)

Each service hand-parses its own env vars into ad-hoc structures scattered through `main.rs`
(`ingest-gateway` alone parses ~9 vars across `main.rs:154-207`). The configs are genuinely
service-specific, so a shared config *crate* isn't the fix — but `storage-writer/src/retention.rs`
already has the right local pattern (`RetentionConfig::from_env()` + `from_values()` for
testability) that the other services should adopt for their own config structs, for consistency
and testability rather than deduplication.

**Recommendation:** no extraction; adopt the `from_env()`/`from_values()` convention per service
opportunistically when touching that service's startup code, not as its own slice.

### 10. Admin/privilege-management surface in query-api

See `docs/superpowers/specs/2026-06-19-admin-service-extraction-design.md` and
`spec/adr/ADR-033-admin-service-extraction.md` (Proposed) — member management, API key/token
lifecycle, and platform config are privilege-granting operations sharing query-api's process
boundary with the trace/log/metric read path. Recorded as its own ADR given the weight of the
decision (new service, new trust boundary) rather than folded into this review's findings list.

## Non-findings (explicitly out of scope for action)

- Lack of a service mesh / inter-service auth (services trust Docker Compose network isolation)
  is a deliberate ADR-008/ADR-009 era tradeoff, not a new finding; not re-litigated here.

## Related ADRs

- **ADR-009** (Queue/Stream Backbone, Accepted) — Finding 3 is a compliance gap against this ADR,
  not an independent stylistic preference.
- **ADR-032** (Modelable Type-Mapping Adoption, Accepted) — Finding 5's recommendation is
  constrained by this ADR; corrected from an earlier draft that proposed a hand-written shared
  crate, which would have reintroduced the drift problem ADR-032 was adopted to solve.
- **ADR-033** (Admin Service Extraction, Proposed) — covers Finding 7 in full; this review only
  cross-references it.

## Why these stay in the Deferred tier (with one exception)

The active roadmap (`2026-06-19-unified-feature-roadmap.md`) explicitly demotes stability/
architecture work unless a concrete feature is blocked on it. Findings 2–6 and 8 (low-effort,
no feature dependency) and Finding 7 (medium effort, no feature dependency) are genuine
improvements but don't block any currently-promoted feature slice — they belong in §7 (Deferred).
Finding 9 isn't a backlog item (no extraction, just a convention to follow opportunistically).
Finding 10 (admin/privilege isolation) is recorded separately as ADR-033.

Finding 1 (query-api/NLQ coupling) is the exception: the roadmap's Tier 2 PromQL Compatibility
Façade and all of Tier 4 (Intelligence Layer) build directly on the NLQ execution path inside
`query-api`. Extracting NLQ now, before that surface area grows, is cheaper than extracting it
later — this is called out in the roadmap as a trigger-qualifying exception rather than left
purely deferred.
