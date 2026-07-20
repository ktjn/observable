# ADR-001: OpenTelemetry as External Contract

**Date:** 2026-04-15
**Updated:** 2026-04-26
**Status:** Accepted
**Authors:** Gemini CLI, Engineering
**Deciders:** Project Stakeholders
**Review date:** 2027-04-26

## Context

The platform needs a stable, industry-standard interface for telemetry ingestion to ensure interoperability with existing tools, SDKs, and collectors. Inventing a custom ingestion protocol would create high friction for user onboarding and increase maintenance overhead.

Since the initial decision, additional questions have emerged that this ADR now also addresses:

- Which _version_ of the OTLP specification is supported (floor and target)?
- When should the platform update its internal OTEL Rust crate versions?
- How are new OTLP spec signals and fields adopted?

**Key insight — client compatibility is independent of internal crate version:**
The ingest gateway is a _receiver_. Proto3 wire format is backward-compatible: a gateway
compiled against newer OTLP proto types correctly parses payloads from any prior OTLP spec
version. Upgrading internal crates does not break existing clients.

## Decision

1. **Protocol:** The platform uses **OpenTelemetry (OTel)** as its primary ingestion and semantic model boundary. All internal data structures for traces, metrics, logs, and profiles are designed around OTel's specifications. OTLP is the only supported wire protocol for telemetry signals; no proprietary wire protocol is introduced.

2. **Minimum supported OTLP spec version: 1.0.0.** Any OTLP-conformant client SDK or collector speaking OTLP/gRPC or OTLP/HTTP is supported. No minimum SDK version is enforced at the wire level.

3. **Target OTLP spec version: latest stable (currently 1.10.0).** The platform tracks the latest stable OTLP specification. Fields introduced in newer spec versions are stored when present and silently ignored when absent; absence is never an error.

4. **Internal Rust crate update cadence:** OTEL-family Rust crates (`opentelemetry`, `opentelemetry-sdk`, `opentelemetry-otlp`, `opentelemetry-proto`, `opentelemetry-semantic-conventions`) must be updated to the latest stable release at least once per quarter, or within 30 days of a security advisory (matching `spec/10-process.md §16.10`). Major version bumps are treated as a feature slice: breaking API changes must be resolved before the PR is merged.

5. **Proto files for smoke testing:** `proto/otlp/` contains OTLP `.proto` definitions
   used exclusively by `grpcurl` in the smoke test (`tests/e2e/smoke_test.sh`) to send
   gRPC test payloads. These files are not compiled into any application binary —
   gRPC deserialization is handled by the `opentelemetry-proto` Rust crate. The proto files
   must be kept in sync with the target OTLP spec version as part of quarterly crate updates.

6. **Proto-format quirk documentation:** Comments that document SDK-version-specific serialization quirks must describe the _behavior_ observed, not pin a specific SDK version, since older and newer clients coexist on the wire simultaneously.

## Consequences

**Easier:**

- Direct compatibility with OTel SDKs and OTel Collector.
- Lower barrier to entry for users already using OTel.
- Standardized semantic conventions for metadata.
- Clients using any OTel SDK from spec 1.0.0 forward are supported without special-casing.
- A clear upgrade cadence removes ambiguity about when to bump crate versions.
- New OTLP signals (e.g., Events, Profiles) are adopted as they stabilize without requiring a new ADR.

**Harder:**

- Strict adherence to OTel's evolving specification.
- Potential performance overhead for high-throughput normalization.
- Quarterly crate upgrades require resolving breaking API changes, especially after large gaps in upgrade cadence.

**Constrained:**

- The internal data model will be primarily OTel-centric, making it more difficult to support legacy non-OTel formats without translation.
- Fields present in OTLP spec 1.0.0 must continue to be accepted; they cannot be dropped without a deprecation cycle.
- The platform cannot advertise support for a new OTLP signal until the ingest gateway and storage-writer handle it end-to-end.

## Alternatives Considered

### Option A: Custom Ingestion Protocol

Rejected due to high user friction and lack of ecosystem support.

### Option B: Vendor-Specific Ingestion (e.g., Datadog, Prometheus)

Rejected to maintain vendor neutrality and broad ecosystem compatibility. OTel already supports these via exporters/receivers.

### Option C: Pin to a fixed OTLP spec version

Rejected. Pinning prevents the platform from supporting new spec signals without another ADR change and requires clients to avoid new spec features.

### Option D: No version policy — upgrade ad hoc

Rejected. The ad-hoc approach produced stale internal crates (5 minor versions behind), a prost version-mismatch workaround, tonic version inconsistency between the mediator binary and its build template, and version-pinned quirk comments in source code.

## Related

- `spec/01-overview.md` (Product Principles: OpenTelemetry first)
- `spec/02-architecture.md` (Ingestion)
- `spec/06-agents.md` (Agent and Collector Strategy)
- `spec/14-domain-model.md` (OTel-aligned telemetry schemas and cross-signal joins)
- [ADR-023](ADR-023-standard-otlp-ports.md) — Standard OTLP port conformance
- `spec/10-process.md §16.10` — Dependency maintenance policy
