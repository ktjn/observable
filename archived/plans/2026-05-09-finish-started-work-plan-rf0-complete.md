# Finish Started Work Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the roadmap slices that already have active or drafted implementation plans before opening additional broad roadmap lanes.

**Architecture:** Treat this document as the active short-horizon queue. Each workstream links to its detailed implementation plan, names the exact completion signal, and records the dependency that decides whether it can run now or must remain paused.

**Tech Stack:** Rust, Axum, SQLx/PostgreSQL, ClickHouse, Redpanda, Testcontainers, Docker Compose, Helm, React 19, Vite, TanStack Query, Base UI, Tailwind CSS v4, MSW, Playwright accessibility checks.

---

## Source Documents

- Active roadmap archive/reference: `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`
- Remaining-roadmap companion: `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`
- Process: `AGENTS.md`, `spec/10-process.md`
- v1 scope and near-term sequence: `spec/13-risks-roadmap.md`
- Current agent map: `docs/agent-context.md`

## Started Work Definition

A slice is in this plan when at least one of these is true:

1. A detailed implementation plan exists in `docs/superpowers/plans/`.
2. A detailed implementation plan exists in `archived/plans/` but the active roadmap still has an open or unclear item for the same capability.
3. A risk-remediation plan is already drafted and should be resolved before feature work can safely proceed.

Completed Phase 2 and Phase 3 slices stay in `2026-04-18-phases2-8-iteration-plan.md` as historical closure evidence; they are not repeated here.

## Execution Order

1. Finish **RF-0 Query API credential-bound tenancy**. Current query-api tenancy still trusts `X-Tenant-ID` directly, so tenant-isolation claims are not externally meaningful yet.
2. Finish **RF-1 OIDC/session hardening and plan reconciliation**. OIDC code exists in the repo, but the detailed plan remains unchecked and the implementation needs security review before it is treated as a completed identity slice.
3. Finish **RF-2 Regression gate restoration for integration tests**. `scripts/local-ci.sh` currently skips Rust integration tests in the normal Rust test stage, which conflicts with the Testcontainers harness being treated as a protected signal.
4. Finish **RF-3 NLQ SQL safety hardening**. Generated SQL still inlines LLM-sourced filters; string escaping is not enough for the current NLQ quality/security bar.
5. **RF-4 Alert lifecycle semantics is complete** on branch `feat/rf-4-alert-lifecycle-semantics`; keep this as closure evidence before P4-S5 burn-rate work.
6. Finish **RF-5 Deployment marker correlation closure** before release/rollback claims depend on deployment markers. Marker CRUD and UI overlays exist, but signal enrichment and canary marker automation are still missing.
7. Finish **RF-6 Self-observability endpoint closure** before production-readiness claims. Services expose health checks and OTLP telemetry, but the required Prometheus `/metrics` and readiness behavior are not uniformly implemented.
8. Finish **SW-1 P4-S5 SLO burn-rate**. This is the current value-first product lane and unlocks notification routing.
9. Finish **SW-2 P4-S1 warm retention** before any object-storage-backed feature, especially continuous profiling.
10. Finish **SW-3 P4-S3 OIDC/Zitadel** as the broader identity-provider slice after RF-1 determines what is already safely complete versus what must be rewritten.
11. Reconcile **SW-4 P8-S6b local LLM backend** before doing more AI setup work. It has an archived detailed plan and an active roadmap item, so the first task is to decide whether to revive, rewrite, or remove the active item.

Do not start a new broad roadmap slice from the remaining-roadmap companion until the next item above is either complete, explicitly paused with a recorded reason, or proven unrelated to the new slice.

---

## Review Findings Added 2026-05-07

These items come from a direct implementation review, not from the roadmap text alone. They are intentionally placed ahead of the existing finish-started queue because later work depends on their correctness.

### RF-0: Query API Credential-Bound Tenancy (COMPLETED)

**Completion signal: (COMPLETED)**
- [x] Query API accepts either a valid API-key credential or a valid OIDC session and derives tenant context from that credential.
- [x] `X-Tenant-ID` becomes a requested scope that must match an authorized tenant, not the source of truth.
- [x] Bootstrap endpoints are explicitly public only where required and are filtered when a user session is present.
- [x] HTTP integration tests cover missing credential, invalid credential, cross-tenant mismatch, valid API-key access, valid session access, and bootstrap behavior.

### RF-1: OIDC/Session Hardening And Plan Reconciliation

**Observed implementation:** OIDC/session code exists in `services/auth-service/src/oidc.rs`, `services/auth-service/src/session.rs`, `services/query-api/src/tenants.rs`, `apps/frontend/src/api/auth.ts`, and login/admin pages. The detailed identity plan `docs/superpowers/plans/2026-05-06-identity-provider-zitadel.md` still has unchecked tasks, so the plan state no longer matches the code state.

**Why this is incomplete or bad:** The current callback handler exchanges a code and uses PKCE, but the reviewed code does not validate the returned `state` against the `oauth_state` cookie before issuing a session. Session JWTs also are not bound to a specific `user_sessions.id`, so revocation semantics depend on a coarse user+tenant active-session lookup. Frontend identity pages use page-local inline styling and hardcoded provider/version text instead of the modern UI primitives required by the roadmap. This is too much partially landed identity surface to treat as merely "future work."

**Finish before:** External v1 onboarding, tenant-list filtering as a security boundary, Admin identity settings, and any broad SW-3 identity work.

**Completion signal: (COMPLETED)**
- [x] The identity detailed plan is reconciled: completed tasks checked via PR #274 feat/rf-1-oidc-session-hardening.
- [x] OIDC callback validates `state` and clears one-time cookies on success and failure. PR #274.
- [x] Session JWTs include a session identifier or equivalent nonce checked against active unrevoked `user_sessions`. PR #274.
- [x] Production cookies use secure attributes (`HttpOnly`, `SameSite`, `Secure`). PR #274.
- [x] Frontend login, callback, user menu, and identity settings use UI primitives with RTL/MSW. PR #274 + commit 021457b login.

**Required verification:**
- `cargo fmt --all`
- Auth-service unit and PostgreSQL Testcontainers tests for callback/session validation and revocation
- Query-api tests for session-filtered tenant listing
- Frontend auth tests
- Manual or scripted local Zitadel smoke if the slice claims end-to-end OIDC
- `bash scripts/local-ci.sh`

### RF-2: Regression Gate Restoration For Integration Tests

**Observed implementation:** `scripts/local-ci.sh` runs `cargo test --workspace --lib --bins` and comments that Testcontainers integration tests are skipped in the normal Rust test stage. The old plan text says the Testcontainers harness is complete and treated as a narrow real-dependency signal.

**Why this is incomplete or bad:** A protected regression signal is not protected if the required pre-push gate skips it by default. This is especially risky because AGENTS.md now requires backend dependency-boundary changes to add or update Testcontainers tests.

**Finish before:** Any backend slice that relies on Testcontainers as its replacement signal, including RF-0, RF-1, RF-3, P4-S5, and P4-S1.

**Completion signal: (COMPLETED)**
- [x] `scripts/local-ci.sh` has explicit "Rust integration tests" stage (cargo test --workspace --tests) when not --skip-docker. lines 98-99.
- [x] Stage "Rust integration tests", skipped if --skip-docker, matches AGENTS.md.
- [x] Skip flag --skip-docker explicit.
- [x] Local script aligned (no Dockerfile CI yet, but local gate protected).

**Required verification:**
- `bash -n scripts/local-ci.sh`
- Script-unit coverage if present
- One local run of the new narrow integration stage
- `bash scripts/local-ci.sh` or documented unavailable-tool skip with replacement signal

### RF-3: NLQ SQL Safety Hardening

**Observed implementation:** `services/query-api/src/sql_templates.rs` renders ClickHouse SQL strings with escaped values for filters, regexes, catalog fields, log queries, and trace/log helpers. Tests assert escaping, but generated SQL still inlines LLM-controlled values.

**Why this is incomplete or bad:** The NLQ pipeline is explicitly LLM-sourced and protected by an NLQ quality gate. Escaping string literals does not fully address unsafe regex payloads, numeric predicate coercion, unsupported field identifiers, expensive patterns, or future template expansion.

**Finish before:** P8-S7 PromQL facade, local LLM backend expansion, or any SLO/cost/security claim that relies on NLQ output being harmless.

**Completion signal: (IMPLEMENTED; live NLQ eval pending)**
- [x] LLM-sourced filters are validated by type and field allowlist before SQL generation.
- [x] Unsupported regexes, numeric predicates with non-numeric values, dangerous field names, and expensive query shapes fail closed with a user-visible 4xx response.
- [x] Tenant predicates remain server-injected and cannot be overridden by IR filters.
- [ ] NLQ eval cases cover accepted and rejected filters, and no previously passing eval case regresses.

**Required verification:**
- `cargo fmt --all`
- Focused sql-template and MCP/NLQ handler tests
- Updated `tests/nlq/cases.json`
- `python3 scripts/nlq-eval.py` against a running cluster, or a documented blocker if unavailable
- `bash scripts/local-ci.sh`

### RF-4: Alert Lifecycle Semantics Before Burn-Rate Alerts

**Observed implementation:** `services/alert-evaluator/src/evaluator.rs` reads unsilenced threshold rules and inserts `alert_firings` rows with `state = 'active'` when a threshold fires. The migration has `for_duration_secs` and `pending`/`resolved` states, but evaluator behavior does not enforce duration, deduplicate already-active firings, or resolve a firing when a condition clears.

**Why this is incomplete or bad:** P4-S5 burn-rate alerts would inherit noisy duplicate active firings and no clear recovery semantics. That makes SLO health unreliable for notification routing and incident timelines.

**Finish before:** SW-1 P4-S5 SLO burn-rate and P5-S2 notification routing.

**Completion signal: (COMPLETED)**
- [x] Alert evaluator enforces `for_duration_secs` as Pending -> Active. Implemented in branch `feat/rf-4-alert-lifecycle-semantics`.
- [x] Existing active firings are reused or updated instead of creating unbounded duplicates. Implemented in branch `feat/rf-4-alert-lifecycle-semantics`.
- [x] Conditions that clear move active/pending firings to `resolved` with a timestamp. Implemented in branch `feat/rf-4-alert-lifecycle-semantics`.
- [x] Query API and UI show active, pending, silenced, and resolved state clearly enough for the next SLO slice. Implemented in branch `feat/rf-4-alert-lifecycle-semantics`.

**Required verification:**
- `cargo fmt --all`
- Alert-evaluator unit tests for duration/dedupe/resolve
- PostgreSQL + ClickHouse Testcontainers coverage for evaluator lifecycle
- Frontend alert-state tests if UI changes
- `bash scripts/local-ci.sh`

### RF-5: Deployment Marker Correlation Closure

**Observed implementation:** Deployment marker CRUD and UI overlays exist, and `spec/18-deployment-markers.md` requires signal enrichment with the active `deployment_id`. Search evidence shows `deployment_id` is stored on spans, but no ingest path currently resolves active deployment markers and stamps `deployment_id`; `scripts/canary-promote.sh` marker automation is also still specified but not evidenced in the reviewed implementation.

**Why this is incomplete or bad:** The roadmap treats deployment correlation and rollback analysis as complete enough for later release safety, but the most valuable part, automatic signal-to-deployment correlation, remains missing.

**Finish before:** P4-S6 production runbooks, P4-S8 upgrade/rollback evidence, incident timelines that reference deploys, and any deployment-regression alerting.

**Completion signal: (COMPLETED)**
- [x] Ingest gateway resolves active/latest deployment marker by tenant, service, environment, and version and stamps `deployment_id` onto spans before storage. `services/ingest-gateway/src/deployment_registry.rs`.
- [x] Cache with 30 s TTL; on DB error returns empty string (fail-open). Stale TTL forces re-fetch. `deployment_registry.rs`.
- [x] `service.version` extracted from OTLP `service.version` resource attribute and stamped on spans. `http-json/convert.rs`.
- [x] Canary promotion creates deployment marker before deploy and updates to success/failed. `scripts/canary-promote.sh --marker-url`.
- [x] Testcontainers integration tests cover: active/success match, failed/rolled_back rejected, empty-version wildcard, tenant scoping, most-recent ordering. `services/ingest-gateway/tests/deployment_registry_integration.rs`.

**Required verification:**
- `cargo fmt --all` ✓
- 6 Testcontainers integration tests + 3 unit tests in `deployment_registry.rs`
- Canary script `bash -n` syntax check ✓
- `bash scripts/local-ci.sh --skip-docker --skip-helm --skip-frontend` ✓ (Docker unavailable; integration tests require Docker and run in `cargo test --workspace --tests`)

### RF-6: Self-Observability Endpoint Closure

**Observed implementation:** Services initialize shared OTLP/log telemetry and expose `/health`. Docker Compose has health checks. `spec/17-self-observability.md` also requires `/metrics` Prometheus endpoints and readiness behavior for services, but reviewed service routers expose only `/health` for several services.

**Why this is incomplete or bad:** A platform cannot claim production supportability or independent observer readiness without scrapeable service metrics and readiness/liveness semantics. OTLP self-ingest is useful, but it is not a replacement for out-of-band monitoring when the ingest path itself is degraded.

**Finish before:** P4-S6 production runbooks, P4-S8 readiness evidence, and any "second observer instance" production recommendation.

**Completion signal:**
- Every Rust service exposes `/health`, `/readyz`, and `/metrics` or the spec is narrowed with an ADR/spec update.
- Prometheus metrics include process/runtime basics and service-specific counters for request counts, errors, queue lag, dependency errors, and evaluator/worker cycles where applicable.
- Docker Compose and Helm probes use the correct liveness/readiness endpoints.
- The self-observability docs name which signals go through recursive OTLP versus independent scraping.

**Required verification:**
- `cargo fmt --all`
- Service-level HTTP tests for health/readiness/metrics endpoints
- Docker Compose config check
- Helm lint/render check
- `bash scripts/local-ci.sh`

---

## SW-0: Out-Of-Band Risk Remediation Superseded By RF Items

**Detailed plan:** `docs/superpowers/plans/2026-05-05-out-of-band-risk-remediation.md`

**Why this is started:** A complete risk-remediation plan exists and names concrete security, correctness, regression-gate, and governance issues found during a whole-repo scan.

**Current status:** Superseded as a top-level queue item by RF-0, RF-2, RF-3, and SW-4. Keep this plan as detail/reference until those RF items are either completed directly or the remediation plan is rewritten to match them.

**Finish before:** P4-S5 implementation if the tenant-auth or NLQ-safety findings still reproduce. P4-S5 adds tenant-scoped SLO APIs and alert behavior, so inherited tenant-context ambiguity is unacceptable.

**Completion signal:**
- Query API tenant context is bound to a valid credential, not only an arbitrary tenant header.
- Unsafe NLQ SQL predicate values are rejected before SQL generation or execution.
- Local CI has an explicit integration-test signal, with no weakened protected regression gates.
- Governance drift around the archived P8-S6b plan and agent entry files is closed or explicitly documented.

**Required verification:**
- Run the focused commands listed in `2026-05-05-out-of-band-risk-remediation.md`.
- Run `bash scripts/local-ci.sh` because this plan includes code and regression-gate changes.
- Run `git diff --check`.

**Missing or unclear items to resolve while finishing:**
- Confirm whether GitHub CI is intentionally disabled for the repository, and state the current replacement signal in the PR.
- Confirm whether the archived P8-S6b local LLM plan is superseded, revived, or replaced by a new plan.
- If `scripts/local-ci.sh` is changed, state the old coverage and new coverage in the PR before requesting review.

---

## SW-1: P4-S5 SLO Definition And Burn-Rate Alert

**Detailed plan:** `docs/superpowers/plans/2026-05-05-p4-s5-slo-burn-rate.md`

**Why this is started:** This is the active detailed implementation plan named by `docs/agent-context.md` before this split. It is also the next value-first slice in the old Phases 2-8 roadmap.

**Finish before:** P5-S2 notification routing. Notifications need a meaningful active alert source; the burn-rate firing path supplies that source.

**Completion signal:**
- One tenant can create or read a service-level availability SLO.
- `alert-evaluator` evaluates an `slo_burn_rate` rule using fast and slow ClickHouse span windows.
- Active burn-rate firings are written through the existing alert-firing model.
- Alerts UI shows SLO health state and can create the first availability SLO.
- The old active roadmap marks P4-S5 complete and points the next detailed plan at P5-S2.

**Required verification:**
- `cargo test -p query-api --test postgres_slos_integration -- --nocapture`
- `cargo test -p query-api --test http_api_integration post_slo -- --nocapture`
- `cargo test -p alert-evaluator burn_rate`
- `cargo test -p alert-evaluator --test slo_burn_rate_integration -- --nocapture`
- `cd apps/frontend && npm test -- Alerts`
- `bash scripts/local-ci.sh`

**Missing or unclear items to resolve while finishing:**
- The detailed plan says the next smallest slice is P5-S2, while the old value-first list names P5-S2 after P4-S5 but also lists profiling/RUM soon after. Keep P5-S2 as the immediate next detailed plan unless a reviewer explicitly reprioritizes.
- The detailed plan only covers availability SLOs. Latency and throughput SLOs remain separate future slices.
- The burn-rate formula and SLO fields are treated as existing spec scope. If implementation changes either, update `spec/07-alerting-slo.md`, `spec/14-domain-model.md`, and any relevant ADR in the same PR.

---

## SW-2: P4-S1 Warm Retention Movement Path

**Detailed plan:** `docs/superpowers/plans/2026-05-05-p4-s1-warm-retention.md`

**Why this is started:** A full detailed implementation plan exists, but the old roadmap deferred it during the value-first reorder.

**Finish before:** P6-S1 continuous profiling ingestion, because `spec/03-storage.md` stores profile blobs in object storage and the roadmap explicitly names P4-S1 as the prerequisite.

**Completion signal:**
- Aged span rows can be exported from hot ClickHouse storage to a deterministic S3-compatible object key.
- The exported object can be read back in a Testcontainers integration test.
- Existing hot ClickHouse query results remain unchanged.
- The worker is disabled by default or has an explicit safe local-dev configuration.
- The old active roadmap marks P4-S1 complete only after code and verification land.

**Required verification:**
- Use the focused Rust and Docker Compose checks in `2026-05-05-p4-s1-warm-retention.md`.
- Run `docker compose config --quiet` after local MinIO or equivalent Compose changes.
- Run `bash scripts/local-ci.sh` before push.

**Missing or unclear items to resolve while finishing:**
- The detailed plan says to check dependency versions. Do that immediately before implementation, not from the 2026-05-05 snapshot.
- The plan must state whether query federation across hot/warm tiers is still out of scope. The current target is copy-first export without changing query results.
- If Compose changes add object storage, update `docs/agent-context.md` with the new local-dev dependency and startup gotcha.

---

## SW-3: P4-S3 Identity Provider Integration With Zitadel

**Detailed plan:** `docs/superpowers/plans/2026-05-06-identity-provider-zitadel.md`

**Why this is started:** A detailed OIDC/Zitadel implementation plan exists and expands the old P4-S3 roadmap item.

**Finish before:** External v1 customer onboarding, tenant-list filtering by authenticated principal, SCIM provisioning, or fine-grained authorization work that assumes human user sessions.

**Completion signal:**
- Zitadel 2.x is available in local development.
- Auth service supports OIDC authorization-code-with-PKCE login, callback, logout, `/me`, and internal session validation.
- User, identity, session, and tenant-role state is stored in PostgreSQL with audit events.
- Query API tenant bootstrap endpoints are filtered by authenticated user where required.
- Frontend shows authenticated user state, login/callback/logout behavior, and an Admin identity settings view.
- API-key validation still works for ingest and internal service paths.

**Required verification:**
- Use the verification checklist in `2026-05-06-identity-provider-zitadel.md`.
- Add or update HTTP integration tests for every new handler path.
- Add PostgreSQL Testcontainers coverage for identity/session persistence.
- Run frontend MSW/RTL tests for login state, session expiry, tenant picker filtering, and Admin identity settings.
- Run `bash scripts/local-ci.sh` before push.

**Missing or unclear items to resolve while finishing:**
- The plan contains version placeholders for Zitadel Helm chart values. Check the current stable chart and image versions immediately before adding dependencies or image pins.
- The setup script section includes assumptions about how the local Zitadel first-instance token is obtained. Validate the exact flow in local Compose before treating it as executable.
- SCIM remains conditional P4-S3b work. Do not include SCIM endpoints in this slice unless a selected v1 customer requires them.

---

## SW-4: P8-S6b Local LLM Backend Reconciliation

**Detailed plan reference:** `archived/plans/2026-04-29-p8-s6b-local-llm-vllm.md`

**Why this is started:** The old active roadmap still has an unchecked P8-S6b item that links to an archived detailed plan. That is planning drift and must be resolved before more AI setup work starts.

**Finish before:** PromQL compatibility, additional NLQ backend configuration, Ollama support, streaming NLQ responses, or any Setup UI work that changes LLM backend settings.

**Completion signal:**
- The active roadmap clearly states whether P8-S6b is active, deferred, superseded, or replaced.
- If active, a non-archived detailed plan exists in `docs/superpowers/plans/` with current dependency versions and current code paths.
- If superseded, the old active item links to the replacement plan or is moved to the remaining-roadmap companion with a clear reason.

**Required verification:**
- Documentation-only reconciliation is exempt from `bash scripts/local-ci.sh`.
- Run `git diff --check`.
- If implementation begins, apply the NLQ quality gate from `AGENTS.md` and `spec/08-ai-ml.md`.

**Missing or unclear items to resolve while finishing:**
- The archived plan predates later NLQ hardening and governance notes. Re-check ADR-014, ADR-021, ADR-026, ADR-027, and `spec/08-ai-ml.md` before reviving it.
- Confirm whether the product still wants vLLM specifically or a generic OpenAI-compatible local backend setting.

---

## Finish Started Work Exit Gate

This plan is complete when:

- SW-0 through SW-4 are complete or explicitly deferred with a dated reason and owner in the PR.
- `docs/agent-context.md` points to the next active detailed implementation plan.
- The remaining-roadmap companion has the next unstarted slice promoted with clear prerequisites.
- No active item points only to an archived detailed plan without an explanation.
- Documentation review passes, including no unresolved placeholder markers.
