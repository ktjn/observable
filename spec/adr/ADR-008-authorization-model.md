# ADR-008: Explicit identity, authorization and purpose boundaries

**Original decision:** 2026-04-15  
**Revised:** 2026-11-07  
**Status:** Accepted — conditional relationship authorization

## Context

Observable needs tenant isolation, resource ownership, purpose-bound investigation, restricted-existence protection and accountable privileged access. These are not all expressible as plain RBAC or relationship tuples. Zitadel authenticates users; application services remain responsible for authorizing each operation against resource scope, context and purpose.

## Decision

- Retain OIDC-based identity and Zitadel integration; do not couple business identity concepts to one identity provider.
- Define versioned authorization capabilities covering principal, tenant, environment, resource, action, purpose, elevation and expiry. Deny by default when essential context or policy evaluation is missing.
- Every API and query path enforces authorization **before** disclosing data or existence. UI hiding is advisory only, not a security boundary.
- Use coarse-grained RBAC plus tenant-scoped ownership for simple operations. Retain OpenFGA as a **candidate** for relationships that demonstrably require ReBAC; do not make a separate policy server mandatory for every deployment without representative policy and operational tests.
- If OpenFGA is used, enforce purpose-based conditions, time-limited elevation, query scoping and audit semantics in the authoritative application policy layer as required. OpenFGA does not by itself prove all business access requirements.
- Cross-tenant and cross-purpose reads must fail closed, including search suggestions, aggregate charts, caches, deep links and background exports. Prevent restricted-record enumeration through distinct errors or side channels.
- Audit sensitive lookup attempts and elevation using minimized metadata; never log secrets or protected payloads. Define data retention and owner approval separately.

## Decision gates

Prototype access for service catalog, incident investigation and business-reference transaction lookup before exposing those endpoints:
1. Positive and negative policy matrix including denied existence.
2. Purpose-based scoped access and explicit time-bounded elevation.
3. Authorization on direct API, search, filters, correlations and exports.
4. Fail-closed behavior during policy backend outages.
5. Load, cache-invalidation, cross-tenant isolation and audit fidelity.
6. OpenFGA versus simpler scoped policies measured for complexity and operational reliability.

## Consequences

Identity and authorization contracts remain implementation-independent. A decision to adopt OpenFGA at scale requires supporting measurements and may be revisited without weakening OBS security requirements.

## Related

- [ADR-041](ADR-041-purpose-bound-authorization.md)
- [Business assurance hardening](../../docs/plans/phase0-assurance-hardening.md)
