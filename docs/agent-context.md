# Agent Context

This file is the living starting map for agents working in this repository. It does not replace
reading the code. Every implementation task still requires inspecting the relevant files before
making changes.

## Required Startup Path

1. Read `AGENTS.md`.
2. Read `spec/adr/README.md`, then read any overlapping ADRs in full.
3. Read the active roadmap plan: `docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md`.
4. Read this file.
5. Inspect the actual code, tests, scripts, specs, and docs touched by the task before editing.
6. Create or switch to a dedicated short-lived branch before changing files.

## Current Source Of Truth

- Repository process: `AGENTS.md` and `spec/10-process.md`.
- Agent role routing: `.github/agents/README.md`, with `.github/agents/coordinator.agent.md` as the
  default entry role. Runtimes without subagent support should apply matching specialist `.agent.md`
  files manually as checklists.
- Active roadmap: `docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md` — consolidates the former post-Phase-3 plan and the Phases P9-P14 feature-parity plan (both now archived) into one feature-first backlog. Stability/compliance/enterprise-packaging work is intentionally demoted to a Deferred tier (§7 of that document) rather than gating new feature phases.
- `archived/plans/2026-06-10-p9-s5-service-catalog-health-signals.md` — completed P9-S5's remaining scope: replaced the hardcoded `active_alert_count: 0` / `latest_deployment: None` placeholders in `services/query-api/src/discovery.rs::service_summary_from_row` with real data and made `health_state` SLO-burn-rate-aware. Housekeeping note carried from that plan, still open: issues #388 (Trace Comparison) and #389 (Query Workbench) describe already-shipped features and should be closed.
- **2026-06-20**: PagerDuty and Opsgenie Notification Channel Adapters (were P12-S1/P12-S2) were retired from the Tier 1 backlog per explicit user direction — not building either integration. The generic `webhook` channel type remains the only outbound notification path; the Escalation Policy Builder (Tier 2) now targets it instead.
- **2026-06-20**: P12-S4 Change-Detection Alert Type is complete — `change_detection` alert_type with `evaluate_change_detection`/`eval_change_detection_rules` in `services/alert-evaluator/src/evaluator.rs`, CRUD support in `services/query-api/src/alerts.rs`, and a third form branch in `apps/frontend/src/features/alerts/AlertsPage.tsx`. See `archived/plans/2026-06-20-change-detection-alert.md` and `docs/superpowers/specs/2026-06-20-change-detection-alert-design.md`.
- **2026-06-20**: Promoted from roadmap §7 Deferred tier per explicit user request — Slice 1 of 3 of the admin-service extraction (ADR-033) is complete: `libs/observable-auth` extracted, `query-api` and `ingest-gateway` migrated to it. Also closes a real audit-trail gap found during the work: `query-api`'s API-key path previously bypassed `auth-service` and queried `api_keys` directly, producing no audit log; it now routes through `auth-service`'s `/internal/validate` like `ingest-gateway` already did. ADR-033 and the admin-service-extraction design doc were corrected — their original premise of duplicated local JWT verification was inaccurate; neither service did that. See `archived/plans/2026-06-20-observable-auth-crate.md`. **Known follow-up:** `ingest-gateway`'s API-key auth path has no equivalent audit-trail integration test (query-api's new one, `services/query-api/tests/api_key_audit_integration.rs`, has no counterpart there) — not yet added, noted here per the final review so it isn't lost.
- **2026-06-20**: Slice 2 of 3 of the admin-service extraction (ADR-033) is complete — `services/admin-service` now exists with working copies of `admin_members.rs`, `tokens.rs`, `config.rs`, `usage.rs` (each with its own duplicated test suite), and `apps/frontend/nginx.conf` routes `/v1/admin/`, `/v1/tokens`, `/v1/config`, and `/v1/tenants/usage-report` to it. query-api's original copies of these four handlers still exist and compile (deliberately, per the two-step rollout design) but are now unreachable in production. One scope correction found during implementation: the design doc's URL scheme said `/v1/tenants/config`; the real route (unchanged before/after this slice) is `/v1/config`. See `archived/plans/2026-06-20-admin-service-scaffold.md`. **Known follow-up:** the new `location` blocks for `/v1/tokens`, `/v1/config`, `/v1/tenants/usage-report` are bare-string prefixes (documented in `nginx.conf`) — any future sibling path sharing that prefix (e.g. `/v1/configs`) needs its own more specific location block. **Slice 3 complete 2026-06-20** (promoted ahead of the original "wait for a production deploy cycle" guidance, per explicit user request — this repo has no live production deployment yet, so the caveat didn't apply): query-api's now-dead duplicate handlers (`admin_members.rs`, `tokens.rs`, `config.rs`, `usage.rs`) removed; `admin-service` is the sole implementation. Two shared helpers (`env_llm_model`, `env_llm_url`, `fetch_db_key`, `fetch_db_value`) that `query-api`'s still-live `/v1/nlq` chat path depended on were extracted from the deleted `config.rs` into a new `services/query-api/src/llm_config.rs` rather than lost. See `archived/plans/2026-06-20-admin-service-cleanup.md`.
- P12-S3 Deadman alert type is complete (see "Deadman Alert Type" section below). Remaining promotion candidate (per the feature-parity plan's §7, not yet promoted): P14-S4 Change Event API (quick win, extends the deployment-marker model). P9-S2 Error Tracking Ingestion is the largest remaining workflow gap but needs its own multi-task plan once promoted.
- Archived detailed implementation plan: `archived/plans/2026-06-01-admin-console-overview.md` — first Admin Console landing-page slice for tenant access, environment context, and usage summary. Query Workbench is complete and its detailed plan has been archived. RF-2, RF-3, RF-6, P4-S9, stream-processor batching, Telemetry Loop Prevention, P4-S4 dashboard ReBAC, ClickHouse insert efficiency, Context Preservation, Live Tail, and Trace Comparison complete. The admin area now also has read-only `/admin/config` and `/admin/fleet` surfaces; the fleet page is a contract view until a live agent inventory endpoint exists. Next: P4-S3b SCIM/SSO only if a v1 customer requires it.
- Completed / archived detailed plans:
  - `archived/plans/2026-05-06-identity-provider-zitadel.md` — Zitadel 4.x OIDC PKCE flow, session JWTs, user/role tables, frontend login/callback/me pages, Admin Console identity settings
  - `archived/plans/2026-05-05-p4-s1-warm-retention.md` — warm-retention movement path (ARCHIVED/DEFERRED; not implemented)
  - `archived/plans/2026-05-17-dockerfile-clippy-cache.md` — Dockerfile planner/rust-ci selective copy + BuildKit cache mounts
  - `archived/plans/2026-05-15-trace-detail-uplift.md` — TraceDetail page-stack, MetricCards, service legend, Panel wrappers
  - `archived/plans/2026-05-12-dashboard-grid-redesign.md` — react-grid-layout edit mode + backend error surfacing
  - `archived/plans/2026-05-05-out-of-band-risk-remediation.md` — query-api auth hardening, NLQ SQL safety, CI integration-test gate, governance drift cleanup
  - `archived/plans/2026-05-21-p5-s5-composite-alert-evaluation.md` — composite alert rule-pair evaluation in the alert evaluator
  - `archived/plans/2026-05-22-p5-s6-reliability-reporting.md` — service-scoped reliability report endpoint and frontend tab
  - `archived/plans/2026-05-22-p4-s7-tenant-usage-report.md` — tenant usage and cost report for one billing interval
  - `archived/plans/2026-05-22-p4-s8-load-chaos-tenant-escape-upgrade-rollback.md` — release-readiness umbrella for load, chaos, tenant-escape, and upgrade/rollback evidence
  - `archived/plans/2026-05-22-p4-s2-backup-restore-drill.md` — P4-S2 hot-store restore drill for the shared PostgreSQL control-plane dataset (COMPLETED 2026-05-22)
  - `archived/plans/2026-05-10-p5-s2-notification-routing-webhook-complete.md` for P5-S2
  - `archived/plans/2026-05-23-rf-6-query-api-self-observability.md` — first RF-6 self-observability slice for `query-api` `/readyz` + `/metrics`
  - `archived/plans/2026-04-27-testcontainers-integration-tests.md` for P3-S15.
  - `archived/plans/2026-05-18-p5-s1-incident-timeline.md` — P5-S1 incident timeline (COMPLETED 2026-05-18)
  - `archived/plans/2026-05-20-p5-s4-topology-impact-view.md` — P5-S4 topology-aware impact view panel in IncidentDetailPage (COMPLETED 2026-05-20)
  - `archived/plans/2026-05-30-clickhouse-insert-efficiency.md` — stream-processor batching + storage-writer WriteBuffer (COMPLETED 2026-05-30)
  - `archived/plans/2026-05-30-p4-s4-dashboard-rebac.md` — fine-grained dashboard ReBAC via OpenFGA (COMPLETED 2026-05-30)
  - `archived/plans/2026-05-31-context-preservation.md` — global service filter preserved across all signal tabs (COMPLETED 2026-05-31)
  - `archived/plans/2026-05-31-live-tail.md` — live-tail streaming toggle for LogExplorer (COMPLETED 2026-05-31)
  - `archived/plans/2026-06-08-modelable-type-mapping-migration-plan.md` — modelable type-mapping migration master plan, all four phases (COMPLETED 2026-06-15)
  - `archived/plans/2026-06-10-modelable-pilot-span-row-types.md` — Phase 2 pilot: SpanRow/SpanEventRow from tracing.mdl
  - `archived/plans/2026-06-12-tracing-attributes-json-type.md` — Phase 2.4: map<string,json> attributes type
  - `archived/plans/2026-06-13-logs-modelable-migration.md` — Phase 3.1: logs domain
  - `archived/plans/2026-06-13-metrics-modelable-migration.md` — Phase 3.2: metrics domain
  - `archived/plans/2026-06-13-tracing-typescript-field-case.md` — Phase 2.5: TypeScript snake_case generation
  - `archived/plans/2026-06-14-admin-members-modelable-migration.md` — Phase 3.4: admin/members domain
  - `archived/plans/2026-06-14-alerts-modelable-migration.md` — Phase 3.7: alerts domain
  - `archived/plans/2026-06-14-dashboards-modelable-migration.md` — Phase 3.8: dashboards domain
  - `archived/plans/2026-06-14-incidents-modelable-migration.md` — Phase 3.6: incidents domain
  - `archived/plans/2026-06-14-notifications-modelable-migration.md` — Phase 3.3: notifications domain
  - `archived/plans/2026-06-14-slos-modelable-migration.md` — Phase 3.5: slos domain
  - `archived/plans/2026-06-15-nlq-visualization-modelable-migration.md` — Phase 3.9: nlq/visualization domain (last regular Phase 3 domain)
  - `archived/plans/2026-05-26-p4-s9-boundary-security-review.md` — P4-S9 boundary security review; two NLQ SQL identifier-injection fixes; findings at `docs/security-review-p4-s9.md`
  - `archived/plans/2026-06-18-frontend-design-system-modernization.md` — frontend design-system modernization across theme tokens, sidebar icons, themed selects, button/table polish, histogram SVG rendering, shared TopologyMap theming, and cross-theme visual verification (COMPLETED 2026-06-18)
  - `archived/plans/2026-06-18-p12-s3-deadman-alert.md` — P12-S3 deadman alert type: alert-evaluator span-recency check, query-api create/list support reusing the existing AlertRuleItem shape, AlertsPage "No data" rule type (COMPLETED 2026-06-18)
  - `archived/plans/2026-06-16-consolidation-plan.md` — post-modelable-migration repo consolidation: archived 13 modelable plans/12 specs + 6 non-modelable plans/specs, removed a duplicate zitadel plan, added `.mdl` header comments, documented the `ch-observable` binding duplication (COMPLETED 2026-06-16, landed via PR #405)
  - `archived/plans/2026-05-18-seed-generator.md` — bulk historical telemetry seed generator (`scripts/seed/`): world model, Postgres + ClickHouse seeders, trace/log/metric generators, CLI with `--resume`/`--dry-run`, Docker Compose `seed` profile (COMPLETED, landed via PR #350/#352)
  - `archived/plans/2026-05-19-p5-s3-runbook-attachment.md` — P5-S3 runbook URL attachment on alert rules, copied to incidents on creation, inline-editable in `AlertRuleDetailPage` (COMPLETED 2026-05-19)
  - `archived/plans/2026-06-01-admin-console-member-management.md` — `/admin/members` tab: list/invite/re-role/remove/revoke-sessions, `tenant_admin`-gated, self-demotion and last-admin-lockout guards (COMPLETED, landed via PR #395)
  - `archived/plans/2026-06-18-playwright-visual-verification-suite.md` — Playwright visual regression suite (`test:visual`), log/span panel overflow fixes with screenshot baselines (COMPLETED, landed via PR #406)
  - `archived/plans/2026-06-18-python-uv-migration.md` — migrated `models/`, `scripts/seed/`, and `testbench/{api,worker,loadgen}/` from pip/`requirements.txt` to `pyproject.toml` + `uv.lock`; Dockerfiles copy the static `uv` binary and run `uv sync --locked --no-dev`; `scripts/local-ci.sh`'s modelable step runs via `uv run --project models modelable`. Full-stack verification (Task 6) confirmed 2026-06-19: `local-ci.sh` exit 0, all four uv-based images build, `shop-api`/`shop-worker`/`shop-loadgen` stay healthy/running, no `requirements.txt` remain (COMPLETED 2026-06-19)
  - `archived/plans/2026-05-07-remaining-roadmap-plan.md` — superseded post-Phase-3 plan; full historical closure log and Phase 4-8 gap analysis retained for reference (SUPERSEDED 2026-06-19 by the unified feature roadmap)
  - `archived/plans/2026-06-04-observability-feature-parity-plan.md` — superseded Phases P9-P14 feature-parity plan; full workflow-gap analysis and success-metrics tables retained for reference (SUPERSEDED 2026-06-19 by the unified feature roadmap)
  - `archived/plans/2026-06-19-change-event-api-dashboard-overlay.md` — P14-S4 change-event API and dashboard overlay: `change_events` table, ingest-gateway/query-api create/list split, `TimeSeriesGraph` `changeEvents` overlay, `/change-events` explorer page (COMPLETED 2026-06-19)
  - `archived/plans/2026-06-19-setup-status-endpoint.md` — P9-S1 setup status endpoint: consolidated `GET /v1/setup/status` backend endpoint and onboarding wizard polling rewire (COMPLETED 2026-06-19)
- Housekeeping found during the 2026-06-19 roadmap consolidation, not yet acted on: GitHub issues #388 (Trace Comparison) and #389 (Query Workbench) describe already-shipped features and should be closed; the Trace UI Context Panel work tracked in project memory is fully merged into `main` (the `feat/trace-ui-context-panel` branch is 0 commits ahead) and that memory record is stale.
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
- Production runbooks now have a dedicated docs surface: `docs/runbooks/deployment-regression.md`
  covers canary and rollout regressions, with the exact `helm` and `kubectl` recovery steps used
  by the current canary flow.

## Tenant Usage Report (P4-S7, completed 2026-05-22)

- `services/admin-service/src/usage.rs` (moved from `query-api` 2026-06-20, admin-service extraction Slice 3) exposes `GET /v1/tenants/usage-report?from=...&to=...` and scopes the report with `TenantContext` plus `X-Tenant-ID`.
- The report is read-only and combines existing ClickHouse telemetry counts with PostgreSQL control-plane audit counts into a relative usage index.
- Frontend page `apps/frontend/src/features/admin/BillingReportPage.tsx` renders the report from the global date range on `/admin`.
- The UI uses the existing shared metric cards and panel components; no separate billing or invoicing model was added.

## P4-S8 Release-Readiness Suite (completed 2026-05-22)

- `scripts/release-candidate-suites.sh` is the local umbrella gate for the P4-S8 readiness evidence.
- `scripts/chaos-smoke.sh` restarts `storage-writer` in the compose stack and verifies that the pipeline recovers after a single failure injection.
- The tenant-escape signal still comes from `docker compose run --rm smoke-test`; the load baseline still comes from `docker compose run --rm perf-smoke`; the upgrade/rollback evidence still comes from `scripts/kind-test.sh`.
- The slice is implementation-only: no ADR or data-model change was required, and the new shell gates simply package the existing release-readiness signals into one repeatable command.

## Self-Observability (RF-6, completed 2026-05-26)

- All six Rust services expose `/readyz` on their respective ports; Prometheus `/metrics` was intentionally omitted (services already emit OTLP metrics via the self-observability pipeline).
- `alert-evaluator` (4322): readyz checks PostgreSQL + ClickHouse; AppState introduced in this slice.
- `ingest-gateway` (4321 platform port): readyz checks PostgreSQL; `IngestGatewayProbeState` keeps probe routes independent of the full AppState.
- `stream-processor` (4323 new probe port): readyz fetches Redpanda broker metadata via `spawn_blocking`; probe server runs as a background tokio task alongside the consumer loop.
- Docker Compose: `stream-processor` now has a healthcheck; `smoke-test` and `perf-smoke` upgraded from `service_started` to `service_healthy`.
- Helm: `streamProcessor.platformPort: 4323` added to values.yaml.

## Security Review (P4-S9, completed 2026-05-26)

- Full findings in `docs/security-review-p4-s9.md`.
- Two SQL identifier-injection findings fixed in `services/query-api/src/sql_templates.rs`:
  - `catalog_field` (NLQ catalog operation) now validated via `validate_sql_identifier()` before use as SQL alias and GROUP BY identifier.
  - `group_by` aliases validated by the same guard; invalid entries are silently dropped with a warning rather than failing the whole query.
- All other findings (bootstrap tenant list, cardinality observe-only, storage-writer internal no-auth) classified INFO and accepted for v1.
- Phase 4 exit gate: all P4 mandatory slices are now complete (P4-S2, P4-S3, P4-S5, P4-S6, P4-S7, P4-S8, P4-S9). RF-6 complete. Phase 5 pause-point review should precede new P5 work.

## Incident Timeline (P5-S1, completed 2026-05-18)

- `alert_rules` now has `auto_trigger_incident` (boolean, default `true`) and `auto_trigger_delay_secs`.
- `incidents` table stores structured incidents with `dedup_key`, `status`, `severity`, and `triggered_by_rule_id`.
- `incident_events` table stores the immutable timeline (`triggered`, `alert_fired`, `alert_resolved`, etc.).
- `services/alert-evaluator/src/evaluator.rs` automatically creates incidents when threshold/SLO alert firings transition to `active` (if `auto_trigger_incident = true`) and resolves them when the alert clears.
- `services/query-api/src/incidents.rs` exposes `GET /v1/incidents` and `GET /v1/incidents/:id`.
- `GET /v1/alerts/rules/:rule_id` — returns `AlertRuleDetailResponse` with rule metadata and up to 20 recent firings. Added in P5-S1 (commit fa33bca).
- `IncidentDetailResponse` now includes `rule_name: Option<String>` via LEFT JOIN on `alert_rules` (P5-S1) and `impacted_service: Option<String>` derived at query time from `slo_definitions` for `slo_burn_rate` rules (P5-S4). Threshold incidents return `null` for `impacted_service`.
- `GET /v1/services/:service_name/reliability-report` renders the P5-S6 reliability tab. The service scope is derived from `incidents.triggered_by_rule_id -> alert_rules -> slo_definitions` because the incidents table in this branch does not have a `service_name` column; the endpoint also filters SLOs and deployment markers by the requested service and optional environment.
- `TopologyMap` D3 component lives at `apps/frontend/src/components/topology/TopologyMap.tsx` (extracted from `ServiceTopologyPage` in P5-S4). Import from there when reusing the force-directed graph.
- Frontend: `apps/frontend/src/features/incidents/` contains `IncidentsPage.tsx` (list with status filters) and `IncidentDetailPage.tsx` (timeline + topology impact panel for SLO incidents).
- Frontend route `/alerts/$ruleId` renders `AlertRuleDetailPage` (rule metadata + firing history). Added in P5-S1.
- Alert evaluator `alert_fired`/`alert_resolved` incident event messages now include rule name and value (e.g. `"High CPU Alert fired: value=95.30"`). Added in P5-S1.
- `alert-evaluator` now supports composite alerts: `alert_type = 'composite'` rules use `condition.left_rule_id` and `condition.right_rule_id`; the evaluator treats the pair as an `AND` and fires only when both source rules are active.
- Known simplification: `dedup_key` currently uses `rule_id` only because `alert_rules` lacks `service_name`/`environment`. The spec (`spec/14-domain-model.md`) defines `rule_id + service_name + environment`.
- `GET /v1/services/summary` and `GET /v1/services/{service_name}/summary` (P9-S5) populate `active_alert_count` (active `slo_burn_rate` firings linked via `slo_definitions.service_name` only — not all alert types, due to `alert_rules` lacking `service_name`) and `latest_deployment` (latest `deployment_markers.service_version`). `health_state` is `"breach"` if any linked SLO is currently breaching, else the existing error-rate threshold result. See `archived/plans/2026-06-10-p9-s5-service-catalog-health-signals.md`.

## Modelable Type-Mapping Migration (Phase 4 complete, 2026-06-15)

All 10 in-scope domains (tracing, logs, metrics, notifications, admin/members, slos,
incidents, alerts, dashboards, nlq/visualization) have been migrated onto
[modelable](https://github.com/ktjn/modelable)-generated TypeScript artifacts; tracing and
logs additionally have generated Rust `db`-projection Row types. `3.5b Schemas` remains
deliberately deferred (no frontend consumer exists for `SchemaEntry`/`SemanticAnnotation`).
See `ADR-032: Adopt Modelable as the Type-Mapping Source of Truth` for the decision record and
current-state table, and
`archived/plans/2026-06-08-modelable-type-mapping-migration-plan.md` for the Phase 1
backlog and per-domain design specs.

- **Model sources:** `models/*.mdl`, validated/compiled with the pinned version in
  `models/pyproject.toml`. There is no local modelable install in this repo — regenerate using
  a checkout of `modelable` itself: `cd <modelable-checkout>/cli && .venv/Scripts/python.exe -m
  modelable compile <observable-checkout>/models --target <rust|typescript> --out <scratch-dir>`,
  then copy the relevant generated files into this repo and commit them (generated code is
  committed, not built in CI — see "Resolved Decisions" in the migration plan).
- **Generated Rust artifacts:** `libs/domain/src/generated/<domain>/`, re-exported via a
  `mod.rs` with `#![allow(dead_code)]`; hand-written domain types reference generated row types
  via `pub type FooRow = generated::<domain>::FooRowV1;` where the shapes are 1:1.
- **Generated TypeScript artifacts:** `apps/frontend/src/api/generated/<domain>/`. Each file is
  copied verbatim from the compiler output plus a short "do not edit, regenerate with..." header
  comment. Hand-written `apps/frontend/src/api/<domain>.ts` re-exports the generated types
  (`import type { Foo } from "./generated/<domain>/<domain>.Foo.v1"; export type { Foo };`)
  instead of declaring its own interface.
- **`@wire(...)` hints** bridge gaps between the IDL's canonical representation and each
  target's real wire format without affecting other targets — e.g.
  `@wire(json.fieldCase: "snake_case")` (added in modelable v0.4.0) makes only the generated
  TypeScript use snake_case field names to match Rust's serde output, with zero effect on the
  Rust/JSON-Schema/SQL/lineage output. Use the narrowest hint that closes the gap; document new
  hints with a short comment above the `entity`/`projection` they apply to.
- **Per-domain rule:** only replace types that represent canonical domain/wire contracts.
  Handler-level aggregation/wrapper types with no 1:1 generated equivalent (e.g.
  `TraceResponse`, `FacetValue`, `TraceListResponse`) stay hand-written — state the reason in the
  PR. Type-tightening from generated types (literal unions for enums, required vs. optional
  fields) is expected fallout; fix call sites with valid enum literals/casts and `{}` defaults
  for now-required maps, not `any`/`@ts-expect-error`.
- **Verification:** `cargo fmt --all` + relevant `cargo test` crates for Rust changes;
  `npm run typecheck && npm test && npm run build` for frontend changes; `bash
  scripts/local-ci.sh`; and a `modelable lineage <Type@version>` proof in the PR description
  showing no `type_loss` warnings.
- **Completed domains:** tracing, logs, metrics, notifications, admin/members, slos,
  incidents, alerts, dashboards, nlq/visualization. `3.5b Schemas` is deferred — no frontend
  consumer exists for `SchemaEntry`/`SemanticAnnotation` yet; revisit if/when one is added.
  Further Rust-layer migration for the 8 domains without generated Rust artifacts is blocked
  on the Phase 1 backlog items documented in `ADR-032`'s "Known Limitations" section.

## Deadman Alert Type (P12-S3, completed 2026-06-18)

- `services/alert-evaluator/src/evaluator.rs` adds `eval_deadman_rules`: fires when no span has
  been received for a service within `window_secs` (including services never seen at all).
  Wired into `eval_alert_rules` alongside the threshold/SLO/composite evaluators.
- `services/query-api/src/alerts.rs` `create_alert_rule`/`list_alert_rules` support
  `alert_type = 'deadman'` by reusing the existing `AlertRuleItem` shape: deadman conditions are
  surfaced as `metric_name = service_name`, `operator = "no_data"`, `threshold = window_secs`.
  This was a deliberate choice to avoid extending the modelable-generated `AlertRuleItem` schema
  for this slice — see `docs/superpowers/specs/2026-06-18-p12-s3-deadman-alert-design.md`.
- Frontend: `AlertsPage.tsx`'s create-rule form has an "Alert type" selector ("Threshold metric"
  / "No data") that swaps in service-name/window fields; the rules table renders `no_data` rows
  as `"No data for {window}s from {service}"`.

## Change Events (P14-S4, completed 2026-06-19)

- New `change_events` PostgreSQL table (`migrations/postgres/032_create_change_events.sql`),
  independent of `deployment_markers` — it covers config changes, feature-flag toggles, schema
  migrations, and ad-hoc incident annotations, i.e. operationally-relevant changes that aren't a
  service deploy. Unlike `deployment_markers.service_name`, `change_events.service_name` is
  nullable to allow tenant/environment-wide events not scoped to one service.
- Follows the same create/list split already established for deployment markers: creation is
  `POST /v1/events/changes` on `ingest-gateway`'s platform port (4321, `auth_middleware`-gated,
  `services/ingest-gateway/src/change_events.rs`), listing is `GET /v1/events/changes` on
  `query-api`'s tenant-scoped read path (`services/query-api/src/change_events.rs`, with
  `list_change_events` exposed as a plain pool-level function and covered by a Testcontainers
  integration test mirroring the alerts one).
- Frontend: `TimeSeriesGraph` (`apps/frontend/src/components/ui/time-series-graph.tsx`) gained a
  `changeEvents` overlay prop rendered as dashed vertical lines with diamond markers (vs.
  deployments' triangle) colored per `event_type`, wired into `ServiceDetailPage.tsx`'s
  `ResponseTimeGraphSection` alongside the unchanged deployment-marker overlay.
  **Known limitation**: that chart's change-events query filters by `service_name` for the
  current service, so tenant/environment-wide events (`service_name = null`) never appear there —
  only the unfiltered `/change-events` explorer page surfaces them.
- New top-level `/change-events` explorer route (`apps/frontend/src/features/changeEvents/ChangeEventsPage.tsx`)
  with a nav entry under Signals, offering plain service/event-type filters (not the NLQ bar —
  this is a control-plane PostgreSQL table like deployments, not an NLQ-queryable ClickHouse signal).

## Setup Status Endpoint (P9-S1, completed 2026-06-19)

- The Onboarding Wizard (`apps/frontend/src/features/onboarding/OnboardingWizard.tsx`) was already
  shipped with its full 4-step flow (language/framework picker, API key creation, polling, success
  state). This slice closed the one literal gap between that deliverable and the roadmap line: the
  roadmap names `GET /v1/setup/status`, but the wizard's polling step previously fanned out to three
  separate existing endpoints (`/v1/traces`, `/v1/logs`, `/v1/metrics`) client-side.
- `services/query-api/src/setup.rs` adds a consolidated, tenant-scoped `GET /v1/setup/status`
  endpoint (`compute_setup_status`, a plain ClickHouse-client-level function reused directly by
  Testcontainers integration tests) that counts traces/logs/metrics within the same 60-minute
  lookback window the frontend previously used, returning `state: "detected" | "waiting"`.
- `apps/frontend/src/api/setup.ts`'s `getFirstSignalStatus(tenantId)` now calls the single new
  endpoint instead of issuing three parallel requests; its external signature and `FirstSignalStatus`
  shape are unchanged, so `OnboardingWizard.tsx` required no changes beyond its test stubs.
- See `archived/plans/2026-06-19-setup-status-endpoint.md` for the full task breakdown.

- **2026-06-21**: `query-api`'s Testcontainers integration tests now share one Postgres and one
  ClickHouse container per `cargo test` run (`libs/test-support`'s `postgres::shared_pool()` /
  `clickhouse::shared_client()`), consolidated into a single `tests/it/main.rs` binary (15 files
  migrated across Tasks 4-10), instead of spinning up a fresh container per test function.
  Postgres isolation is a fresh migrated database per test; ClickHouse isolation is per-test
  random tenant IDs against one shared `observable` database (production SQL hardcodes that
  name). `http_api_integration.rs`'s ClickHouse-touching tests are deliberately exempt — they key
  off a fixed `DEV_TENANT_ID` for the auth header path and the production histogram endpoints
  have no per-test query discriminator finer than tenant + time window, so sharing would require
  sacrificing test parallelism. See
  `docs/superpowers/specs/2026-06-21-testcontainers-shared-container-design.md` for the full
  design and `archived/plans/2026-06-21-testcontainers-shared-container-pilot.md` for this
  pilot's implementation. Follow-up slices (same pattern, one per service) remain for
  `admin-service`, `alert-evaluator`, `auth-service`, `storage-writer`, `ingest-gateway`, and
  `stream-processor` (the last needs a new `test_support::redpanda` module, not yet built).

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
