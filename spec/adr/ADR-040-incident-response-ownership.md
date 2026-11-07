# ADR-040: Own Incident Response and Integrate External Tooling

**Date:** 2026-11-07  
**Status:** Accepted (2026-11-07; decisions Q1–Q10 in docs/plans/business-spec-adoption.md, accepted by merging the 2.0 re-review PR)  
**Authors:** Claude Code  
**Deciders:** Tommy (ktjn)  
**Superseded by:**  
**Review date:** 2026-12-07  

## Context

The specification allows incident capabilities to be satisfied by integrated incident-management tooling, and also requires exchange with incident-management, collaboration and security-operations processes (OBS-08, OBS-16, OBS-23, OBS-30, OBS-33, OBS-37). Observable has alert-derived incidents in `observable-alerting` with acknowledge, status, comments and a deployment link.

## Decision

Observable owns incidents, operational guidance, status communication and on-call schedules as first-class capabilities, and integrates external tools through adapters that exchange context without transferring ownership (Q4). Incidents, guidance, communication and exchange move to a new domain-layer component `observable-response` (`manage-incidents`, `manage-operational-guidance`, `communicate-incident-status`, `exchange-operational-context`); on-call (`manage-on-call`) and reliability objectives stay with `observable-alerting`.

## Consequences

**Easier:** Incident state is authoritative in one place (`risk-inconsistent-incident-state`); UX-04/UX-06 do not depend on third-party availability.  
**Harder:** More product scope (roles, timelines, communication approval, schedules) and adapter maintenance per external tool.  
**Constrained:** Adapters must not become a second source of incident truth; adapter targets are chosen in the phase 5-6 detailed plans.

## Alternatives Considered

### Option A: Integrate only
Less code, but essential response information would fail with the external tool (OBS-29-01) and the workflows could not be verified in-product. Rejected.

### Option B: Build only
Ignores OBS-16-01/16-04/16-08 exchange requirements. Rejected.

## Related

- [docs/plans/business-spec-adoption.md](../../docs/plans/business-spec-adoption.md) Q4
- [ADR-038](ADR-038-service-catalog-component.md)
