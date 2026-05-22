# Agent Context

This file is the living starting map for agents working in this repository. It does not replace
reading the code. Every implementation task still requires inspecting the relevant files before
making changes.

## Required Startup Path

1. Read `AGENTS.md`.
2. Read `spec/adr/README.md`, then read any overlapping ADRs in full.
3. Read the active roadmap plan: `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`.
4. Read this file.
5. Inspect the actual code, tests, scripts, specs, and docs touched by the task before editing.
6. Create or switch to a dedicated short-lived branch before changing files.

## Current Source Of Truth

- Repository process: `AGENTS.md` and `spec/10-process.md`.
- Agent role routing: `.github/agents/README.md`, with `.github/agents/coordinator.agent.md` as the
  default entry role. Runtimes without subagent support should apply matching specialist `.agent.md`
  files manually as checklists.
- Active roadmap: `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md` — unified post-Phase-3 implementation plan.
- Active detailed implementation plan: none.
- Completed / archived detailed plans:
  - `archived/plans/2026-05-06-identity-provider-zitadel.md` — Zitadel 4.x OIDC PKCE flow, session JWTs, user/role tables, frontend login/callback/me pages, Admin Console identity settings
  - `archived/plans/2026-05-05-p4-s1-warm-retention.md` — warm-retention movement path (ARCHIVED/DEFERRED; not implemented)
  - `archived/plans/2026-05-17-dockerfile-clippy-cache.md` — Dockerfile planner/rust-ci selective copy + BuildKit cache mounts
  - `archived/plans/2026-05-15-trace-detail-uplift.md` — TraceDetail page-stack, MetricCards, service legend, Panel wrappers
  - `archived/plans/2026-05-12-dashboard-grid-redesign.md` — react-grid-layout edit mode + backend error surfacing
  - `archived/plans/2026-05-05-out-of-band-risk-remediation.md` — query-api auth hardening, NLQ SQL safety, CI integration-test gate, governance drift cleanup
  - `archived/plans/2026-05-10-p5-s2-notification-routing-webhook-complete.md` for P5-S2
  - `archived/plans/2026-04-27-testcontainers-integration-tests.md` for P3-S15.
  - `docs/superpowers/plans/2026-05-18-p5-s1-incident-timeline.md` — P5-S1 incident timeline with source links (COMPLETED 2026-05-18)
- Historical Phase 1 plan: `archived/plans/2026-04-17-phase1-internal-mvp.md`; do not treat it as an active backlog.
- Historical Phases 2-8 plan: merged into the active roadmap above. The old `2026-04-18-phases2-8-iteration-plan.md` file has been removed.
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

## Deployment Marker Enrichment (RF-5, completed 2026-05-09)

- `services/ingest-gateway/src/deployment_registry.rs` resolves the active deployment marker
  for each incoming span's (tenant_id, service_name, environment, service_version) and stamps
  `deployment_id` before the span is published to Redpanda.
- The registry caches results for 30 s; on DB error it returns `""` (fail-open, never blocks ingestion).
- `canary-promote.sh --marker-url <url>` creates a marker before deploy and updates it to
  `success` or `failed` when promotion or gate-failure completes.
- `ingest-gateway` is now a dual lib+bin crate (`src/lib.rs` re-exports `deployment_registry`
  for Testcontainers integration tests in `services/ingest-gateway/tests/`).

## Incident Timeline (P5-S1, completed 2026-05-18)

- `alert_rules` now has `auto_trigger_incident` (boolean, default `true`) and `auto_trigger_delay_secs`.
- `incidents` table stores structured incidents with `dedup_key`, `status`, `severity`, and `triggered_by_rule_id`.
- `incident_events` table stores the immutable timeline (`triggered`, `alert_fired`, `alert_resolved`, etc.).
- `services/alert-evaluator/src/evaluator.rs` automatically creates incidents when threshold/SLO alert firings transition to `active` (if `auto_trigger_incident = true`) and resolves them when the alert clears.
- `services/query-api/src/incidents.rs` exposes `GET /v1/incidents` and `GET /v1/incidents/:id`.
- `GET /v1/alerts/rules/:rule_id` — returns `AlertRuleDetailResponse` with rule metadata and up to 20 recent firings. Added in P5-S1 (commit fa33bca).
- `IncidentDetailResponse` now includes `rule_name: Option<String>` via LEFT JOIN on `alert_rules` (P5-S1) and `impacted_service: Option<String>` derived at query time from `slo_definitions` for `slo_burn_rate` rules (P5-S4). Threshold incidents return `null` for `impacted_service`.
- `TopologyMap` D3 component lives at `apps/frontend/src/components/topology/TopologyMap.tsx` (extracted from `ServiceTopologyPage` in P5-S4). Import from there when reusing the force-directed graph.
- Frontend: `apps/frontend/src/features/incidents/` contains `IncidentsPage.tsx` (list with status filters) and `IncidentDetailPage.tsx` (timeline + topology impact panel for SLO incidents).
- Frontend route `/alerts/$ruleId` renders `AlertRuleDetailPage` (rule metadata + firing history). Added in P5-S1.
- Alert evaluator `alert_fired`/`alert_resolved` incident event messages now include rule name and value (e.g. `"High CPU Alert fired: value=95.30"`). Added in P5-S1.
- `alert-evaluator` now supports a first composite alert slice: `alert_type = 'composite'` with `condition` fields `left_rule_id` and `right_rule_id`; the evaluator treats the pair as an `AND` and fires only when both source rules are active.
- Known simplification: `dedup_key` currently uses `rule_id` only because `alert_rules` lacks `service_name`/`environment`. The spec (`spec/14-domain-model.md`) defines `rule_id + service_name + environment`.

## Dev Environment Gotchas

### PostgreSQL major version upgrade requires volume reset
Bumping the `postgres` image tag across a major version (e.g. 16→17) in `docker-compose.yml`
makes the existing `observable_postgres_data` volume incompatible. The container will crash with:
```
FATAL: database files are incompatible with server
DETAIL: The data directory was initialized by PostgreSQL version N.
```
Fix: run `make reset-volumes` (or `bash scripts/reset-dev-volumes.sh`) to drop the old volume,
then `docker compose up --build`. All 28+ migrations in `migrations/postgres/` re-apply
automatically via `postgres-setup`. No data is lost — this is a local dev environment.

### Redpanda version bump that skips logical versions also requires a volume reset
Redpanda tracks an internal logical version and refuses to upgrade by more than a few steps
at a time. A too-large version jump crashes with:
```
Assert failure: 'false' Attempted to upgrade from incompatible logical version N to M!
```
Fix: same as above — `make reset-volumes` drops `redpanda_data`. The `telemetry.raw` topic is
re-created automatically by the `redpanda-setup` container on next startup.

The same pattern applies if ClickHouse changes on-disk formats across major versions.
`make reset-volumes` (default, no flags) now drops `postgres_data`, `shop_db_data`, and
`redpanda_data`. Use `--all` to also wipe ClickHouse and Zitadel bootstrap volumes.

### Browser auth routing uses the shared Gateway
The live k8s cluster currently exposes the frontend through `observable/testbench-gateway`
listener `observable` on port 80, with the frontend HTTPRoute attached at `/`. Zitadel now
needs its own HTTPRoute on the same listener so `/oauth`, `/oidc`, `/.well-known`, and `/ui`
go to `observable-zitadel` instead of looping through the SPA.
For local access, the gateway is port-forwarded to `localhost:8080`, so the browser-facing
Zitadel origin is `http://localhost:8080`, while the Zitadel service itself still listens on
internal port 8080.

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
- Dependency upgrades prefer the latest stable versions. Use native tooling only: npm for npm
  packages, cargo for Rust crates, and uv for Python packages. If Python dependencies are not yet
  uv-managed, plan the `pyproject.toml` + `uv.lock` migration before changing them. Keep Docker
  Compose and Testcontainers image versions identical for the same dependency unless the PR explains
  a deliberate compatibility exception.

## Keep This File Updated

Update this file in the same PR when a change affects future agent orientation, including:

- repo layout or ownership boundaries;
- active roadmap or plan selection;
- required verification commands or exemptions;
- architectural assumptions, deployment assumptions, or dependency-boundary rules;
- important gotchas discovered while implementing or verifying a slice.

If a change does not affect future agent guidance, state that in the PR description instead of
editing this file.
