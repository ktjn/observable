# ADR-042: Orthogonal Operational Domain Boundaries

**Date:** 2026-11-07  
**Status:** Accepted for the 2.0 specification baseline (2026-11-07; revised in the 2.0 re-review)  
**Authors:** Tommy (ktjn), Claude Code  
**Deciders:** Tommy (ktjn)  
**Superseded by:**  
**Review date:** 2026-12-07  

## Context

The existing system had implementation layers but no explicit system domains. A large codebase needs domain dependency rules orthogonal to deployment and layering. The business requirement areas are not necessarily software components or implementation domains.

The first version of this ADR deferred domain placement of the planned components `observable-catalog`, `observable-health` and `observable-response`. That left a latent cycle: `investigation` already uses `incident-response` (observable-query requires `evaluate-alerts`), so placing health in `investigation`, or letting response read through `investigation`, fails DOM-03 on the first Phase 1/2 change.

## Decision

Declare ten implementation domains in `spec/system.yaml`, including the domains of planned components, so placement is reviewed before code exists:

| Domain | Component | May use |
|---|---|---|
| operational-experience | observable-web | investigation, service-model, service-assessment, service-control, incident-response, identity |
| ingestion | observable-ingest | identity, processing, service-control |
| investigation | observable-query | identity, service-model, service-assessment, service-control, incident-response, telemetry-storage |
| incident-response | observable-alerting; planned observable-response | identity, service-model, service-assessment, telemetry-storage |
| service-assessment | planned observable-health | identity, service-model, telemetry-storage |
| service-model | planned observable-catalog | identity, service-control, telemetry-storage |
| service-control | observable-control | identity, telemetry-storage |
| processing | observable-process | telemetry-storage |
| identity | observable-auth | — |
| telemetry-storage | observable-store-clickhouse | — |

Rules:

- `incident-response` never uses `investigation`. Response and alerting read telemetry through `telemetry-storage` contracts and service state through `service-model` and `service-assessment`.
- `service-model` reads observed service identities from `telemetry-storage` (for unregistered-service detection, OBS-02), not through `investigation`.
- `assess-customer-experience` and `track-business-processes` get their provider and domain in the Phase 7 detailed plan (ADR-039).
- Business capabilities and UX experiences may span any number of domains; domains do not duplicate business requirements. Domain changes follow the system-specification compatibility and impact process.

## Consequences

- Architectural coupling is expressible and reviewable at the metamodel level, and the Phase 1/2 components start inside an acyclic graph.
- Declarations alone do not prove runtime or source-code isolation; check-drift, provider/consumer contracts and tests remain separate controls.
- Do not create components merely to match a domain or business requirement area.
- Splitting incident or health functionality later requires updating `uses` and bounded-context handoffs.

## Alternatives Considered

### Option A: Keep unbounded component relationships
Insufficient control as the system grows. Rejected.

### Option B: Align domains to technical layers or all 37 business areas
Confuses orthogonal classification and would incentivize artificial boundaries. Rejected.

### Option C: Place health in `investigation` and invert the query → alerts edge later
Smallest declaration now, but forces a cycle or an unplanned inversion in Phase 2. Rejected.

## Related

- [docs/plans/phase0-assurance-hardening.md](../../docs/plans/phase0-assurance-hardening.md) section 5
- [ADR-038](ADR-038-service-catalog-component.md), [ADR-039](ADR-039-health-assessment-capability.md), [ADR-040](ADR-040-incident-response-ownership.md)
