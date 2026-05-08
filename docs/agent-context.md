# Agent Context

This file is the living starting map for agents working in this repository. It does not replace
reading the code. Every implementation task still requires inspecting the relevant files before
making changes.

## Required Startup Path

1. Read `AGENTS.md`.
2. Read `spec/adr/README.md`, then read any overlapping ADRs in full.
3. Read the active split roadmap plans: `docs/superpowers/plans/2026-05-07-finish-started-work-plan.md`
   and `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`.
4. Read this file.
5. Inspect the actual code, tests, scripts, specs, and docs touched by the task before editing.
6. Create or switch to a dedicated short-lived branch before changing files.

## Current Source Of Truth

- Repository process: `AGENTS.md` and `spec/10-process.md`.
- Agent role routing: `.github/agents/README.md`, with `.github/agents/coordinator.agent.md` as the
  default entry role. Runtimes without subagent support should apply matching specialist `.agent.md`
  files manually as checklists.
- Active roadmap: split between `docs/superpowers/plans/2026-05-07-finish-started-work-plan.md`
  for already-started/scoped work and `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`
  for the long-horizon backlog. Keep `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`
  as the historical Phases 2-8 closure reference.
- Active detailed implementation plan: `docs/superpowers/plans/2026-05-05-p4-s5-slo-burn-rate.md`,
  unless the finish-started plan requires out-of-band risk remediation first.
- Completed detailed plan archive: `archived/plans/2026-04-27-testcontainers-integration-tests.md` for P3-S15.
- Historical Phase 1 plan: `archived/plans/2026-04-17-phase1-internal-mvp.md`; do not treat it as an active backlog.
- Architecture decisions: `spec/adr/`.
- Product and platform specs: `spec/`.

## Codebase Map

- `apps/frontend/`: React 19 + Vite frontend.
- `apps/frontend/src/components/`: shared reusable frontend components.
- `apps/frontend/src/features/**/components/`: feature-scoped frontend components.
- `services/`: Rust platform services.
- `libs/`: shared Rust libraries.
- `contracts/` and `proto/`: API and protobuf contracts.
- `migrations/`: versioned database migrations.
- `charts/`: Helm deployment assets.
- `scripts/`: local CI, smoke, migration, and operational scripts.
- `tests/`: cross-cutting test suites and end-to-end checks.

## Global Tenant + Environment Context (ADR-031)

Every API call that is tenant-scoped must receive `tenantId` as its first parameter — obtained
from the `useTenantContext()` hook. **Never import `LOCAL_DEV_TENANT_ID` at an API call site.**

Key files:
- `apps/frontend/src/hooks/useTenantContext.tsx` — `TenantContextProvider` + `useTenantContext` hook.
  Default: self-ingestion/system tenant (`00000000-0000-0000-0000-000000000001`), environment `null` (= all).
- `apps/frontend/src/api/tenants.ts` — `listTenants()` and `listEnvironments(tenantId)` (bootstrap, no auth header needed).
- `services/query-api/src/tenants.rs` — `GET /v1/tenants` and `GET /v1/tenants/:id/environments`.
  Routes are registered **outside** the `require_tenant` auth middleware (bootstrap endpoints), but are filtered by the authenticated user session if a `session` cookie or `Bearer` token is present.

Pattern for new call sites:
```typescript
const { tenantId } = useTenantContext();
useQuery({ queryKey: ["my-key", tenantId], queryFn: () => myApiFn(tenantId, ...params) });
```

`LOCAL_DEV_TENANT_ID` (exported from `api/setup.ts`) is still valid as the dev seed default value
in `useTenantContext.tsx` itself, but must not be used directly at API call sites.

When authentication is introduced, `GET /v1/tenants` will filter by the authenticated principal's
access; the frontend needs no structural changes.

The `projects` table exists in PostgreSQL (seeded with one "default" row) but is not connected to
`api_keys`. The Tenant → Project → Environment hierarchy is deferred; this iteration implements
Tenant → Environment only (per ADR-028 + ADR-031).

## Standing Constraints

- Never commit or merge directly to `main` without human review.
- Every implementation iteration needs a short-lived branch, commit, push, and pull request.
- Pure documentation changes are exempt from `bash scripts/local-ci.sh`; code changes are not.
- Rust code changes must run `cargo fmt --all` explicitly before pushing, even though
  `bash scripts/local-ci.sh` also runs formatting.
- Completed detailed task plans must move from `docs/superpowers/plans/` to `archived/plans/`,
  with active roadmap and agent-context links updated in the same PR.
- Backend changes crossing PostgreSQL, ClickHouse, Redpanda/Kafka-compatible brokers, object
  storage, OpenFGA, or similar real dependency boundaries need the narrowest applicable
  Testcontainers integration test unless the PR explains why a different regression signal applies.
- Frontend work must reuse existing shared or feature components before adding new ones.
- Frontend filtering surfaces use the shared NLQ query input as the primary filter UI. Preserve the
  separate global time picker, accept raw `NlqIr` JSON as the no-LLM fallback, and avoid adding new
  selector-style filters unless a spec or ADR explicitly reintroduces them.
- ADRs and specs must be updated together when architecture, technology choices, deployment model,
  data model, security model, or roadmap scope changes.

## Keep This File Updated

Update this file in the same PR when a change affects future agent orientation, including:

- repo layout or ownership boundaries;
- active roadmap or plan selection;
- required verification commands or exemptions;
- architectural assumptions, deployment assumptions, or dependency-boundary rules;
- important gotchas discovered while implementing or verifying a slice.

If a change does not affect future agent guidance, state that in the PR description instead of
editing this file.
