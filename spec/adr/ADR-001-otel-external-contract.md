# ADR-001: OpenTelemetry as External Contract

**Date:** 2026-04-15  
**Status:** Proposed  
**Authors:** Gemini CLI  
**Deciders:** Project Stakeholders  
**Review date:** 2026-04-15  

## Context

The platform needs a stable, industry-standard interface for telemetry ingestion to ensure interoperability with existing tools, SDKs, and collectors. Inventing a custom ingestion protocol would create high friction for user onboarding and increase maintenance overhead.

## Decision

The platform will use **OpenTelemetry (OTel)** as its primary ingestion and semantic model boundary. All internal data structures for traces, metrics, logs, and profiles will be designed around OTel's specifications.

## Consequences

**Easier:** 
- Direct compatibility with OTel SDKs and OTel Collector.
- Lower barrier to entry for users already using OTel.
- Standardized semantic conventions for metadata.

**Harder:** 
- Strict adherence to OTel's evolving specification.
- Potential performance overhead for high-throughput normalization.

**Constrained:** 
- The internal data model will be primarily OTel-centric, making it more difficult to support legacy non-OTel formats without translation.

## Alternatives Considered

### Option A: Custom Ingestion Protocol
Rejected due to high user friction and lack of ecosystem support.

### Option B: Vendor-Specific Ingestion (e.g., Datadog, Prometheus)
Rejected to maintain vendor neutrality and broad ecosystem compatibility. OTel already supports these via exporters/receivers.

## Related

- `spec/01-overview.md` (Product Principles: OpenTelemetry first)
- `spec/02-architecture.md` (Ingestion)
- `spec/06-agents.md` (Agent and Collector Strategy)
