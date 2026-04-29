# ADR-001: Independent Compiled-Mediator Architecture

**Date:** 2026-04-19
**Status:** Accepted
**Authors:** Engineering
**Deciders:** Project Stakeholders
**Review date:** 2026-10-19

## Context

Legacy log and metric sources — syslog emitters, log4j2 appenders, MQTT brokers,
Kafka topics, managed cloud services, file tails — cannot emit OTLP natively.
Three approaches exist to bridge the gap:

**Option A — Require adoption of the OTel SDK**
Valid for greenfield applications. Not feasible for legacy apps without source
access, third-party software, or managed cloud services.

**Option B — Use an existing edge collector (Fluent Bit or OTel Collector)**
- *Fluent Bit:* Small binary (~2–5 MB). Its OTLP output plugin requires the user
  to manually decide what becomes a resource attribute, log attribute, body,
  severity, and trace context. No live preview; errors manifest silently at
  query time.
- *OTel Collector:* Broad ecosystem, but 50–150 MB binary and 100–300 MB RAM under
  load — prohibitive for dense environments. The same OTLP mapping ambiguity
  problem applies.

**Option C — Independent compiled-mediator (Collectable)**
A standalone tool with no runtime coupling to any observability backend. Each
mediator is a compiled Rust binary — not an interpreted pipeline config — with
transport and parser logic baked in at compile time. The binary emits OTLP to
any OTLP-compatible endpoint.

## Decision

Build Collectable as an independent compiled-mediator tool (Option C).

Collectable consists of:
1. A **parser development UI** — web-based, interactive, with live preview against
   sample data and guided OTLP field mapping.
2. A **build service** — receives a pipeline definition, generates Rust source from
   templates, and cross-compiles for the requested target ABI.
3. A **mediator runtime library** — composable Rust transport and parser modules
   assembled into a single static binary.

The tool runs via `docker compose up`. No account or cloud connection is required.
The OTLP output endpoint is runtime configuration, not hardwired to any backend.

## Consequences

**Easier:**
- Small binary footprint (Rust + musl static ≈ 5–15 MB), matching Fluent Bit.
- Guided UI eliminates OTLP mapping ambiguity.
- Mediators are auditable — users can inspect the generated Rust source.
- Useful with any OTLP backend, not just Observable.
- Adding a new source format is isolated to Collectable; no other service is touched.

**Harder:**
- Significant new build surface: React UI, Rust build service, mediator library,
  and a cross-compilation pipeline.
- Cross-compilation toolchains (musl, mingw, aarch64) must be maintained in the
  build service container.
- The mediator template library must be kept current as Rust crates evolve.

**Constrained:**
- The build service must never execute user-supplied code. User input flows only
  through a strictly validated pipeline definition schema before reaching codegen.
  Strict input validation is a permanent constraint, not a nice-to-have.

## Alternatives Considered

### Add native format receivers to an observability backend's ingest gateway
Rejected. Compounds transport + parser complexity in the ingest hot path and
makes the ingest surface hard to maintain. Violates the OTLP-only external
contract principle.

### Ship an OTel Collector distribution only
Partially useful. Addresses sources reachable by a Collector agent, but the
footprint is prohibitive for dense environments and the OTLP mapping problem
remains unsolved for most users.

### Fork Fluent Bit and extend with better OTLP mapping
Rejected. Maintaining a C codebase fork is high ongoing cost. The OTLP mapping
problem is better solved by a guided UI and compiled output than by patching
Fluent Bit's config model.

## Related

- [ADR-002](ADR-002-mediator-workspace-isolation.md) *(planned)* — Rust workspace isolation
- `mediator/` — Mediator runtime library (standalone Rust workspace)
- `builder/` — Build service and parser development UI
