# 2026-06-01 Admin Console Overview

**Status:** Completed

**Goal:** Turn the existing `/admin` surface into a real Admin Console landing page that summarizes tenant access, environment scope, identity entry points, and the current usage/cost view without adding backend endpoints.

**Source spec:** `spec/05-frontend.md` §9.2.1, §9.5, §9.11, §9.13; `spec/04-tenancy-security.md`; ADR-031

**Parent roadmap item:** `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md` §5 Platform Administration -> Admin Console

**Acceptance target:** `/admin` becomes a composited admin landing page that:
- shows the selected tenant and environment context from `useTenantContext()`
- shows the current user’s tenant memberships and roles from `/v1/auth/me`
- reuses the existing tenant usage report as the cost/usage summary
- links to the existing identity settings surface at `/admin/identity`

**User/operator outcome:** tenant admins and operators can land on one page and immediately see who they are, which tenant they are working in, what environments are available, and where identity settings live.

**Files or modules expected to change:**
- `apps/frontend/src/pages/AdminPage.tsx`
- `apps/frontend/src/features/admin/BillingReportPage.tsx` or a nearby admin feature component if refactoring is needed
- `apps/frontend/src/pages/AdminPage.test.tsx`
- `apps/frontend/src/router.ts` only if route wiring needs to be adjusted
- `docs/agent-context.md`

**Out of scope:**
- new backend endpoints
- editable tenant settings
- RBAC mutation flows
- quota enforcement changes
- fleet health / remote config UI
- identity provider mutation or SCIM provisioning

**Verification:**
- `npm run test -- apps/frontend/src/pages/AdminPage.test.tsx`
- `npm run typecheck`
- `npm run lint`
- `npm run build`
- `bash scripts/local-ci.sh --skip-docker` if the full gate is too heavy for the slice, otherwise the full local CI gate before push

**Baseline:** `/admin` currently renders only the tenant usage report and `/admin/identity` is a separate read-only page.

**New errors introduced:** none expected.

**Telemetry impact:** none.

**Auth/tenancy impact:** read-only use of the existing authenticated `/v1/auth/me` and tenant/environment context APIs; no new permission model.

**Data retention or migration impact:** none.

**Rollback path:** revert the `AdminPage` composition changes and keep the existing billing report and identity pages intact.

**ADR/spec sync:** no ADR change expected; this slice only reuses existing frontend/auth/tenant contracts and does not alter the architectural model.

**Checkpoint question:** after the landing page is in place, is the next admin slice tenant configuration, RBAC, quota management, or fleet health?

**Next smallest slice:** add a dedicated tenant-configuration read view or split the fleet health summary into its own `/admin/fleet` page, depending on which data/API surface is already available.

**Closure note:** Implemented on `feat/admin-console-overview`; `/admin` now composes tenant access, environment scope, identity entry points, and the usage summary without adding backend endpoints.
