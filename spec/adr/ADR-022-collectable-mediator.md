# ADR-022: Collectable — Independent Compiled-Mediator Tool for Legacy Source Ingestion

**Date:** 2026-04-19
**Status:** Accepted
**Authors:** Engineering
**Deciders:** Project Stakeholders
**Review date:** 2026-10-19

---

## Context

Observable's ingest gateway accepts OTLP only (ADR-001). This is an intentional
offloading strategy: parsing and transport complexity belong at the edge, not in
the backend. However, 80%+ of enterprise log volume originates from sources that
cannot emit OTLP natively — legacy applications, syslog emitters, log4j2 appenders,
MQTT brokers, managed cloud services, and unstructured text files.

Three options exist to bridge this gap:

**Option A — Require clients to adopt OTel SDK**
Valid for greenfield applications. Not feasible for legacy apps without source
access, third-party software, or managed cloud services.

**Option B — Client-side mediator using existing tools (Fluent Bit or OTel Collector)**
Both tools can receive native formats and forward via OTLP in principle.

- **Fluent Bit:** Small footprint (2–5 MB binary, 5–10 MB RAM). Written in C.
  Its OTLP output plugin is extremely difficult to configure correctly: the user
  must manually decide what becomes a resource attribute, a log attribute, the
  body, severity, and trace context fields. There is no guidance and errors
  produce silently malformed data in the backend. Pipeline debugging is hard —
  there is no live preview and errors often only manifest at query time.

- **OTel Collector:** Excellent composability (receiver/processor/exporter model),
  broad ecosystem. However its footprint is 50–150 MB binary and 100–300 MB RAM
  under load — too heavy for dense Kubernetes node deployments or edge environments.
  Its YAML pipeline config has the same OTLP mapping ambiguity problem as Fluent Bit.

**Option C — Server-side translation layer in the Observable ingest gateway**
Adding native format receivers to the Observable ingest gateway couples two
orthogonal concerns: for every new source format, Observable must implement a new
transport *and* a new parser *and* test their interaction with the ingest pipeline.
Complexity compounds rather than accumulates. This also violates ADR-001 (OTLP as
the external contract) and creates a surface that is difficult to keep consistent
across Observable releases.

**Variant of Option C — Independent compiled-mediator tool (Collectable)**
A standalone tool that lives in the Observable repository but has no runtime
coupling to Observable. Each mediator is a compiled Rust binary — not an
interpreted pipeline config — with transport and parser logic baked in at
compile time. The binary emits OTLP to any OTLP endpoint. Observable is not
involved in mediator compilation, distribution, or operation.

---

## Decision

Build **Collectable** as an independent compiled-mediator tool (Option C variant).

Collectable consists of:
1. A **parser development UI** — web-based, interactive, with live preview against
   sample data and guided OTLP field mapping.
2. A **build service** — receives a pipeline definition, generates Rust source from
   templates, and cross-compiles for the requested target ABI.
3. A **mediator runtime library** — composable Rust transport and parser modules
   that the code generator assembles into a single-binary executable.

The tool runs via `docker compose up`. No Observable account or connection is
required to build mediators. The OTLP output endpoint is configuration, not
hardwired to Observable.

---

## Consequences

**Positive:**
- Observable's ingest gateway surface stays minimal and uniform (OTLP only).
  ADR-001 is reinforced, not compromised.
- The compiled binary is small (Rust + musl static = ~5–15 MB), matching
  Fluent Bit's footprint, with no C library dependencies.
- The guided UI eliminates the OTLP mapping ambiguity that makes Fluent Bit
  and the OTel Collector painful to use for legacy sources.
- Mediators are auditable — users can download and inspect the generated Rust
  source before running the binary.
- Collectable is useful to any OTLP backend, not just Observable. This increases
  adoption surface and positions Collectable as a potential open-source project.
- Transport and parser complexity is isolated in Collectable; adding a new source
  format does not touch Observable services.

**Negative / risks:**
- Collectable is a significant new build surface: a React UI, a Rust build service,
  a mediator library, and a cross-compilation pipeline.
- The build service must handle cross-compilation securely — user-defined patterns
  become part of generated code. Strict input validation is required; users must
  never be able to inject arbitrary Rust code via pipeline definitions.
- Cross-compilation toolchains (musl, mingw, aarch64) must be maintained in the
  build service container.
- Observable must maintain the mediator template library as Rust crates evolve.

**Neutral:**
- Collectable does not replace the OTel Collector distribution (Tier 2 gap,
  spec/06-agents.md §10.1). The Collector remains the recommended path for
  sources that already have an OTel SDK or Collector-compatible receiver.
  Collectable addresses the subset of sources where the Collector's footprint or
  OTLP mapping complexity is a barrier.

---

## Implementation Notes

- Collectable has been extracted to its own repository,
  [github.com/ktjn/collectable](https://github.com/ktjn/collectable), reflecting its
  status as a generic tool with no runtime coupling to Observable. It is no longer
  part of this repository's source tree.
- `collectable/mediator/` was a **standalone Rust workspace** and was never added
  to the root `Cargo.toml` workspace members, even before extraction.
- MQTT transport uses `rumqttc` (pure Rust, musl-compatible). `paho-mqtt` is
  excluded because it links to a C library, which breaks static musl builds.
- The pipeline definition schema is versioned (`"version": "1"`) from day one.
- The build service **never executes user-supplied code** — it renders templates
  from a pipeline definition that is fully parsed and validated before codegen.

---

## Alternatives Considered

### Add native format receivers to the Observable ingest gateway
Rejected. Compounds transport + parser complexity in the ingest hot path, violates
ADR-001, and makes the ingest gateway surface hard to maintain.

### Ship an Observable OTel Collector distribution only
Partially useful. Addresses sources that are reachable by a Collector agent, but
the footprint is prohibitive for dense environments and the OTLP mapping problem
remains unsolved for users without deep Collector expertise.

### Fork Fluent Bit and extend with better OTLP mapping
Rejected. Maintaining a C codebase fork is high ongoing cost. The OTLP mapping
problem is better solved by a different approach (guided UI + compiled output)
rather than patching Fluent Bit's config model.

---

## Related

- [ADR-001](ADR-001-otel-external-contract.md) — OTel as the external contract
- [ADR-004](ADR-004-rust-data-plane.md) — Rust for data plane components
- [spec/16-collectable.md](../16-collectable.md) — Full Collectable specification
- [spec/06-agents.md §10.1](../06-agents.md) — Agent and collector components
