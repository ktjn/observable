# ADR-041: Purpose-Bound and Existence-Hiding Authorization

**Date:** 2026-11-07  
**Status:** Accepted (2026-11-07; decisions Q1–Q10 in docs/plans/business-spec-adoption.md, accepted by merging the 2.0 re-review PR)  
**Authors:** Claude Code  
**Deciders:** Tommy (ktjn)  
**Superseded by:**  
**Review date:** 2026-12-07  

## Context

ADR-008 accepted OpenFGA relationship-based authorization, but no service calls OpenFGA; access is tenant isolation plus `tenant_admin | member | viewer` roles and dashboard/saved-view grants. The specification requires role- and need-based access (OBS-14-01, 14-05), segregation by legal entity or business unit (14-10), time-limited break-glass access with justification (14-09), purpose-bound and audited transaction lookup (31-03), elevated-access indication (25-03), and withholding restricted records without revealing their existence (25-02, `risk-unauthorized-disclosure`).

## Decision

Implement authorization as a single policy decision point in `observable-auth`, exposed as the `decide-access` contract (`POST /internal/authz/decide`, fail closed, denied and non-existent indistinguishable) that realizes `govern-data-access`, using relationship-based checks per ADR-008, extended with purpose and elevation context on each request, list filtering at query time so restricted records are never returned or counted, and audit records for elevated and purpose-bound access (`audit-and-compliance`). The contract and `req-decide-access-fail-closed` are Phase 1 deliverables because `observable-catalog` and `observable-health` call it from their first protected read. Whether OpenFGA remains the engine is decided in the phase 8 detailed plan.

## Consequences

**Easier:** One place to verify disclosure rules; UI and API share the same decisions (OBS-16-07).  
**Harder:** Every list and search endpoint must filter through the decision point; performance under incident load (OBS-19-08) must be measured.  
**Constrained:** Role names change on `2.0`; existing role checks in handlers are replaced.

## Alternatives Considered

### Option A: Keep coarse roles
Cannot express purpose, segregation or existence hiding. Rejected.

### Option B: Per-service ad hoc checks
Inconsistent decisions and no single audit point. Rejected.

## Related

- [ADR-008: Authorization model](ADR-008-authorization-model.md)
- [docs/plans/business-spec-adoption.md](../../docs/plans/business-spec-adoption.md) section 4 decision 5
