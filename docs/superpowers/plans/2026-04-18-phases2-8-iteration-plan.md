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
6. update this plan document as part of the PR's definition of done:
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
  - Outcome: Phase 2 has measurable performance baselines instead of assumptions. `scripts/perf-smoke.sh` seeds one trace/log/metric, waits for the pipeline, then samples each ingest and query endpoint 20 times. It reports P50 and P95 per path and exits non-zero if any path exceeds its threshold (ingest P50 < 500 ms / P95 < 1000 ms; query P50 < 1000 ms / P95 < 3000 ms per spec/11-testing.md §18.3). A `perf-smoke` Docker Compose service runs the script against the live stack. The nightly CI workflow (`.github/workflows/nightly.yml`) runs `perf-smoke` after the existing smoke-test step. Thresholds are overridable via env vars.
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

- [x] **P3-S1: Add trace-to-log correlation for logs with full trace context**
  - Outcome: a trace detail view can fetch exact correlated log lines. The `query-api` now supports `trace_id` and `span_id` filters for logs, and the frontend `TraceDetail` view displays these logs, allowing for span-level filtering.
  - Checkpoint: are joins based on canonical IDs only, with no fuzzy heuristics yet? Answer: yes. Joins use exact equality on `trace_id` and `span_id` in ClickHouse.

- [x] **P3-S1b: Add log-context (surrounding logs) capability**
  - Outcome: operators can view logs occurring before and after a specific log line for the same host/service. The `query-api` now supports `GET /v1/logs/:log_id/context`, and the frontend allows clicking a log line to see its surrounding context.
  - Checkpoint: does the context view correctly ignore search filters while preserving tenant and host/service scope? Answer: yes. The context query uses `service_name` and `host_id` from the pivot log and ignores any other search parameters.

- [ ] **P3-S1c: Add live tail capability for logs**
  - Outcome: real-time streaming of logs in the explorer with auto-scroll
  - Checkpoint: is the end-to-end latency from ingest to UI display < 2s?

- [ ] **P3-S2: Add trace-level log correlation when `span_id` is absent**
  - Outcome: trace views show trace-correlated logs without claiming exact span linkage
  - Checkpoint: is the UI language precise about exact vs trace-level correlation?

- [ ] **P3-S2b: Add rate limiting for log ingest**
  - Outcome: one authenticated tenant exceeding a log-ingest request budget gets a stable `429` rejection. Mirrors the pattern from P2-S2a for traces.
  - Checkpoint: is the rate-limit response shape (status code, error body, `Retry-After`, warn log) identical to the trace path so operators face a consistent contract?

- [ ] **P3-S2c: Add rate limiting for metric ingest**
  - Outcome: one authenticated tenant exceeding a metric-ingest request budget gets a stable `429` rejection. Mirrors the pattern from P2-S2a and P3-S2b.
  - Checkpoint: are all three signal ingest paths (traces, logs, metrics) now covered by rate limiting?

- [ ] **P3-S3: Build a minimal service catalog from resource attributes**
  - Outcome: services appear as navigable entities with stable IDs
  - Checkpoint: do we have a durable service identity model or are we still overfitting to labels?

- [ ] **P3-S4: Add RED metric derivation for one service**
  - Outcome: one service detail page shows request rate, errors, and duration from spans
  - Checkpoint: is the derived metric contract good enough for alerting reuse?

- [ ] **P3-S5: Add deployment event ingestion and one timeline overlay**
  - Outcome: traces or metrics can be viewed against deploy events
  - Checkpoint: is deployment identity clean enough for rollback analysis later?

- [ ] **P3-S6: Add a focused service map view**
  - Outcome: one service and its direct edges can be rendered from trace data
  - Checkpoint: do topology rollups stay performant before broad graph work starts?

- [ ] **P3-S7: Add deep links across trace, log, metric, and service views**
  - Outcome: context survives reload and cross-navigation
  - Checkpoint: are URLs now the source of truth for investigation context?

- [ ] **P3-S7b: Add field faceting and statistics to explorers**
  - Outcome: Log and Trace explorers show distribution of common fields (e.g. status codes, log levels)
  - Checkpoint: does the UI correctly handle high-cardinality facets by showing Top N?

- [ ] **P3-S7c: Add "Promote to Dashboard" from explorers**
  - Outcome: ad-hoc queries can be saved directly as new dashboard panels
  - Checkpoint: does the promoted panel preserve all filters and time range settings?

- [ ] **P3-S8: Add dashboard-as-code import/export for one dashboard shape**
  - Outcome: one dashboard can round-trip through API, storage, and UI
  - Checkpoint: is the serialized contract stable enough to support CI validation later?

- [ ] **P3-S9: Add infrastructure correlation from service and trace views**
  - Outcome: users can navigate from a service or trace to correlated host/pod/container metrics and logs
  - Checkpoint: are links derived correctly from OTel resource attributes?

- [ ] **P3-S10: Add Schema Registry with semantic annotations for one signal type**
  - Outcome: one signal type's fields have business-meaning annotations queryable via API; sets the grounding foundation required by the NL query layer (ADR-021). See `spec/03-storage.md §5.4.1`.
  - Checkpoint: are annotations stored structurally separate from schema shape so they can evolve independently of structural metadata?

### Phase 3 pause point

Before Phase 4 starts, answer:
- Are services first-class entities now?
- Are cross-signal links precise enough to trust during incidents?
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
  - Outcome: aged data moves from hot to warm storage without breaking queries
  - Checkpoint: do query semantics stay stable across tiers?

- [ ] **P4-S2: Add backup and restore drill for one dataset**
  - Outcome: one restore path is practiced and timed, not merely specified
  - Checkpoint: are measured RPO/RTO values acceptable?

- [ ] **P4-S3: Add SSO/OIDC for one customer-compatible flow**
  - Outcome: one external identity provider can authenticate into the platform
  - Checkpoint: does this change require ADR/spec sync for auth scope or model?

- [ ] **P4-S4: Add fine-grained authorization for one protected resource**
  - Outcome: one OpenFGA-style protected object has enforceable sharing semantics
  - Checkpoint: is the ReBAC model additive to RBAC rather than conflicting with it?

- [ ] **P4-S5: Add SLO definition and one burn-rate alert**
  - Outcome: one service has a complete SLO workflow with alerting
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
- [ ] **P6-S2: Add browser RUM for one web app**
- [ ] **P6-S3: Add mobile signal ingestion for one SDK path**
- [ ] **P6-S4: Add one synthetic check workflow**
- [ ] **P6-S5: Add eBPF-assisted enrichment for one justified use case**
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
  - Outcome: operators can ask natural-language questions against one signal type and receive an explained, sourced answer grounded in semantic annotations from the Schema Registry (P3-S10). Governed by ADR-021 and within ADR-014 advisory-only, provenance-required, read-only constraints.
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

**Phase 2 exit gate is now satisfied.** All Phase 2 slices (P2-S0 through P2-S9a) are complete. Before starting Phase 3, answer the Phase 2 pause-point questions:
- Tenant safety under test: yes — P2-S1a through P2-S1d enforce and test cross-tenant isolation for all signal types.
- Cost controls without hand-waving: yes — P2-S2a (rate limiting), P2-S3a (cardinality budget observation), P2-S4a (hot retention) are all in place.
- Roll back a bad deploy without manual heroics: yes — P2-S8a (Helm rollback skeleton) and P2-S8b (canary promotion path) cover both runtime and schema rollback.

**Next recommended slice: P3-S1c — Add live tail capability for logs.**

---

## 14. ADR/Spec Sync Note

No ADR update is included in this document. This plan decomposes the already-defined roadmap into an execution sequence. If future edits change roadmap scope, architecture, deployment model, data model, security model, or technology choice, update the relevant ADRs and specs in the same iteration.

**ADR-021** (NL query layer, Proposed — added 2026-04-19 via PR #53) introduces the NL query layer as a new Phase 8 feature and the Schema Registry semantic annotations as a Phase 3 prerequisite. P3-S10 and P8-S6 above reflect this. ADR-021 operates within the advisory-only, provenance-required, read-only constraints established by ADR-014.
