# ADR-008: Authorization Model

**Date:** 2026-04-15  
**Status:** Accepted  
**Authors:** Gemini CLI  
**Deciders:** Project Stakeholders  
**Review date:** 2026-04-15  

## Context

The platform requires a robust and scalable authorization model to manage access to diverse resources (dashboards, projects, incidents, data scopes) across multiple tenants. Simple RBAC is often insufficient for the granular and relationship-based access patterns common in large-scale observability platforms.

## Decision

The platform will use a **Relationship-Based Access Control (ReBAC)** model, influenced by Google's Zanzibar and implemented using a solution like **OpenFGA**. This will be supplemented with a coarse-grained RBAC for tenant-level administrative roles.

## Consequences

**Easier:** 
- Highly granular and flexible permission management.
- Ability to define complex relationships (e.g., "owner of project X has access to all incidents in project X").
- Centralized and auditable authorization logic.
- Scalable to millions of relationships.

**Harder:** 
- More complex implementation and integration compared to simple RBAC.
- Requires learning a new DSL and model for defining permissions.
- Potential performance impact on the query path (must be optimized with caching).

**Constrained:** 
- All resource access must be checked against the ReBAC service.

## Alternatives Considered

### Option A: Pure RBAC (Role-Based Access Control)
Rejected because it leads to "role explosion" and difficulty in managing fine-grained, dynamic access patterns.

### Option B: Custom Authorization Logic in Every Service
Rejected due to lack of consistency, poor auditability, and high maintenance overhead.

## Related

- `spec/04-tenancy-security.md` (Authorization)
- `spec/14-domain-model.md` §6 (Authoritative authorization entity definitions: RBAC roles, ReBAC tuple format, resource types, DataScope)
- `spec/10-process.md` (ADR list)
- `spec/adr/ADR-007-multi-tenant-isolation.md` (Multi-Tenant Isolation Strategy)
