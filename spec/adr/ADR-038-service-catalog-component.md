# ADR-038: Service Catalog Component

**Date:** 2026-11-07  
**Status:** Accepted (2026-11-07; decisions Q1–Q10 in docs/plans/business-spec-adoption.md, accepted by merging the 2.0 re-review PR)  
**Authors:** Claude Code  
**Deciders:** Tommy (ktjn)  
**Superseded by:**  
**Review date:** 2026-12-07  

## Context

The business specification centres on business services with accountable owners, criticality, lifecycle, outcomes and coverage obligations (OBS-01, OBS-02, OBS-27, OBS-34, OBS-36). Observable identifies a "service" only by the OTel `service.name` attribute discovered from telemetry. Every later phase (health, objectives, alerting, incidents, impact) needs the catalog.

## Decision

Add a new domain-layer component `observable-catalog` that owns the service model: business services, their mapping to telemetry `service.name`/environment, owners and support routes, criticality, lifecycle, coverage obligations and status, providers and dependencies. It provides `manage-service-catalog`, `govern-observability-assets`, `map-dependencies-and-impact` and `manage-providers`. `observable-control` keeps tenant configuration.

The web catalog reads service records, coverage and assets, and providers through those existing
runtime contracts. The catalog service applies tenant scope and existence-hiding decisions before
the web receives records or counts.

## Consequences

**Easier:** One authoritative service identity for all components; catalog state, contract and tests are isolated per ADR-035.  
**Harder:** A new component, image, schema and distribution entry; consumers must resolve `service.name` to a catalog entity.  
**Constrained:** Alert rules, SLOs, incidents, dashboards and saved views reference catalog IDs instead of free-text service names (breaking on `2.0`).

## Alternatives Considered

### Option A: Extend observable-control
Fewer moving parts, but control would mix tenant configuration with the service model and exceed its context budget. Rejected.

### Option B: Keep services implicit in telemetry
Cannot hold owners, criticality or coverage obligations. Rejected.

## Related

- [docs/plans/business-spec-adoption.md](../../docs/plans/business-spec-adoption.md) Q5
- [ADR-035](ADR-035-component-independence.md)
