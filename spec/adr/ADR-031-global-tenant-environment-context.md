# ADR-031: Global Tenant and Environment Context in the Frontend UI

**Date:** 2026-05-05
**Status:** Accepted
**Authors:** Copilot
**Deciders:** Project Stakeholders
**Review date:** 2026-05-05

## Context

The platform supports multiple tenants (customers) each with multiple environments (e.g. `prod`,
`staging`, `testbench`) derived from their ingestion tokens (per ADR-028). Before this decision,
the frontend hardcoded the local-dev tenant UUID in every API call, making it impossible to view
data for any other tenant or filter by environment from the UI.

Users need a way to switch which tenant's data they are viewing and to optionally narrow the view
to a specific environment. This is a **workspace-level scope switch** — analogous to a workspace
or organisation picker in tools like Vercel or Slack — not a per-query signal filter.

### Relationship to the standing UI filter constraint

ADR-026 and the agent constraints say "avoid adding new selector-style filters". That constraint
targets signal-search UIs (filtering which spans or logs are shown inside an exploration view).
Tenant and environment selection is categorically different: it changes the **identity of the
data source** consulted by all subsequent queries, rather than narrowing one query's result set.
The constraint does not apply here.

### Domain model hierarchy note

The domain model (`spec/14-domain-model.md`) defines a Tenant → Project → Environment hierarchy.
The current implementation skips the Project layer: `api_keys` links directly to `tenant_id` and
`environment`, as formalised in ADR-028. This ADR follows the implemented model (Tenant →
Environment) and explicitly defers Project-level scoping to a future iteration alongside full
authentication.

## Decision

1. **Introduce two new backend endpoints in `query-api`** (placed outside the tenant-auth
   middleware because they are bootstrap resources used before a tenant is selected):
   - `GET /v1/tenants` — returns all tenants from the `tenants` PostgreSQL table.
   - `GET /v1/tenants/:id/environments` — returns `DISTINCT environment` from active
     (non-revoked) `api_keys` rows for the given tenant, derived per ADR-028.

2. **Introduce a global React Context in the frontend**: `TenantContextProvider` +
   `useTenantContext()` hook (modelled on the existing `ThemeProvider`/`useTheme` pattern).
   State: `{ tenantId: string, tenantName: string, environment: string | null }`.
   Defaults: self-ingestion/system tenant (`00000000-0000-0000-0000-000000000001`) and
   `environment = null` (= "all environments").

3. **Add two `<Select>` dropdowns to the AppShell topbar**, replacing the previous quick-link
   "Traces" and "Logs" buttons (both already reachable via the left sidebar):
   - **Tenant picker**: populated from `GET /v1/tenants`.
   - **Environment picker**: populated from `GET /v1/tenants/:id/environments`, cascades from
     the tenant selection. First option is always "All environments" (value = `null`).
   - On tenant change: environment is reset to "all".

4. **Thread `tenantId` through every API call**. All API module functions in `api/*.ts` accept
   `tenantId: string` as their first parameter. Call sites obtain the value from
   `useTenantContext()`. The `LOCAL_DEV_TENANT_ID` constant is retained as the dev seed value
   but must no longer be imported directly at API call sites.

## Auth hook point

When authentication is introduced:
- `GET /v1/tenants` will be protected and return only the tenants accessible to the
  authenticated principal (admin → all including the system tenant, regular user → own tenants).
- `useTenantContext` needs no structural change; the dropdown will simply show fewer options.
- Single-tenant users will see one option and the picker will effectively be hidden or collapsed.

## Consequences

**Easier:**
- Operators can switch tenant context without reloading the page.
- All existing queries automatically refetch when the tenant selection changes (TanStack Query
  uses `tenantId` in query keys).
- Auth integration requires only a server-side filter change, not a frontend restructure.
- Environment filtering is available across all signal types once the environment is propagated
  into backend query parameters in follow-on slices.

**Harder:**
- Every API function now requires `tenantId` as a parameter; adding new API functions must follow
  this convention.
- "All environments" (`null`) is currently not passed as a query parameter — the backend returns
  all environments for the tenant. Future slices may need to explicitly handle the null case
  differently per query type.

**Constrained:**
- The `LOCAL_DEV_TENANT_ID` constant must not be used directly in API call sites; use
  `useTenantContext().tenantId` instead.
- No "all tenants" aggregate view exists; the default is always a specific tenant.

## Alternatives Considered

### Option A: URL search params for tenant/environment
Rejected for this iteration in favour of React Context (consistent with existing `ThemeProvider`
pattern). URL persistence would make selections bookmarkable but adds router coupling. Deferred.

### Option B: Zustand store
Rejected to maintain consistency with the existing context-based state pattern in this codebase.
Zustand would be appropriate if cross-component state becomes more complex.

### Option C: Retain hardcoded tenant and add env filter only
Rejected. The hardcoded tenant is an anti-pattern that blocks multi-tenant development. The full
context switch is the correct long-term solution.

## Related

- ADR-007: Multi-Tenant Isolation Strategy
- ADR-028: Ingestion Token — Per-Token Environment Binding
- `spec/14-domain-model.md` (Tenant, ApiKey, Environment entities)
- `spec/04-tenancy-security.md` (Multi-Tenancy)
- `apps/frontend/src/hooks/useTenantContext.tsx`
- `services/query-api/src/tenants.rs`
- `services/query-api/tests/postgres_tenants_integration.rs`
