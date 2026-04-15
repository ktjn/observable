# ADR-007: Multi-Tenant Isolation Strategy

**Date:** 2026-04-15  
**Status:** Proposed  
**Authors:** Gemini CLI  
**Deciders:** Project Stakeholders  
**Review date:** 2026-04-15  

## Context

The platform must support multiple tenants with varying isolation requirements (shared, isolated storage, or dedicated deployments). Strong isolation is critical for security, performance, and compliance (e.g., GDPR, SOC 2).

## Decision

The platform will adopt a **layered multi-tenancy isolation strategy**:
1.  **Logical Isolation (Shared):** All data includes a `tenant_id` for filtering in queries and storage.
2.  **Storage Isolation (Optional):** Support for per-tenant storage clusters or namespaces (e.g., ClickHouse databases).
3.  **Compute Isolation:** Per-tenant quotas and rate limits enforced at the ingest and query layers.
4.  **Security Isolation:** OIDC/SAML for identity, with fine-grained authorization (ReBAC) for resource access.

## Consequences

**Easier:** 
- Strong security and data privacy.
- Flexible deployment options for different customer tiers.
- Ability to enforce resource quotas and prevent "noisy neighbor" issues.

**Harder:** 
- Increased complexity in managing per-tenant configuration and storage.
- Query performance can be affected by complex filtering.
- Testing and validation of isolation boundaries are more rigorous.

**Constrained:** 
- Every data access must include a verified `tenant_id`.

## Alternatives Considered

### Option A: Pure Shared (Logical Only)
Rejected because some enterprise customers require stronger physical isolation for compliance.

### Option B: Pure Dedicated (Single-Tenant Only)
Rejected because it doesn't scale efficiently for smaller customers or self-serve onboarding.

## Related

- `spec/04-tenancy-security.md` (Multi-Tenancy)
- `spec/13-risks-roadmap.md` (Risk 3: Weak tenant isolation)
