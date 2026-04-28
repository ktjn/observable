# Phases 2–8 — Interactive Small-Iteration Delivery Plan

> **Purpose:** Convert the remaining roadmap into a practical sequence of small, reviewable vertical slices with explicit pause points, decision checkpoints, and entry/exit criteria.

> **Scope:** This document starts **after Phase 1**. It assumes the platform can already ingest telemetry, persist it, query it, and render the minimum UI described in `spec/10-process.md §17` and the existing [Phase 1 plan](2026-04-17-phase1-internal-mvp.md).

---

## 1. Operating Rules

This plan follows:
- `spec/10-process.md §16.8` tiny-agent iteration workflow
- `spec/10-process.md §17` phased roadmap
- `spec/13-risks-roadmap.md §24.3` near-term execution order

Every iteration in the remaining phases must:
1. change one user-visible or operator-visible behavior
2. stay small enough for one reviewer sitting
3. include verification, rollback, and next-slice notes
4. stop at a checkpoint before crossing a new trust boundary
5. avoid bundling backend, frontend, infrastructure, and docs unless the slice requires all of them
6. **Mandatory UI Standards**: Every frontend slice MUST:
   - Use the **feature-based directory structure** (`src/features/<domain>`)
   - Use **Base UI** primitives for new interactive components (Shadcn pattern)
   - Implement **Tailwind CSS v4** for styling
   - Include **MSW handlers** for the new API endpoints
   - Include **Accessibility tests** (`playwright-axe`) for major new views
   - Follow the **Testing Trophy** (prioritize integration tests with RTL/MSW)
7. **UI renovation gate before new product UI**: before starting additional product UI workflows such as threshold alerts, dashboards, or dashboard-as-code, complete the dedicated UI renovation lane below. The goal is to finish converting existing high-traffic views to the modern primitive/token system so new product slices do not extend the legacy mixed-style surface area. Backend-only and infrastructure-only slices may proceed when they do not add or change frontend surface area.
8. **Mandatory backend integration harness**: Every backend slice that touches PostgreSQL, ClickHouse, Redpanda/Kafka-compatible brokers, object storage, OpenFGA, or another real containerized dependency boundary MUST add or update the narrowest applicable Testcontainers integration test unless the slice explicitly requires Docker Compose, kind, browser, or external-provider verification instead. If Testcontainers is not applicable, the PR must state why and name the replacement signal.
9. update this plan document as part of the PR's definition of done:
   - mark the finished slice state
   - add any new checkpoint answer or discovered dependency
   - adjust the next recommended slice if priorities changed

---

## 2. How To Use This Document

Treat this as an interactive backlog driver, not a one-shot build spec.

For each slice:
1. Confirm the phase entry gate is actually satisfied.
2. Pick the next unchecked slice in priority order.
3. Write the slice contract in the PR body.
4. Implement only that slice.
5. Verify locally.
6. Update this plan document before opening or updating the PR:
   - change the slice checkbox state
   - record the checkpoint answer if the slice resolves one
   - note any sequence changes caused by new information
7. Pause at the checkpoint question and record the answer in the PR.
8. Merge only after review.
9. Start the next slice on a new branch.

If a checkpoint answer is "not yet" or "unclear", stop and resolve that before advancing.

**PR definition of done for this plan:** a slice PR is not done until the implementation and this
plan document agree on the current state of the phase.

---

## 3. Standard Slice Packet

Use this packet shape for every slice after Phase 1:

```markdown
Source spec:
Phase:
Parent phase item:
Acceptance target:
User/operator outcome:
Files or modules expected to change:
Out of scope:
Verification:
Baseline:
New errors introduced:
Telemetry impact:
Auth/tenancy impact:
Data retention or migration impact:
Rollback path:
ADR/spec sync:
Checkpoint question:
Next smallest slice:
```

---

## 4. Global Sequencing Rules For The Remaining Project

1. Finish **Phase 2** before starting broad **Phase 3** correlation work.
2. Finish the operationally necessary parts of **Phase 4** before building the full **Phase 5** reliability product.
3. Do not start **Phase 6** advanced signals until retention, auth, and release safety are proven under load.
4. Treat **Phase 7** as customer-driven packaging and policy work; only do the parts required by target customers.
5. Treat **Phase 8** as optional until data quality, retention, labeling, and auditability are stable.
6. Preserve self-observability as a platform invariant: every platform component must emit logs, metrics, traces, health, readiness, and Prometheus-compatible metrics, and every deployment must route that telemetry either to the platform's own `system` tenant or to a second Observable instance. Self-observability covers three instrumentation levels: service level (Rust services, workers, jobs, APIs, queues, storage dependencies, and background tasks), infrastructure level (Kubernetes nodes, pods, containers, ingress, service mesh if present, Redpanda, ClickHouse, PostgreSQL, object storage, and resource saturation), and UI level (frontend route transitions, API calls, Web Vitals, render errors, user-visible failures, and critical interaction latency). The recommended production choice is a second observer instance, because it keeps visibility available when the primary ingest, queue, storage, query, or UI-serving path is degraded. Recursive self-ingest remains the default for local development, internal dogfooding, and single-instance bootstrap environments.

---

## 5. Phase 2 — Governed MVP

**Goal:** Make the internal MVP safe to run continuously with tenant isolation, cost controls, release controls, and auditability.

**Entry gate:**
- [x] Phase 1 exit gate passed and Task 17 through Task 20 status is reconciled in the Phase 1 plan (2026-04-18)
- [x] Query API correctness bugs are recorded as resolved in `spec/09-api.md` with Phase 1 Task 20 as closure evidence (2026-04-18)
- [x] Baseline ingest/query smoke-test stability is recorded in the Phase 1 plan (2026-04-18)

**Exit gate:**
- tenant isolation enforced and tested
- rate limits, quotas, RBAC, audit logs, and retention are working
- deployment can roll forward and back through controlled automation

### Priority slice order

The frontend sequence for this phase starts by closing the service-centric MVP UI defined in
`spec/05-frontend.md` §9.3 before adding broader topology, deployment, dashboard-builder, or
incident-management surface area. Explorer-only UI is treated as foundation, not closure.

- [x] **P2-S0: Reconcile plan and spec state before Phase 2 implementation**
  - Outcome: Phase 1 closure, resolved query API bug state, and Phase 2 next-slice guidance agree across the plan and API spec. Standardized on a unified Docker Compose workflow and updated ADR-019 and referencing specs to reflect the simplified setup.
  - Checkpoint: can an agent start Phase 2 without resolving contradictory planning state first? Answer: yes, after the initial reconciliation and subsequent synchronization of deployment scripts and specs.

- [x] **P2-S1a: Enforce tenant context contract for trace query**
  - Outcome: trace query code cannot execute without tenant context, and tenant A cannot read tenant B traces
  - Checkpoint: do we have a failing-then-passing tenant-escape test for trace lookup and trace search? Answer: yes. Query API now rejects missing tenant context before handlers run, trace responses validate all returned rows against the request tenant, and tests cover missing/invalid tenant context plus same-tenant and cross-tenant trace rows.

- [x] **P2-S1b: Enforce tenant isolation for log query**
  - Outcome: tenant A cannot read tenant B log records
  - Checkpoint: do we have one negative cross-tenant test and one positive same-tenant test for log search? Answer: yes. `search_logs` now collects all LogRow values before converting them, calls `validate_log_rows_for_tenant` which fails closed on any cross-tenant row, and unit tests cover same-tenant valid, cross-tenant rejected, and empty result cases.

- [x] **P2-S1c: Enforce tenant isolation for metric query**
  - Outcome: tenant A cannot read tenant B metric series or points
  - Checkpoint: do metric series lookup and point lookup both include tenant-scoped assertions? Answer: yes. `list_metrics` now collects all MetricSeriesRow values before converting them, calls `validate_metric_series_rows_for_tenant` which fails closed on any cross-tenant row; `get_metric_points` does the same with `validate_metric_point_rows_for_tenant`. Unit tests cover same-tenant valid, cross-tenant rejected, and empty result cases for both series and point lookups.

- [x] **P2-S1d: Assert tenant partition preservation in storage writes**
  - Outcome: span, log, and metric storage rows preserve the tenant partition key from the normalized envelope. The stream-processor now stamps `env.tenant_id` onto every span, log, metric series, and metric point before forwarding to the storage-writer, making the envelope the single authoritative source of tenant identity. Unit tests in the storage-writer verify that `SpanRow`, `LogRow`, `MetricSeriesRow`, and `MetricPointRow` each preserve the tenant_id from the domain object unchanged.
  - Checkpoint: can storage writer tests identify a tenant-key regression without relying on query API behavior? Answer: yes. Four new storage-writer tests (`span_row_preserves_tenant_id`, `log_row_preserves_tenant_id`, `metric_series_row_preserves_tenant_id`, `metric_point_row_preserves_tenant_id`) and four new stream-processor normalise tests (`normalise_span_stamps_envelope_tenant_id`, `normalise_log_stamps_envelope_tenant_id`, `normalise_metric_series_stamps_envelope_tenant_id`, `normalise_metric_point_stamps_envelope_tenant_id`) all pass without touching the query API.

- [x] **P2-S2a: Add deterministic rate limiting for trace ingest**
  - Outcome: one authenticated tenant exceeding a trace-ingest request budget gets a stable `429` rejection path. The ingest-gateway now enforces a per-tenant token-bucket quota (default 100 req/s, configurable via `TRACE_INGEST_RATE_LIMIT_PER_SECOND`) using the `governor` crate. Exceeded requests return HTTP 429 with a `Retry-After: 1` header and a JSON body `{"error":"rate_limit_exceeded","message":"Trace ingest rate limit exceeded"}`. A warning log with `tenant_id` is emitted on every rejection.
  - Checkpoint: are status code, error body, retry semantics, and telemetry stable enough to reuse for logs and metrics? Answer: yes. Status code is 429, error body uses a stable `error`/`message` shape, `Retry-After` is set, and the warn log carries tenant context. The same pattern (add `DefaultKeyedRateLimiter<Uuid>` to AppState, check before handler body, emit warn log) applies directly to log and metric ingest handlers with no redesign needed.

- [x] **P2-S3a: Add cardinality budget observation for one signal**
  - Outcome: operators can see budget consumption for one signal before enforcement starts. A `MetricCardinalityBudget` tracker was added to the ingest-gateway (`cardinality.rs`). Every `/v1/metrics` request increments a per-tenant cumulative series counter; when the total meets or exceeds the configurable budget (`METRIC_SERIES_BUDGET_PER_TENANT`, default 10 000), a `warn!` log is emitted with `tenant_id`, `series_count`, and `budget`. Ingest is never rejected; the counter is observation-only. Four unit tests cover counter accumulation, independent-tenant tracking, budget exhaustion, and the exact boundary. Four integration tests verify HTTP 200 on valid payloads, counter increment after a request, continued HTTP 200 when budget is exceeded, and 401 on missing auth.
  - Checkpoint: do we have operator-visible telemetry for budget exhaustion without changing ingest acceptance yet? Answer: yes. The `warn!` log fires when `series_count >= budget` and carries structured fields an operator can query or alert on. Ingest acceptance is unchanged — the handler always returns 200 after the observe call regardless of budget state.

- [x] **P2-S4a: Add hot retention policy for traces**
  - Outcome: one trace retention path is enforced end to end. A `RetentionConfig` struct in `services/storage-writer/src/retention.rs` reads `TRACE_HOT_RETENTION_DAYS` (default 14, clamped to 3–14 per ADR-012) and `RETENTION_CHECK_INTERVAL_SECONDS` (default 3600). A background Tokio task starts at service startup, ticks every interval, computes a Unix-nanosecond cutoff, and issues `ALTER TABLE observable.spans DELETE WHERE start_time_unix_nano < {cutoff_ns}` against ClickHouse. Every cycle logs the configured retention window and cutoff timestamp before submitting the mutation. Failures are logged as warnings; the HTTP server continues regardless. Six unit tests cover: cutoff arithmetic, underflow safety, default config, clamping below minimum, clamping above maximum, and the SQL shape.
  - Checkpoint: deletion timing and rollback behavior are explicit. Timing: the worker fires once per `RETENTION_CHECK_INTERVAL_SECONDS` (default 1 h); the cutoff is `now - hot_trace_days * 86 400 s`, converted to nanoseconds. The schema-level TTL (14-day) remains as a safety net. Rollback: remove the `TRACE_HOT_RETENTION_DAYS` env var to restore the default, or stop the storage-writer (the mutation is already queued in ClickHouse and runs asynchronously, but no new mutations will be issued). The ClickHouse TTL ensures eventual cleanup even if the explicit worker is disabled.

- [x] **P2-S5a: Add audit logging for credential validation**
  - Outcome: API key validation produces immutable audit records for allow and deny outcomes. The auth-service now appends a row to `credential_audit_log` on every call to `/internal/validate` — both allow and deny paths. Fields: `occurred_at`, `action` ("credential_validate"), `outcome` ("allow"/"deny"), `credential_hash` (SHA-256 of presented key; this is both the actor identity and the credential identifier at this layer), `tenant_id` (nullable; NULL when the key is not found), `denial_reason` (NULL on allow; "not_found", "revoked", or "hash_mismatch" on deny). Audit writes are fire-and-forget: a failure logs a warning but does not fail the auth response. Migration `004_create_credential_audit_log.sql` adds the table with indexes on `occurred_at` and `tenant_id`. Three new unit tests verify the `AuditEntry` constructor fields for all three outcomes.
  - Checkpoint: are audit fields sufficient for tenant, actor, action, credential identifier, and outcome? Answer: yes. `tenant_id` covers tenant (nullable for key-not-found); `credential_hash` serves as both actor and credential identifier (no plaintext exposure); `action` is the fixed label "credential_validate"; `outcome` is "allow" or "deny"; `denial_reason` further qualifies deny outcomes.

- [x] **P2-S5b: Add audit logging for query reads**
  - Outcome: trace, log, and metric reads produce audit records with tenant, actor, action, and result metadata. The query-api now appends a row to `query_audit_log` on every successful read across all five handlers: `trace_get`, `trace_search`, `log_search`, `metric_series_list`, and `metric_points_get`. Fields: `occurred_at`, `action` (handler label), `tenant_id` (the authenticated tenant; serves as both tenant and actor at this layer), `result_count` (number of rows returned; no payload content). Writes are fire-and-forget: a failure logs a warning but does not fail the query response. Migration `005_create_query_audit_log.sql` adds the table with indexes on `occurred_at` and `tenant_id`. Three new unit tests verify `QueryAuditEntry` field values for `trace_get`, `log_search`, and `metric_points_get` action labels.
  - Checkpoint: can query-read auditing run without logging sensitive payload contents? Answer: yes. Only `action`, `tenant_id`, and `result_count` (an integer) are recorded. No query parameters, field names, attribute values, or payload bytes are written to the audit table.

- [x] **P2-S6a: Add minimal RBAC distinction for one role pair**
  - Outcome: at least one privileged and one read-only role differ in observable API behavior. A `role` column (`viewer` | `member` | `admin`, default `member`) was added to `api_keys` via migration `006_add_role_to_api_keys.sql`. The auth-service now returns `role` alongside `tenant_id` in `/internal/validate` responses. The ingest-gateway extracts role from the auth response, stores it in `TenantContext`, and rejects requests with `403 Forbidden` when the role is `viewer`. A seeded viewer dev key (`dev-viewer-key-0000`) is available for testing. Unit tests cover `member`/`admin` allowed and `viewer` rejected for both `can_ingest()` logic and the full HTTP path (`POST /v1/traces`). Query endpoints remain open to all roles (read-only paths require no role restriction at this stage).
  - Checkpoint: is the role model still simple enough to extend without redesign? Answer: yes. Role is a single `TEXT` column on `api_keys` with a `CHECK` constraint; adding new roles or moving to a separate `roles` table is a straightforward migration. The `can_ingest()` method is the single enforcement point; extending to per-endpoint checks or a role-hierarchy follows the same pattern with no architectural change needed.

- [x] **P2-S7a: Add one threshold alert evaluation path**
  - Outcome: an operator can define a threshold and see an alert fire. A new `alert-evaluator` service reads `alert_rules` (type `threshold`) from PostgreSQL, queries the most recent metric point for each rule from ClickHouse, evaluates the scalar value against the condition (`gt`, `gte`, `lt`, `lte`, `eq`), writes an `alert_firings` row (state `active`), and emits a `warn!` log with `rule_id`, `tenant_id`, `metric_name`, `value`, and `threshold` when the condition fires. Migrations `007_create_alert_rules.sql` and `008_create_alert_firings.sql` add the tables. A seeded dev rule fires when `error_rate > 0.05`. Eight unit tests cover all operators.
  - Checkpoint: is the evaluation model stable enough to support burn-rate later? Answer: yes. The `alert_rules` table holds `alert_type` with an enum constraint that already includes `slo_burn_rate`; the `condition` JSONB column can carry a different structure for burn-rate rules without schema migration. The evaluator's dispatch model (read rules → branch on `alert_type` → evaluate → record firing) extends naturally: add a new `alert_type = 'slo_burn_rate'` branch that parses a burn-rate condition struct and queries the error budget window from ClickHouse. The `alert_firings` table and `start_eval_worker` loop are reused unchanged. The `for_duration_secs` column is present for the Pending→Active debounce but is not yet enforced; enforce it when flap avoidance becomes a requirement.

- [x] **P2-S8a: Add Kubernetes manifest render and rollback skeleton**
  - Outcome: one deployable environment has rendered manifests and a documented rollback path. A Helm library chart (`charts/observable-common`) provides shared Deployment, Service, and label templates; an umbrella chart (`charts/observable`) composes all six services with a pre-install migration Job hook. Infrastructure is deployed in kind via `deploy/kind/infra/` manifests using the same images and env var names as `docker-compose.yml`. `scripts/kind-test.sh` creates a kind cluster, installs the chart, runs ingest-to-query smoke checks, and verifies `helm rollback`. `scripts/helm-lint.sh` validates chart syntax. ADR-020 documents the Helm and kind tooling decisions. `spec/11-testing.md §18.7` documents the full Kubernetes test strategy.
  - Checkpoint: does rollback documentation cover both runtime and schema assumptions? Answer: yes. `spec/12-deployment.md §19.7` documents: (1) runtime rollback via `helm rollback <release> <revision>` which redeploys the previous Deployment specs without re-executing migration Jobs; (2) schema rollback policy — migrations are forward-only (ADR-013), backward compatibility with the preceding image version is a release gate, and the expand–migrate–contract pattern is required for any migration that would break a previous service version. `scripts/kind-test.sh` exercises the rollback path as part of the kind integration test.

- [x] **P2-S8b: Add one canary promotion path**
  - Outcome: one deployable environment can promote progressively and revert safely. A canary Deployment + Service template (`charts/observable/templates/ingest-gateway-canary.yaml`) deploys a candidate `ingest-gateway` image tag alongside the stable release, isolated behind a dedicated `ingest-gateway-canary` Service so production traffic is never diverted. `scripts/canary-promote.sh` runs three automated gates (health, smoke ingest, zero 5xx in pod logs after a configurable soak) and either promotes stable (upgrades global image tag, removes canary) or reverts (removes canary, stable unchanged). `spec/12-deployment.md §19.8` documents lifecycle, rollback contract, and the relationship to Argo Rollouts.
  - Checkpoint: do automated analysis gates have enough telemetry to make rollback decisions? Answer: yes at this stage. Gate 1 (health) and Gate 2 (smoke ingest) provide binary liveness and acceptance signal; Gate 3 (5xx count in pod logs) provides an error-rate signal sufficient to catch regression during soak. The gates are conservative (zero-5xx tolerance, HTTP 200 required) and can be refined without changing the promotion contract. Full metric-based SLO burn-rate gates (using the alert-evaluator path from P2-S7a) are the next evolution when Argo Rollouts is provisioned.

- [x] **P2-S9a: Add perf smoke baseline for ingest and common query paths**
  - Outcome: Phase 2 has measurable performance baselines instead of assumptions. `scripts/perf-smoke.sh` seeds one trace/log/metric, waits for the pipeline, then samples each ingest and query endpoint 20 times. It reports P50 and P95 per path and exits non-zero if any path exceeds its threshold (ingest P50 < 500 ms / P95 < 1000 ms; query P50 < 1000 ms / P95 < 3000 ms per spec/11-testing.md §18.3). A `perf-smoke` Docker Compose service runs the script against the live stack. The nightly CI workflow (`.github/workflows/nightly.yml`) runs `perf-smoke` after the existing smoke-test step. Thresholds are overridable via env vars. This slice also closes the old Phase 1 Task 19 carry-forward for CI-level smoke/nightly follow-through.
  - Checkpoint: are the numbers good enough to proceed to correlation features? Answer: yes. In a local Docker Compose environment with minimal synthetic data, all ingest paths complete well under 500 ms P50 (auth + Redpanda publish on a single machine) and all query paths complete well under 1000 ms P50 (ClickHouse hot cache with minimal data). The baseline script is the regression gate — any future change that pushes P50 or P95 over threshold will fail the nightly extended gate before reaching Phase 3 work.

### Phase 2 pause point

Before Phase 3 starts, answer:
- Can we prove tenant safety under test?
- Can we explain cost controls without hand-waving?
- Can we roll back a bad deploy without manual heroics?

---

## 6. Phase 3 — Correlation And Service Operations

**Goal:** Turn isolated telemetry views into connected service operations workflows.

**Entry gate:**
- Phase 2 exit gate passed
- tenant isolation and RBAC behavior are stable
- query latency for common paths is acceptable for correlation views

**Exit gate:**
- operators can move between service, trace, log, metric, and deployment context without manual ID copying

### Priority slice order

- [x] **P3-S0: Define deployment marker specification and ingestion contract**
  - Outcome: The authoritative specification for deployment markers is established in `spec/18-deployment-markers.md`, covering ingestion schema, RBAC, and automatic `deployment_id` enrichment.
  - Checkpoint: Is the ingestion contract stable enough to be used by CI/CD providers? Answer: Yes, the spec defines a stable OTLP-compatible JSON and Protobuf path.

- [x] **P3-S1: Add trace-to-log correlation for logs with full trace context**
  - Outcome: a trace detail view can fetch exact correlated log lines. The `query-api` now supports `trace_id` and `span_id` filters for logs, and the frontend `TraceDetail` view displays these logs, allowing for span-level filtering.
  - Checkpoint: are joins based on canonical IDs only, with no fuzzy heuristics yet? Answer: yes. Joins use exact equality on `trace_id` and `span_id` in ClickHouse.

- [x] **P3-S1b: Add log-context (surrounding logs) capability**
  - Outcome: operators can view logs occurring before and after a specific log line for the same host/service. The `query-api` now supports `GET /v1/logs/:log_id/context`, and the frontend allows clicking a log line to see its surrounding context.
  - Checkpoint: does the context view correctly ignore search filters while preserving tenant and host/service scope? Answer: yes. The context query uses `service_name` and `host_id` from the pivot log and ignores any other search parameters.

- [x] **P3-S1c: Add live tail capability for logs**
  - Outcome: the Query API exposes `GET /v1/logs/tail` for tenant-scoped cursor reads ordered by timestamp, and the frontend explorer shows a live log panel that polls every 1s, appends new records, deduplicates by `log_id`, and auto-scrolls to the newest line. The first transport is cursor-polled JSON rather than SSE so the existing explicit `X-Tenant-ID` header remains mandatory.
  - Checkpoint: is the end-to-end latency from ingest to UI display < 2s? Answer: yes by contract for this slice: the frontend polls once per second, so newly queryable ClickHouse rows are fetched within the <2s live-tail target under the existing ingest-to-query path. Full ingest pipeline latency should be measured in a follow-up perf/smoke slice once synthetic log generation is part of the smoke harness.

- [x] **P3-S2: Add trace-level log correlation when `span_id` is absent**
  - Outcome: trace views show trace-correlated logs without claiming exact span linkage. The trace detail log panel now fetches all logs for the trace, keeps trace-level logs with no `span_id` visible even when a span is selected, and filters out logs from other spans in selected-span mode.
  - Checkpoint: is the UI language precise about exact vs trace-level correlation? Answer: yes. Logs with a `span_id` are labeled `Exact span`; logs without a `span_id` are labeled `Trace-level`. The selected-span heading says `Exact span logs and trace-level logs`, while the all-trace view says `Trace-correlated logs`.

- [x] **P3-S2b: Add rate limiting for log ingest**
  - Outcome: one authenticated tenant exceeding a log-ingest request budget gets a stable `429` rejection. Mirrors the pattern from P2-S2a for traces. Completed 2026-04-21.
  - Checkpoint: is the rate-limit response shape (status code, error body, `Retry-After`, warn log) identical to the trace path so operators face a consistent contract? Answer: yes.

- [x] **P3-S2c: Add rate limiting for metric ingest**
  - Outcome: one authenticated tenant exceeding a metric-ingest request budget gets a stable `429` rejection. Mirrors the pattern from P2-S2a and P3-S2b. Completed 2026-04-21.
  - Checkpoint: are all three signal ingest paths (traces, logs, metrics) now covered by rate limiting? Answer: yes.

- [x] **P3-S2d: Fix OTLP standard port conformance for the ingest-gateway**
  - Source spec: `spec/02-architecture.md §4.1` (required interfaces: OTLP/gRPC and OTLP/HTTP), ADR-001.
  - Context: the ingest-gateway currently serves HTTP/JSON on port 4317 (the OTLP/gRPC standard port) and the auth-service occupies port 4318 (the OTLP/HTTP standard port). This means any standard OTLP sender — including Collectable-built binaries, the OTel Collector, and language SDKs using default configuration — cannot reach Observable without non-standard endpoint configuration.
  - Outcome: the ingest-gateway accepts OTLP/gRPC on port 4317 (via tonic) and OTLP/HTTP JSON (`application/json`) on port 4318. Port 4317 is gRPC-only, and port 4318 does not accept OTLP/HTTP protobuf (`application/x-protobuf`). The auth-service moves to an internal-only port (4319). The Collectable mediator template is updated so `OTLP_PROTOCOL=http` emits OTLP JSON (`http-json` feature) and attaches `Authorization: Bearer ${OTLP_TOKEN:-}` — matching what the ingest-gateway's auth middleware expects. `spec/12-deployment.md` port table, `spec/16-collectable.md` endpoint examples, `docker-compose.yml`, and Helm chart values are updated to reflect the new layout. A new ADR (ADR-023) documents the standard port assignment and the migration path for any existing deployments using the old ports. Completed 2026-04-21.
  - Checkpoint: can a standard OTLP sender (e.g. `otelcol`, a Collectable binary with `OTLP_ENDPOINT=http://host:4318`) deliver logs, traces, and metrics to Observable with no Observable-specific client configuration beyond an API key? Answer: yes for port layout and HTTP/JSON path; gRPC stubbed and ready for full Protobuf mapping.

- [x] **P3-S3: Add frontend navigation shell and theme system**
  - Source spec: `spec/05-frontend.md` §9.2, §9.11, §9.13.
  - Outcome: the React app has primary navigation entries for Services, Infrastructure, Service Overview, Dashboards, Alerts & SLOs, and Admin / Fleet / Billing. Light, dark, and system themes are selectable and persisted as `light`, `dark`, or `system`. Completed 2026-04-21 with a root app shell, token-based light/dark styles, persisted theme preference, product-area routes, and focused frontend tests.
  - Files or modules expected to change: `apps/frontend/src/App.tsx`, router setup, layout/navigation components, design token/theme utilities, frontend tests.
  - Out of scope: real service, infrastructure, or topology data. Use current routes, placeholders, or existing mock data until backend endpoints exist.
  - Verification: frontend unit tests cover route rendering and theme preference behavior; frontend typecheck/lint/build pass.
  - Checkpoint: can operators switch major product areas and theme modes without losing project, environment, tenant, time range, or URL state? Answer: yes for the shell-level contract. Product-area routes keep the current project/environment/tenant/time-range context visible in the app shell, and theme changes persist independently of route state. Service-specific filter preservation remains part of P3-S4 through P3-S6.

- [x] **P3-S4: Build the Services catalog from resource attributes**
  - Source spec: `spec/05-frontend.md` §9.2.1 Services; `spec/09-api.md` Service Detail Summary.
  - Outcome: services appear as navigable entities with stable IDs, health/performance columns, search, filters, and sort controls.
  - Files or modules expected to change: query-api service discovery/summary endpoint, service catalog API client, Services route, catalog table/list component, tests.
  - Out of scope: full service detail page. Rows link to a placeholder service detail route if the detail view is not implemented yet.
  - Verification: API tests prove tenant-scoped service listing; frontend tests cover service rendering, empty state, filter state, and navigation.
  - Checkpoint: do we have a durable service identity model or are we still overfitting to labels?

- [x] **P3-S5: Add service detail overview with quick performance**
  - Source spec: `spec/05-frontend.md` §9.2.1 Services; `spec/09-api.md` Service Detail Summary.
  - Outcome: one service detail overview shows request rate, error rate, latency percentiles, SLO/health state, active alert count, latest deployment marker, and related signal entry points. Completed 2026-04-21 with a tenant-scoped single-service summary endpoint, frontend service detail overview route, and focused backend/frontend tests.
  - Files or modules expected to change: query-api service summary path, RED metric derivation for one service, frontend service detail route, overview panels, tests.
  - Out of scope: editable dashboards and advanced SLO management.
  - Verification: API tests cover one service summary; frontend tests cover overview rendering and links to Logs, Metrics, Traces, and Infrastructure. `bash scripts/local-ci.sh` passed.
  - Checkpoint: is the derived service summary contract good enough for alerting, dashboards, and later SLO reuse? Answer: yes for P3 scope. The summary contract now has stable RED metrics, health state, active alert count, and latest deployment marker fields; alert count and deployment marker are explicit placeholders until alert/deployment slices populate them.

- [x] **P3-S6: Add service-scoped Logs, Metrics, and Traces tabs**
  - Source spec: `spec/05-frontend.md` §9.2.1 Services and §9.4.
  - Outcome: service detail has Logs, Metrics, and Traces tabs. Each tab opens with service and time range filters applied, preserves URL state, and links back to the overview. Completed with service detail subroutes (`/services/:serviceId/logs`, `/metrics`, `/traces`), frontend panels that reuse existing log/trace query helpers and a new metric-series helper, and a service filter on the metrics query endpoint.
  - Files or modules expected to change: frontend service detail route tree, existing trace/log/metric explorer filter wiring, API client query parameters, tests.
  - Out of scope: new query language features. Reuse existing explorer capabilities and backend filters.
  - Verification: frontend tests cover tab deep links, browser-back behavior, and service filter preservation; API tests cover service-scoped query filters where missing.
  - Checkpoint: are URLs now the source of truth for service investigation context across traces, logs, and metrics? Answer: yes. Service investigation tabs are addressable by path, keep the `lookback_minutes` query string, and each tab applies the route service as the query filter.

- [x] **P3-S6b: Make self-observability routing explicit for all platform components**
  - Outcome: every platform service, worker, frontend-serving component, migration job, canary path, scheduled/background task, infrastructure dependency, and UI runtime has an explicit self-observability route. The configuration supports two modes: `self`, which sends platform telemetry to the primary instance's `system` tenant, and `observer_instance`, which sends it to a second Observable instance using a separate endpoint and credential. Production and customer-facing environments should use `observer_instance`; local development, internal dogfooding, and bootstrap environments may use `self`. This slice absorbs the old Phase 1 Task 17 carry-forward (health/readiness, Prometheus metrics, `system`-tenant routing, verification) and the Phase 1 Task 18 frontend-instrumentation carry-forward.
  - Instrumentation scope:
    - Service level: all Rust services, HTTP/gRPC handlers, auth checks, queue producers/consumers, storage writes, query execution, alert evaluation, migrations, canary paths, and background tasks emit traces, metrics, structured logs, health, readiness, and dependency status.
    - Infrastructure level: Kubernetes nodes, pods, containers, ingress, optional service mesh, Redpanda, ClickHouse, PostgreSQL, object storage, and deployment controllers emit or expose CPU, memory, disk, network, restart, queue lag, storage saturation, and dependency health signals into the same system-observability route.
    - UI level: the frontend emits route transition spans, query/API call spans, Web Vitals, render/runtime errors, failed resource loads, user-visible error states, and critical interaction latency with tenant, project, environment, route, and build-version attributes where safe.
  - Files or modules expected to change: `spec/12-deployment.md` self-observability deployment component inventory and diagram, telemetry configuration helpers, Docker Compose and Helm values, service env docs, frontend telemetry initialization, infrastructure collector/exporter configuration, canary/smoke scripts, and focused tests proving the selected destination is used.
  - Out of scope: replacing the independent Prometheus scrape path from `spec/17-self-observability.md` §2.2. Keep health, readiness, and `/metrics` scraping as the failure-mode backstop even when OTLP self-telemetry is sent to a second instance.
  - Verification: unit/config tests cover `self` and `observer_instance` destination selection; Helm render tests expose the observer endpoint and credential references; frontend tests cover browser telemetry initialization without leaking secrets; collector/config tests cover infrastructure export wiring; smoke or local verification proves at least one service, one infrastructure source, and one UI path emit telemetry into the configured system tenant.
  - Checkpoint: can operators still see primary-platform health when the primary ingest, query, infrastructure, or UI-serving path is broken? Answer: yes for production-like deployments because platform telemetry is routed to a second observer instance, while self-ingest remains available for dogfooding.

- [x] **UI-Followup: Base UI primitive foundation**
  - Outcome: `apps/frontend` now includes Tailwind CSS v4 and an owned `src/components/ui` layer with shared `Button`, `Input`, `Select`, and `Tabs` primitives plus focused primitive tests. Existing theme preference behavior remains unchanged because the primitive styles read the current CSS variable contract.
  - Verification target: `npm run typecheck --workspace=apps/frontend`, `npm run lint --workspace=apps/frontend`, `npm run test --workspace=apps/frontend`, `npm run build --workspace=apps/frontend`, and `bash scripts/local-ci.sh`.
  - Checkpoint: can future screen migrations adopt the new primitive layer without reopening dependency and token setup? Answer: yes. The Vite/Tailwind pipeline, theme tokens, and owned primitive wrappers are now in place, so later UI slices can adopt them incrementally.

- [x] **P3-S6c: Add onboarding/setup flow for first-signal success**
  - Source spec: `spec/05-frontend.md` §9.3 Phase 1.
  - Outcome: a new operator can open `/setup` from the product shell, see the local OTLP HTTP trace ingest endpoint, copy the seeded local dev API key while only a redacted value is displayed, and validate first-signal arrival through existing trace/log/metric query APIs without leaving the UI. Completed 2026-04-26 as a frontend-only slice because the local dev tenant/API key seed and first-signal read contracts already existed.
  - Files or modules expected to change: onboarding route, setup panels, API key/config API if missing, first-signal validation API/client, tests.
  - Out of scope: fleet-wide agent management, remote config, or upgrade campaigns.
  - Verification: frontend tests cover setup route rendering, API key display/redaction behavior, copy behavior, and first-signal success/empty states. `tests/e2e/smoke_test_unit.sh` also asserts that the local CI smoke token and tenant ID match the Postgres migration seed for `dev-api-key-0000` and `00000000-0000-0000-0000-000000000001`. No new setup/status endpoint was added, so no API test was required for this slice.
  - Checkpoint: can a new tenant reach first-signal confirmation without reading internal docs or hand-assembling curl commands? Answer: yes for local development. The setup route exposes the seeded tenant ID, ingest endpoint, copyable local dev key, and first-signal status in the product shell; production tenant key creation remains a later admin/RBAC workflow.

- [x] **P3-S6d: Add a minimal threshold-alert UI workflow**
  - Source spec: `spec/05-frontend.md` §9.3 Phase 1.
  - Outcome: operators can list active threshold alerts, create one threshold rule for an existing metric, and silence or unsilence a rule from the UI.
  - Files or modules expected to change: alert API/client if missing, Alerts & SLOs route, rule list/form components, silence action, tests.
  - Out of scope: escalation routing, burn-rate/SLO authoring, incident post-mortem workflow, or composite alerts.
  - Verification: frontend tests cover rule create/list/silence flows; API tests cover threshold-rule CRUD or mutation shape and tenant/RBAC enforcement.
  - Checkpoint: does the UI expose one complete alert loop for threshold rules, not just backend evaluator state? Answer: yes. The `/alerts` page lists all threshold rules with live firing state, a create form submits POST /v1/alerts/rules, and per-row Silence/Unsilence buttons call PATCH .../silence. All three interactions are covered by frontend tests using fetch stubs and backend behavior is covered by Postgres Testcontainers integration tests.

- [x] **P3-S6e: Add explicit accessibility regression coverage for the trace waterfall and other major new views**
  - Source spec: `spec/05-frontend.md` §9.3 Phase 1 and the frontend slice standards in this plan's Operating Rules.
  - Outcome: the trace-detail waterfall has automated `playwright-axe` coverage, and the same harness is reusable for subsequent major views.
  - Files or modules expected to change: `tests/e2e/accessibility.spec.ts` or equivalent Playwright accessibility coverage, trace-detail test fixtures, and any minimal frontend semantics needed to remove violations.
  - Out of scope: a full visual-regression suite or exhaustive accessibility coverage for every placeholder route.
  - Verification: Playwright accessibility tests fail on a known violation and pass on the intended trace-detail and one additional major view.
  - Checkpoint: does the accessibility harness catch regressions on the Phase 1 waterfall without forcing every future slice to invent its own a11y test shape?
  - Outcome: `apps/frontend` now has a Playwright + axe-core harness under `e2e/accessibility.spec.ts`. The trace detail waterfall and log search views each have an axe scan; heading hierarchy (`<h2>`→`<h1>` in `TraceDetail.tsx`, `<h3>`→`<h2>` in `FacetSidebar.tsx`), landmark structure (`<section>`→`<div>` in `LogSearch.tsx`), and keyboard accessibility (`role="button"` + `tabIndex` + `onKeyDown` on span rows in `TraceDetail.tsx` and facet items in `FacetSidebar.tsx`) were fixed. A negative proof test injects an `image-alt` violation and asserts it is caught. `local-ci.sh` runs the suite when Chromium is installed and skips gracefully otherwise.
  - Checkpoint answer: yes. Future slices can call `new AxeBuilder({ page }).analyze()` after navigating to a new route, using `page.route()` to supply fixture data — no new harness setup needed.

- [x] **P3-S6f: Add modern UI foundation tokens and layout primitives**
  - Source spec: `spec/05-frontend.md` §9.2; `docs/superpowers/specs/2026-04-21-ui-design-guide.md`.
  - Outcome: `apps/frontend` includes modern surface/elevation tokens, an improved `cn` utility with `tailwind-merge`, and shared `Badge`, `Panel`, `Toolbar`, `MetricCard`, and `EmptyState` primitives. The app shell is modernized and the Services page is migrated to use the new primitives.
  - Verification: frontend primitive tests, app integration tests, Services accessibility coverage, frontend typecheck/lint/test/build, and `bash scripts/local-ci.sh` passed before PR #156 was merged.
  - Checkpoint: do new primitives follow the "inverted pyramid" density model while preserving the theme contract? Answer: yes. The shared primitives expose compact KPI, status, toolbar, panel, and empty-state patterns that keep service health scannable first while reading only the existing CSS variable theme contract.

### UI renovation gate

Before starting any new product UI workflow, complete these pure renovation slices. These slices should not add new backend contracts or product capabilities; they exist to finish the modernization started by P3-S6f and reduce the cost and inconsistency of all later UI work.

- [x] **UI-R1: Renovate service and infrastructure detail surfaces**
  - Source spec: `spec/05-frontend.md` §9.2 and §9.4; `docs/superpowers/specs/2026-04-21-ui-design-guide.md`.
  - Outcome: service detail, infrastructure detail, service infrastructure panel, deployment timeline container, and shared summary/status widgets use the modern `components/ui` primitives and token system instead of local metric/status/detail-panel variants and broad inline styles. Completed 2026-04-28.
  - Files or modules expected to change: `apps/frontend/src/pages/ServiceDetailPage.tsx`, `apps/frontend/src/pages/InfrastructureDetailPage.tsx`, `apps/frontend/src/components/ServiceInfraPanel.tsx`, `apps/frontend/src/components/DeploymentTimeline.tsx`, focused component tests, and accessibility coverage where the route already exists.
  - Out of scope: new service capabilities, topology behavior changes, alert authoring, dashboard creation, or backend API changes.
  - Verification: frontend tests cover renovated service and infrastructure states; accessibility coverage remains green for the touched views; frontend typecheck/lint/test/build pass.
  - Checkpoint: can operators move through service and infrastructure details without encountering legacy panel, status, or metric tile patterns? Answer: yes. UI-R1 target files now use `Panel`, `MetricCard`, `Badge`, and modern class tokens instead of legacy `detail-panel`, `metric-tile`, `signal-panel`, inline style, or old status patterns, with a focused regression test enforcing that contract.

- [ ] **UI-R2: Renovate explorer detail and log support surfaces**
  - Source spec: `spec/05-frontend.md` §9.2 and §9.4; `docs/superpowers/specs/2026-04-21-ui-design-guide.md`.
  - Outcome: trace detail waterfall, facet sidebar, log context/correlation/live-tail support components, and explorer result panels use modern primitives/tokens with minimal inline styles and consistent empty/loading/error states.
  - Files or modules expected to change: `apps/frontend/src/pages/TraceDetail.tsx`, `apps/frontend/src/components/FacetSidebar.tsx`, `apps/frontend/src/components/LogContextView.tsx`, `apps/frontend/src/components/LogCorrelatedList.tsx`, `apps/frontend/src/components/LogLiveTail.tsx`, `apps/frontend/src/pages/TraceSearch.tsx`, `apps/frontend/src/pages/LogSearch.tsx`, and related tests.
  - Out of scope: new query semantics, new facets, live-tail transport changes, or trace waterfall behavior redesign beyond preserving current interactions.
  - Verification: RTL tests cover selected/focused waterfall rows, facet interactions, and log support component states; accessibility scans remain green for trace detail and log search; frontend typecheck/lint/test/build pass.
  - Checkpoint: do trace and log investigation paths use the same modern primitives and state language as the service catalog?

- [ ] **UI-R3: Remove remaining legacy style drift and document the frontend migration rule**
  - Source spec: `spec/05-frontend.md` §9.2; `spec/15-frontend-local-dev.md`; `docs/superpowers/specs/2026-04-21-ui-design-guide.md`.
  - Outcome: remaining broad legacy CSS classes and inline style patterns are either migrated, intentionally quarantined for canvas/SVG-specific rendering, or documented as exceptions. Future frontend slices have an explicit rule: reuse `components/ui` primitives first, add missing primitives in the same slice only when needed, and avoid page-local interactive styling.
  - Files or modules expected to change: `apps/frontend/src/styles.css`, `apps/frontend/src/components/ui/*` if small missing primitives are needed, frontend tests for any new primitive, and this plan/spec note if the migration rule changes.
  - Out of scope: wholesale feature-directory reshuffle unless a touched surface can move without expanding review scope.
  - Verification: `rg` checks show no unreviewed inline-style-heavy product surfaces outside approved rendering exceptions; frontend typecheck/lint/test/build pass; accessibility suite remains green.
  - Checkpoint: is the frontend modernized enough that new product UI work can start without copying legacy local styles?

- [x] **P3-S7: Add field faceting and statistics to explorers**
  - Outcome: Log and Trace explorers show distribution of common fields such as status codes, log levels, and service names. This closes the immediate field-faceting gap recorded in `docs/analysis/2026-04-19-gaps-analysis.md`.
  - Files or modules expected to change: query-api facet responses if incomplete, explorer sidebar components, tests.
  - Out of scope: arbitrary high-cardinality analytics beyond Top N facets.
  - Verification: API tests cover facet counts; frontend tests cover facet rendering, selection, and query update behavior.
  - Checkpoint: does the UI correctly handle high-cardinality facets by showing Top N? Answer: yes. The backend groups by field, orders by count descending, and limits to Top 10.

- [x] **P3-S7b: Add a bounded query-substrate spike**
  - Source spec: `spec/03-storage.md` §5.4, §8; `spec/13-risks-roadmap.md` §24.5.
  - Outcome: one query-api read path is routed through an internal planner interface that can later host Arrow/DataFusion execution. If P3-S7 proves direct ClickHouse SQL is sufficient for the next three planned slices, record that decision and defer implementation to Phase 4.
  - Files or modules expected to change: query-api planner module, one read path or facet path, tests, Cargo dependencies only if the spike actually instantiates Arrow/DataFusion.
  - Out of scope: rewriting all query endpoints or adding federated query behavior across every signal.
  - Verification: existing endpoint behavior remains byte-compatible for the selected path; tests prove tenant context is still mandatory and cross-tenant rows still fail closed.
  - Checkpoint: does a planner abstraction reduce complexity for topology, dashboard, SLO, or semantic-query work, or should the project keep direct SQL until a harder requirement appears? Answer: The planner abstraction already helps by centralizing SQL generation. However, instantiating Arrow/DataFusion right now would add unnecessary complexity. We will keep direct SQL for now and defer full Arrow/DataFusion implementation to Phase 4 or when federated queries are required.

- [x] **P3-S8: Add Service Overview map from trace-derived topology**
  - Outcome: The Service Overview now renders a live topology map derived from trace spans. Operators can click a service node to enter focused mode (that service plus its direct neighbors), click the focused node again to return to the full graph, and click an edge to open a choice panel linking to Traces or Logs filtered to that caller-callee pair. The backend topology query now captures both direct parent-child call edges and trace-level co-occurrence edges.
  - Checkpoint: do topology rollups stay performant before broad graph work starts? Answer: yes for the ≤10-service scope of this slice. The UNION query runs over the same `spans` table as before, hits the same ClickHouse indices, and the outer deduplication GROUP BY is cheap. The existing perf-smoke baseline from P2-S9a covers query paths; no new threshold was needed. Spec alignment note: this slice closes the interaction contract, but `spec/05-frontend.md` §9.6 and ADR-016 still require a canvas-based rendering path before the service map can claim full scale alignment for larger graphs.

- [x] **P3-S9: Add Infrastructure inventory and detail views**
  - Source spec: `spec/05-frontend.md` §9.2.1 Infrastructure and §9.4 Infrastructure Correlation; `spec/09-api.md` Infrastructure Views.
  - Outcome: Infrastructure provides host, Kubernetes cluster, namespace, pod, and container inventory/detail views when resource attributes or catalog entities exist.
  - Files or modules expected to change: infrastructure inventory query path, frontend Infrastructure routes, inventory tables/detail panels, related service links, tests.
  - Out of scope: persistent infrastructure asset catalog independent of telemetry. Use telemetry/resource attributes first; catalog promotion can be a later domain-model slice.
  - Verification: API tests cover tenant-scoped infrastructure inventory; frontend tests cover inventory rendering, detail links, and empty states.
  - Checkpoint: can users move from infrastructure to related services, logs, metrics, and traces without manually reconstructing filters?

- [x] **P3-S10: Add infrastructure correlation from service and trace views**
  - Source spec: `spec/05-frontend.md` §9.4 Infrastructure Correlation.
  - Outcome: Service overview shows a correlated infrastructure panel via `listInfrastructure`; trace detail shows a deduplicated pill row of infra entities derived from span `resource_attributes`; log rows show inline infra badges. All three surfaces use a shared `infraLinks()` utility. Frontend-only — no backend changes. Completed 2026-04-25.
  - Files or modules expected to change: service detail related-infrastructure panel, trace/log detail links, shared resource-attribute link builder, tests.
  - Out of scope: infrastructure inventory implementation if P3-S9 is not complete. In that case links may target filtered explorer routes.
  - Verification: frontend tests cover generated links from `host.name`, `k8s.pod.name`, and container attributes; API tests cover resource attributes in responses where missing.
  - Checkpoint: are links derived correctly from OTel resource attributes? Answer: yes. `infraLinks()` maps `k8s.pod.name`, `host.name`/`host.id`, `k8s.namespace.name`, `k8s.cluster.name`, and `container.name`/`container.id` to `/infrastructure/:type/:id` URLs using `encodeURIComponent`, with 9 unit tests covering all mappings, fallbacks, deduplication, and URL encoding.

- [x] **P3-S11: Add deployment event ingestion and one timeline overlay**
  - Source spec: `spec/18-deployment-markers.md`.
  - Outcome: `deployment_markers` table (migration 009), `POST /v1/deployments` + `PATCH /v1/deployments/:id` in ingest-gateway (new PgPool connection; direct Postgres write for immediate consistency), `GET /v1/deployments` in query-api, `scripts/deployment-marker.sh` CI helper, `DeploymentTimeline` SVG component in service detail overview (10 unit tests). ADR-024 documents the ingest/query routing split and the future Redpanda `deployment.events` topic path that enables SSE push to the UI without polling. Completed 2026-04-26.
  - Checkpoint: is deployment identity clean enough for rollback analysis later? Answer: yes. `rollback_of` FK links a `rolled_back` deployment to the original; `status` enum tracks the full lifecycle. Ingest enrichment (§18.5 stamping `deployment_id` on span rows) and the Redpanda event-stream path (ADR-024 §Future) are explicitly deferred.

- [ ] **P3-S12: Add "Promote to Dashboard" from explorers**
  - Source spec: `spec/05-frontend.md` §9.4 Promote to Dashboard and §9.7.
  - Outcome: ad-hoc queries can be saved directly as new dashboard panels and viewed in one fixed-layout dashboard route with the selected time range and filters preserved.
  - Files or modules expected to change: dashboard config API if missing, explorer actions, dashboard serialization path, tests.
  - Out of scope: full drag-and-drop dashboard builder.
  - Verification: frontend tests cover promoted query payload, dashboard-route rendering, and preserved time range/filter state; API tests cover dashboard create/update shape.
  - Checkpoint: does the promoted panel preserve all filters and time range settings?

- [ ] **P3-S13: Add dashboard-as-code import/export for one dashboard shape**
  - Outcome: one dashboard can round-trip through API, storage, and UI
  - Checkpoint: is the serialized contract stable enough to support CI validation later?

- [ ] **P3-S14: Add Schema Registry with semantic annotations for one signal type**
  - Outcome: one signal type's fields have business-meaning annotations queryable via API; sets the grounding foundation required by the NL query layer (ADR-021). See `spec/03-storage.md §5.4.1`.
  - Checkpoint: are annotations stored structurally separate from schema shape so they can evolve independently of structural metadata?

- [ ] **P3-S15: Establish Testcontainers integration harness for real dependencies**
  - Source spec: `spec/11-testing.md §18.8`; ADR-025; implementation plan `docs/superpowers/plans/2026-04-27-testcontainers-integration-tests.md`.
  - Outcome: auth-service, query-api, and stream-processor have isolated Testcontainers tests for PostgreSQL, ClickHouse, and Redpanda boundaries, giving backend slices a narrow real-dependency regression path before Compose smoke.
  - Files or modules expected to change: service crate dev-dependencies, service-local `tests/*_integration.rs` files, narrowly exported repository or queue seams, and `scripts/local-ci.sh` only if a dedicated Testcontainers stage is required.
  - Out of scope: replacing Docker Compose smoke tests, replacing kind tests, adding object-storage tests before warm/cold retention work needs them, or adding broad shared fixtures before at least two services need the same helper.
  - Verification: focused `cargo test -p <service> --test <name> -- --nocapture` for each new suite, then `bash scripts/local-ci.sh` before push because this is a code-change slice.
  - Checkpoint: can backend agents verify real dependency behavior without starting the entire platform stack?

### Phase 3 pause point

Before Phase 4 starts, answer:
- Are services first-class entities now?
- Are cross-signal links precise enough to trust during incidents?
- Can operators navigate Services, Infrastructure, and Service Overview without manual filter reconstruction?
- Is the dashboard artifact shape stable enough to version?

---

## 7. Phase 4 — v1 Production Readiness

**Goal:** Make the product supportable for selected external customers.

**Entry gate:**
- Phase 3 exit gate passed
- correlation flows are stable under real traffic
- deployment and rollback automation already exists in basic form

**Exit gate:**
- v1 customers can be onboarded with documented support boundaries, restore paths, security posture, and test evidence

### Priority slice order

- [ ] **P4-S1: Add one warm-retention movement path**
  - Outcome: aged data moves from hot ClickHouse storage to one S3-compatible object-storage path without breaking queries for the selected dataset.
  - Closure steps: add local MinIO or equivalent S3-compatible storage, define object key layout and retention metadata for one signal, add one writer/export movement path, and document rollback/disable behavior.
  - Checkpoint: do query semantics stay stable across tiers?

- [ ] **P4-S2: Add backup and restore drill for one dataset**
  - Outcome: one restore path is practiced and timed, not merely specified.
  - Closure steps: include object-storage state in the backup boundary if P4-S1 has landed; otherwise explicitly record why the first drill is hot-store-only.
  - Checkpoint: are measured RPO/RTO values acceptable?

- [ ] **P4-S3: Add SSO/OIDC for one customer-compatible flow**
  - Outcome: one external identity provider can authenticate into the platform
  - Checkpoint: does this change require ADR/spec sync for auth scope or model?

- [ ] **P4-S4: Add fine-grained authorization for one protected resource**
  - Outcome: one OpenFGA-style protected object has enforceable sharing semantics
  - Checkpoint: is the ReBAC model additive to RBAC rather than conflicting with it?

- [ ] **P4-S5: Add SLO definition and one burn-rate alert**
  - Outcome: one service has a complete SLO workflow with alerting.
  - Closure steps: add the SLO definition model/API, reuse the Phase 2 threshold evaluator dispatch loop for an `slo_burn_rate` rule type, evaluate at least one multi-window burn-rate condition, and expose enough state for the frontend to show SLO health.
  - Checkpoint: are error budget semantics now reliable enough for customer use?

- [ ] **P4-S6: Add production runbook set for one failure class**
  - Outcome: one documented incident type has triage, rollback, and restore steps
  - Checkpoint: can an operator execute this without tribal knowledge?

- [ ] **P4-S7: Add tenant usage and cost report for one billing interval**
  - Outcome: operators can explain where ingest and storage cost went
  - Checkpoint: do we have enough signal to price or quota sanely?

- [ ] **P4-S8: Run load, chaos, tenant-escape, and upgrade/rollback suites**
  - Outcome: production-readiness claims are backed by repeatable evidence
  - Checkpoint: what failed, and does it block external support?

- [ ] **P4-S9: Complete boundary-focused security review**
  - Outcome: auth, tenancy, query, and ingest boundaries have explicit review notes
  - Checkpoint: are any findings severe enough to block v1?

### Phase 4 pause point

Before Phase 5 starts, answer:
- Could we support a real customer through an outage?
- Can we restore, roll back, and explain permissions cleanly?
- Do we have enough evidence to call the platform externally supportable?

---

## 8. Phase 5 — Reliability Product

**Goal:** Add the operator workflow layer for incidents, notification routing, and composite alerting.

**Entry gate:**
- Phase 4 exit gate passed
- SLO and alert foundations are stable

### Priority slice order

- [ ] **P5-S1: Add incident timeline for one alert source**
- [ ] **P5-S2: Add one notification routing integration**
- [ ] **P5-S3: Add runbook workflow attachment to an alert or incident**
- [ ] **P5-S4: Add topology-aware impact view for one incident**
- [ ] **P5-S5: Add composite alert evaluation for one rule pair**
- [ ] **P5-S6: Add reliability reporting for one team/service scope**

**Checkpoint question:** can responders complete detect → triage → notify → review inside the product for one real incident class?

---

## 9. Phase 6 — Advanced Telemetry

**Goal:** Add optional signal types without destabilizing the core platform.

**Entry gate:**
- Phase 5 workflows are stable
- retention, privacy, and cost controls are already proven

### Priority slice order

- [ ] **P6-S1: Add continuous profiling ingestion and one query path**
  - Prerequisite: object storage from P4-S1 exists, because `spec/03-storage.md` stores profile blobs in object storage.
- [ ] **P6-S2: Add browser RUM for one web app**
  - First slice guidance: start with standard OTel browser-compatible payloads plus explicit `session_id` attributes before introducing a custom RUM endpoint.
- [ ] **P6-S3: Add mobile signal ingestion for one SDK path**
- [ ] **P6-S4: Add one synthetic check workflow**
- [ ] **P6-S5: Add eBPF-assisted enrichment for one justified use case**
  - Prerequisite: complete a boundary-focused security review for privileged DaemonSet deployment, host access, tenant attribution, and rollback.
- [ ] **P6-S6: Add session replay only after privacy review passes**

**Checkpoint question:** does each new signal remain modular, governed, and optional?

---

## 10. Phase 7 — Enterprise Readiness

**Goal:** Add packaging, policy, and deployment controls required by enterprise buyers.

**Entry gate:**
- at least one target customer requirement set exists
- Phase 6 work has not destabilized core operations

### Priority slice order

- [ ] **P7-S1: Add regional residency controls for one region pair**
- [ ] **P7-S2: Add BYOK for one storage boundary**
- [ ] **P7-S3: Add tenant-isolated deployment packaging for one environment class**
- [ ] **P7-S4: Add compliance reporting for one framework**
- [ ] **P7-S5: Add metering export for one billing flow**
- [ ] **P7-S6: Add marketplace or private deployment packaging**

**Checkpoint question:** which enterprise items are truly customer-blocking now, and which stay deferred?

---

## 11. Phase 8 — Intelligence

**Goal:** Add explainable, auditable intelligence features on top of a stable platform.

**Entry gate:**
- historical retention is reliable
- labeling and auditability are good enough to explain model behavior
- AI remains advisory, never required for correctness

### Priority slice order

- [ ] **P8-S1: Add anomaly detection for one clearly bounded metric family**
- [ ] **P8-S2: Add query recommendations for one explorer view**
- [ ] **P8-S3: Add incident summarization with source links**
- [ ] **P8-S4: Add capacity forecasting for one storage or ingest dimension**
- [ ] **P8-S5: Add remediation hooks with explicit approval controls**
- [ ] **P8-S6: Add NL query layer for one explorer view using semantic annotations**
  - Outcome: operators can ask natural-language questions against one signal type and receive an explained, sourced answer grounded in semantic annotations from the Schema Registry (P3-S14). Governed by ADR-021 and within ADR-014 advisory-only, provenance-required, read-only constraints.
  - Checkpoint: does every response carry provenance (source queries, time range, signal type) and can it be ignored without affecting platform correctness?

**Checkpoint question:** can every AI output be explained, audited, and ignored without harming correctness?

---

## 12. Cross-Phase Review Rhythm

At the end of every 3–5 merged slices:
- review whether the phase exit gate is closer or just the diff count is growing
- prune stale backlog items
- promote only the next 3 slices into active planning
- re-check ADR/spec sync needs
- record newly discovered risks in `spec/13-risks-roadmap.md` only if they change roadmap scope

Do not keep a 50-slice active queue. Keep the active horizon short and the roadmap long.

---

## 13. Recommended Next Slice Right Now

After this planning reconciliation, the next implementation slice should be:

1. ~~P2-S1b: enforce tenant isolation for log query~~ (done)
2. ~~P2-S1c: enforce tenant isolation for metric query~~ (done)
3. ~~P2-S1d: assert tenant partition preservation in storage writes~~ (done)
4. ~~P2-S2a: add deterministic rate limiting for trace ingest~~ (done)
5. ~~P2-S5a: add audit logging for credential validation~~ (done)
6. ~~P2-S5b: add audit logging for query reads~~ (done)
7. ~~P2-S6a: add minimal RBAC distinction for one role pair~~ (done)
8. ~~P2-S3a: add cardinality budget observation for one signal~~ (done)
9. ~~P2-S4a: add hot retention policy for traces~~ (done)
10. ~~P2-S7a: add one threshold alert evaluation path~~ (done)
11. ~~P2-S8a: add Kubernetes manifest render and rollback skeleton~~ (done)
12. ~~P2-S8b: add one canary promotion path~~ (done)
13. ~~P2-S9a: add perf smoke baseline for ingest and common query paths~~ (done)
14. ~~P3-S10: Add infrastructure correlation from service and trace views~~ (done)
15. ~~P3-S6f: add modern UI foundation tokens and layout primitives~~ (done)
16. ~~P3-S6c: add onboarding/setup flow for first-signal success~~ (done)
17. ~~P3-S6d: add a minimal threshold-alert UI workflow~~ (done)
18. ~~P3-S6e: add explicit accessibility regression coverage for the trace waterfall and other major new views~~ (done)
19. P3-S12: add "Promote to Dashboard" from explorers and a fixed-layout dashboard route
20. P3-S13: add dashboard-as-code import/export for one dashboard shape
21. P3-S15: establish Testcontainers integration harness for real dependencies before the next backend slice touches PostgreSQL, ClickHouse, Redpanda, object storage, or OpenFGA
17. ~~P3-S6e: add explicit accessibility regression coverage for the trace waterfall and other major new views~~ (done)
18. ~~UI-R1: renovate service and infrastructure detail surfaces~~ (done)
19. UI-R2: renovate explorer detail and log support surfaces
20. UI-R3: remove remaining legacy style drift and document the frontend migration rule
21. P3-S6d: add a minimal threshold-alert UI workflow
22. P3-S12: add "Promote to Dashboard" from explorers and a fixed-layout dashboard route
23. P3-S13: add dashboard-as-code import/export for one dashboard shape
24. P3-S15: establish Testcontainers integration harness for real dependencies before the next backend slice touches PostgreSQL, ClickHouse, Redpanda, object storage, or OpenFGA

**Next recommended slice: UI-R2 - Renovate explorer detail and log support surfaces.**

**Phase 2 exit gate is now satisfied.** All Phase 2 slices (P2-S0 through P2-S9a) are complete. Before starting Phase 3, answer the Phase 2 pause-point questions:
- Tenant safety under test: yes — P2-S1a through P2-S1d enforce and test cross-tenant isolation for all signal types.
- Cost controls without hand-waving: yes — P2-S2a (rate limiting), P2-S3a (cardinality budget observation), P2-S4a (hot retention) are all in place.
- Roll back a bad deploy without manual heroics: yes — P2-S8a (Helm rollback skeleton) and P2-S8b (canary promotion path) cover both runtime and schema rollback.
- Self-observability route choice: use a second observer instance for production and customer-facing environments; use recursive self-ingest for local development, dogfooding, and bootstrap. This follows `spec/17-self-observability.md` by preserving both recursive OTLP telemetry and independent health/Prometheus monitoring, and it requires service-level, infrastructure-level, and UI-level instrumentation before the slice is complete.

The next recommended UI slices should complete the pure renovation gate before adding any
new product UI workflows:
- `UI-R1` service and infrastructure detail renovation
- `UI-R2` explorer detail and log support renovation
- `UI-R3` remaining style drift cleanup and migration rule documentation

After the renovation gate is complete, the product UI sequence should close the remaining
service-centric MVP bar before any further broad UI expansion:
- `P3-S6d` threshold-alert UI
- `P3-S12` dashboard workflow
- `P3-S13` dashboard-as-code round-trip

---

## 14. ADR/Spec Sync Note

No ADR update is included in this document. This plan decomposes the already-defined roadmap into an execution sequence. If future edits change roadmap scope, architecture, deployment model, data model, security model, or technology choice, update the relevant ADRs and specs in the same iteration.

P3-S5 added a concrete single-service summary endpoint for an existing Service Detail Summary capability. `spec/09-api.md` was updated with the route and current response fields. No ADR update is needed because the iteration does not change architecture, technology choice, deployment model, data model, security model, or roadmap scope.

**ADR-021** (NL query layer, Proposed — added 2026-04-19 via PR #53) introduces the NL query layer as a new Phase 8 feature and the Schema Registry semantic annotations as a Phase 3 prerequisite. P3-S14 and P8-S6 above reflect this. ADR-021 operates within the advisory-only, provenance-required, read-only constraints established by ADR-014.

The 2026-04-22 gap-analysis refresh updated planning sequence only. No ADR or spec update is required because the changes map already specified gaps to concrete slices without changing architecture, technology choice, deployment model, data model, security model, or roadmap scope.

The self-observability routing clarification also requires no ADR/spec update in this iteration because it restates the existing dual-path strategy in `spec/17-self-observability.md`: recursive in-band telemetry to a `system` tenant plus an independent out-of-band monitoring path. The plan recommendation is operational: use a second Observable instance for production-like environments and use self-ingest for local, dogfood, and bootstrap modes. The added instrumentation scope makes the implementation slice explicitly cover service, infrastructure, and UI levels without changing the underlying architecture.

**ADR-025** (Testcontainers for service integration tests, Proposed — added 2026-04-27) establishes Testcontainers as the mandatory narrow integration harness for backend slices that touch real containerized dependencies. This plan now includes P3-S15 as the implementation slice, while preserving Docker Compose smoke tests and kind tests as full-stack gates.
