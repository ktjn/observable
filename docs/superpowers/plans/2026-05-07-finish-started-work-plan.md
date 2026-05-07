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

1. Finish **SW-0 Out-of-band risk remediation** if any of its findings still reproduce locally. It protects auth tenancy, NLQ SQL safety, local gate coverage, and governance drift.
2. Finish **SW-1 P4-S5 SLO burn-rate**. This is the current value-first product lane and unlocks notification routing.
3. Finish **SW-2 P4-S1 warm retention** before any object-storage-backed feature, especially continuous profiling.
4. Finish **SW-3 P4-S3 OIDC/Zitadel** before external v1 customer onboarding or any user-session-based tenant filtering work.
5. Reconcile **SW-4 P8-S6b local LLM backend** before doing more AI setup work. It has an archived detailed plan and an active roadmap item, so the first task is to decide whether to revive, rewrite, or remove the active item.

Do not start a new broad roadmap slice from the remaining-roadmap companion until the next item above is either complete, explicitly paused with a recorded reason, or proven unrelated to the new slice.

---

## SW-0: Out-Of-Band Risk Remediation

**Detailed plan:** `docs/superpowers/plans/2026-05-05-out-of-band-risk-remediation.md`

**Why this is started:** A complete risk-remediation plan exists and names concrete security, correctness, regression-gate, and governance issues found during a whole-repo scan.

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
