# Repo Analysis — 2026-07-02

**Date:** 2026-07-02
**Status:** Initial review
**Scope:** Whole-repo, code-level survey for security, self-observability, test-coverage, and
hygiene gaps — reading actual implementation files, not re-deriving the existing competitive
feature backlog.
**Baseline:** Active roadmap is `docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md`
(unified feature roadmap). Admin-service extraction (ADR-033) complete, all 3 slices. Playwright
visual suite, uv migration, and the 2026-06-26 UI usability remediation are complete.

This report complements `docs/analysis/2026-05-01-repo-review.md` (still worth reading — two of
its low-priority items are confirmed still open below) and the 2026-06-19 service-layer
architecture review. It focuses on concrete, file/line-verified findings rather than feature-parity
brainstorming, which the active roadmap already covers well (it already lists 4 gap items dated
2026-07-02 — Service Topology Endpoint, Log Pattern Clustering, Notebooks, SLO Burn Rate Alerting).

All findings below have been promoted into `docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md`
§3.6 "Security, Observability & Reliability Hardening". This document is the supporting evidence.

---

## 1. High-priority (security)

### 1.1 Hardcoded default session-signing secret ships in the Helm chart

- `charts/observable/values.yaml:270` — `identity.sessionSecret: "dev-session-secret-for-observable-00"`
- `charts/observable/templates/auth-service.yaml:18` — passes it straight through as `SESSION_SECRET`
- `services/auth-service/src/main.rs:88` — also falls back to a hardcoded literal
  (`"dev-session-secret-change-in-prod!!"`) if `SESSION_SECRET` is unset at all
- `services/auth-service/src/session.rs:46,54` — the HS256 sign/verify path that consumes it

Anyone who deploys the public Helm chart without explicitly overriding `identity.sessionSecret`,
or who reads this open-source repo, has the exact HS256 signing key used by default installs. That
key can forge a valid session JWT for **any** `tenant_id`/`role`/`user_id`, fully bypassing
authentication and tenant isolation.

**Recommendation:** Fail closed. `auth-service` should refuse to start if `SESSION_SECRET` is
unset in a non-dev environment (e.g. gate on an explicit `ENVIRONMENT=development` flag), and the
Helm chart should either require the value (no default) or generate+persist a random secret at
install time (e.g. via a `lookup` against an existing Secret, falling back to `randAlphaNum` only
on first install).

### 1.2 The entire OIDC login/callback/session flow is untested

`services/auth-service/src/oidc.rs` (573 lines: PKCE challenge/verifier generation, CSRF `state`
check at line 212, authorization-code token exchange at line 228, userinfo fetch at line 266,
`sub`/tenant-role resolution, session issuance) has **no `#[cfg(test)]` module at all**.
`services/auth-service/src/session.rs` (JWT sign/verify, PKCE helpers) is likewise untested.

This is the highest-value attack surface in the codebase — the code that decides whether a login
attempt is legitimate — with zero automated regression coverage.

**Recommendation:** Add unit tests for PKCE generation/verification, CSRF state mismatch
rejection, and session-JWT round-trip (sign → verify → claims), plus an integration test for the
full callback handler using a stubbed OIDC provider response.

### 1.3 Tenant-scoping/authorization middleware has zero tests in two services

`services/query-api/src/middleware/auth.rs` and `services/admin-service/src/middleware/auth.rs`
(both ~120–160 lines) contain the API-key-vs-session dispatch and the cross-tenant-switch access
check (`query-api/src/middleware/auth.rs:75-113`, checking `user_tenant_roles`) — the core
authorization gate for every request in both services. Neither file has a test module.

A regression here (e.g. an inverted `has_access` comparison, or a missing `AND tenant_id = $1`)
would let a user read or mutate another tenant's data with nothing in CI to catch it.

**Recommendation:** Add table-driven tests covering: valid same-tenant access, cross-tenant access
without a role grant (must reject), cross-tenant access with a role grant (must allow), expired
session, and malformed/missing API key.

### 1.4 Panic on DB row parsing in the login path

`services/auth-service/src/oidc.rs:123-124`, `list_user_tenants`:

```rust
let tid: Uuid = r.try_get("tenant_id").unwrap();
let role: String = r.try_get("role").unwrap();
```

Runs on every row for every user login. A schema drift (nullable column, migration mismatch)
panics the request task instead of surfacing a typed error. Low probability, but on the login
critical path.

**Recommendation:** Propagate via `?` into the function's existing `anyhow::Result`/`sqlx::Error`
return type instead of `.unwrap()`.

### 1.5 Dead SQL-building functions retain manual string escaping instead of parameterization

`services/query-api/src/sql_templates.rs:664` (`build_filter_expr`), `:743`
(`build_filter_clauses`), `:824`, and `services/query-api/src/logs.rs:344,363`
(`build_log_filter_clauses`, a `fetch_log_rows` variant) are all marked `#[allow(dead_code)]` and
documented as "kept for reference" — the live code path uses the safer `_checked` variants
(parameterized / allowlisted).

Leaving a working-but-unsafe SQL builder in the tree, rather than deleting it, is a landmine: nothing
stops a future contributor (or an agent) from reasonably calling the unchecked version and silently
reintroducing the injection class that ADR-history and `archived/plans/2026-05-26-p4-s9-boundary-security-review.md`
already fixed once.

**Recommendation:** Delete the dead functions. If they're genuinely useful as documentation, move
the pattern into a code comment instead of live (if unreachable) code.

---

## 2. High-priority (self-observability)

### 2.1 Three of seven Rust services have no `/metrics` endpoint

Verified via `route("/metrics", ...)` search across `services/*/src`:

| Service | `/health` | `/readyz` | `/metrics` |
|---|---|---|---|
| query-api | yes | yes | yes |
| storage-writer | yes | yes | yes |
| auth-service | yes | yes | yes |
| admin-service | yes | yes | yes |
| **alert-evaluator** | yes | yes | **no** |
| **ingest-gateway** | yes | yes | **no** |
| **stream-processor** | yes | yes | **no** |

`alert-evaluator`'s `main.rs:44-53` registers only `/health` and `/readyz`; its
`start_eval_worker`/`notification_worker` background loops (the actual alert-firing logic) emit no
metrics at all. `ingest-gateway` and `stream-processor` are the same — no `prometheus::Registry`
anywhere in either crate (confirmed by grep). This means the platform's own dogfooding dashboards
(RF-6 self-observability work) have no visibility into alert-evaluation latency/rule counts,
ingest throughput/rejection rates, or stream-processor batching behavior — for an observability
product, this is a notable gap in eating your own dog food.

Note: the existing roadmap's "shared observable-observability crate" item (§7 Deferred) describes
this scaffolding as already present on "auth-service, storage-writer, query-api, ingest-gateway,
alert-evaluator, and stream-processor" — that list is factually wrong for the last three; they
have no `/metrics` to extract a shared crate *from* yet. The roadmap text has been corrected as
part of this review's changes.

**Recommendation:** Add a minimal `observability.rs` (`http_requests_total`,
`http_request_duration_seconds`, plus one domain metric per service — e.g. `alert_eval_duration_seconds`,
`ingest_rejected_total`, `stream_batch_size`) to each of the three services. This is also the
natural first real consumer that would validate the planned shared `observable-observability`
crate's design before extracting it.

---

## 3. Medium-priority (performance)

### 3.1 New `reqwest::Client` constructed per request in the auth hot path

- `services/query-api/src/middleware/auth.rs:121,143`
- `services/admin-service/src/middleware/auth.rs:121,143`

Both call `reqwest::Client::new()` inside `validate_session`/`verify_credentials`, which run on
**every** request that hits `require_tenant` — i.e. every authenticated API call in both services.
Each call builds a fresh connection pool with no keep-alive reuse to `auth-service`, adding
TCP/TLS handshake overhead per request under load. Same pattern (once per login/lookup, less
severe since off the hot path) in `services/auth-service/src/oidc.rs:228,266` and
`services/query-api/src/tenants.rs:178`.

**Recommendation:** Hoist a single `Arc<reqwest::Client>` into each service's `AppState` and reuse
it. Mechanical, low-risk change.

---

## 4. Medium-priority (test coverage)

### 4.1 Privilege-mutating admin-service handlers are untested

`services/admin-service/src/tokens.rs` (311 lines — API token issuance/revocation),
`admin_members.rs` (287 lines — member add/remove/re-role), and `usage.rs` (230 lines) have no
`#[cfg(test)]` module, unlike most of `query-api`/`ingest-gateway`. These are privilege-sensitive
mutation endpoints with no regression safety net.

### 4.2 Shared test-support crate has no tests of its own

`libs/test-support/src/{lib.rs,postgres.rs,clickhouse.rs}` (194 lines total) — the Testcontainers
fixture/helper crate consumed by every other service's integration test suite — has zero tests. A
subtle bug here (e.g. a fixture that silently reuses a stale container, or a schema-migration
helper that swallows an error) would quietly weaken every consuming service's test suite at once.

### 4.3 Inconsistent frontend test coverage in the admin feature area

`apps/frontend/src/features/admin/MemberManagementPage.tsx` has a matching
`MemberManagementPage.test.tsx`; sibling files in the same folder —
`BillingReportPage.tsx` (260 lines, renders billing/usage data) and `FleetManagementPage.tsx`
(28 lines) — have none.

**Recommendation (all three):** Not urgent enough to block other work, but worth a dedicated small
slice per area given these guard privilege-granting and billing-adjacent surfaces.

---

## 5. Low-priority (hygiene / doc drift)

### 5.1 `AGENTS.md` still claims GitHub CI is disabled

`AGENTS.md:135` — *"You **MUST** run `bash scripts/local-ci.sh` before pushing **ANY** code
changes. No exceptions. GitHub CI is disabled — do not push and rely on it to catch errors."*

This was accurate as of the 2026-05-01 review (`.github/workflows/build.yml` had all triggers
commented out except `workflow_dispatch`). It is no longer true: `build.yml` today runs real
`pull_request`/`push`/`schedule`-triggered jobs (`changes`, `backend`, `frontend`, `helm`, `smoke`,
`perf-smoke`), with SHA-pinned actions kept current by `.github/workflows/renovate.yml` (recent
history shows real Renovate-authored dependency-bump commits merged to `main`). Agents reading
`AGENTS.md` are being told to disregard a CI signal that actually runs and would catch real
regressions (including on schedule and via `workflow_dispatch` for the surfaces a given PR didn't
touch).

**Recommendation:** Update the sentence to describe the current state — CI runs on PR/push for
changed surfaces (via `scripts/ci-changed-surfaces.sh`) plus nightly full-matrix — while keeping
the "run `local-ci.sh` before pushing" mandate (a 20+ minute cloud CI round-trip is still worse
than a local check).

### 5.2 `scripts/nlq-multi-model.py` is still undocumented (open since 2026-05-01)

Flagged in `docs/analysis/2026-05-01-repo-review.md` §1.4, still true two months later: the script
exists and works, but is mentioned in none of `spec/08-ai-ml.md`, `spec/10-process.md`'s NLQ
Quality Gate section, `AGENTS.md`, or `README.md`. Carried forward rather than re-analyzed.

### 5.3 `tests/nlq/cases.json` still has no schema/version marker (open since 2026-05-01)

Flagged in the same prior review §3.2, still true: still a bare JSON array, now at 38 cases (up
from 27), with no `schema_version` field or changelog. Low priority, carried forward.

### 5.4 `scripts/local-ci.sh` leaks temp directories on modelable-compile failure

Lines ~65-83: `TMP_TS`/`TMP_RS` are created via `mktemp -d`, then
`uv run ... modelable compile ... || fail "modelable compile (...)"`. `fail()` calls `exit`
immediately (see the function definition near the top of the script), bypassing the `rm -rf
"$TMP_TS" "$TMP_RS"` cleanup that only runs after the diff loop completes. Repeated local CI
failures at this step accumulate orphaned temp directories under the OS temp dir. Harmless, but
inconsistent with the `trap`-based cleanup pattern the same script already uses for
`docker compose down`.

**Recommendation:** Register `TMP_TS`/`TMP_RS` cleanup in a `trap ... RETURN` inside the function,
or add them to the existing exit trap.

---

## 6. Out of scope for this report

- Feature-parity gaps (Error Tracking, Infrastructure Monitoring, Service Map, Log Pattern
  Clustering, Notebooks, etc.) — already tracked in the active roadmap's Tiers 1-4.
- Architecture/coupling debt (NLQ extraction, repository/tenant-scoping layer, queue-based
  stream-processor handoff, shared crates) — already tracked in the roadmap's §7 Service Layer
  Architecture section; this review did not find new items in that category beyond the
  `/metrics` factual correction noted in §2.1 above.
- Dependency version staleness — `Cargo.toml`/`package.json` pins looked current at review time and
  Renovate is actively bumping them; no stale major-version pins found.
- Performance/load testing — `perf-smoke` is the active gate; not re-litigated here.

---

## 7. Suggested next slices

| Finding | Slice size | Touches |
|---|---|---|
| 1.1 — fail closed on session secret | S | `services/auth-service/src/main.rs`, `charts/observable/values.yaml`, `templates/auth-service.yaml` |
| 1.2 — test OIDC flow | L | `services/auth-service/src/oidc.rs`, `session.rs` |
| 1.3 — test tenant-scoping middleware | M | `services/query-api/src/middleware/auth.rs`, `services/admin-service/src/middleware/auth.rs` |
| 1.4 — remove login-path unwraps | XS | `services/auth-service/src/oidc.rs` |
| 1.5 — delete dead SQL builders | S | `services/query-api/src/sql_templates.rs`, `logs.rs` |
| 2.1 — add `/metrics` to 3 services | M | `services/alert-evaluator`, `services/ingest-gateway`, `services/stream-processor` |
| 3.1 — reuse `reqwest::Client` in auth middleware | S | `query-api`/`admin-service` `middleware/auth.rs` |
| 4.1 — test admin-service privilege handlers | M | `tokens.rs`, `admin_members.rs`, `usage.rs` |
| 4.2 — test `libs/test-support` | S | `libs/test-support/src` |
| 4.3 — test admin billing/fleet pages | S | `apps/frontend/src/features/admin` |
| 5.1 — fix stale CI-disabled claim | XS | `AGENTS.md` |
| 5.2 — document `nlq-multi-model.py` | XS | `spec/08-ai-ml.md`, `spec/10-process.md`, `AGENTS.md`, `README.md` |
| 5.3 — version `tests/nlq/cases.json` | XS | `tests/nlq/cases.json` |
| 5.4 — fix `local-ci.sh` temp-dir leak | XS | `scripts/local-ci.sh` |

Each is independently shippable as a single-purpose PR per `AGENTS.md`'s "Branch and PR Every
Iteration" rule. All are also tracked as backlog items in
`docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md` §3.6.
