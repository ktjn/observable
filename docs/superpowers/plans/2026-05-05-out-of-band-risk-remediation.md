# Out-Of-Band Risk Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close concrete security, correctness, and regression-gate weaknesses found during a whole-repo code scan without changing the active P4-S5 roadmap lane.

**Architecture:** Keep each remediation as a separate short-lived PR. The first two slices harden tenant/auth and NLQ SQL safety because they can affect data isolation. The third slice restores the local regression gate contract so Testcontainers coverage is not silently missed. The fourth slice removes small governance drift that can mislead future agents.

**Tech Stack:** Rust, Axum, SQLx/PostgreSQL, ClickHouse, Testcontainers, Bash, repository docs/spec governance.

---

## Findings Summary

The scan was anchored in `AGENTS.md`, `spec/10-process.md`, `spec/adr/README.md`, `docs/agent-context.md`, and the active Phases 2-8 plan. It found these code-level gaps:

1. **Query API trusts caller-selected tenant IDs.** `services/query-api/src/middleware/auth.rs:16-26` accepts any valid `X-Tenant-ID` and inserts it as `TenantContext`. This preserves tenant scoping inside handlers, but it does not prove the caller is allowed to use that tenant. It conflicts with the intent of ADR-008 and leaves query/read APIs spoofable by anyone who can reach query-api.
2. **NLQ SQL filters inline untrusted values.** `services/query-api/src/sql_templates.rs:711-723` escapes string delimiters but emits numeric comparisons without numeric validation, and `services/query-api/src/mcp_query.rs:336-363` builds trace SQL through raw string concatenation. A malicious or malformed IR can produce invalid SQL, expensive regexes, or a predicate shape that is harder to reason about than the tenant-isolation gate requires.
3. **`local-ci.sh` does not run integration tests.** `scripts/local-ci.sh:60-64` runs `cargo test --workspace --lib --bins`; the Dockerfile repeats the same test scope. Testcontainers tests exist under `services/**/tests`, but the mandatory local gate does not execute them unless a developer runs them separately.
4. **Planning and code still point at already-known drift.** `docs/analysis/2026-05-01-repo-review.md` records stale LLM plan text, duplicated top-level agent instruction files, and disabled GitHub CI discoverability. Those are small but recurring agent-orientation risks.

## File Structure

- Modify: `services/query-api/src/middleware/auth.rs` for authenticated tenant context extraction.
- Modify: `services/query-api/src/main.rs` for middleware state and bootstrap-route exclusions.
- Modify: `services/query-api/tests/http_api_integration.rs` for query-api auth/tenant spoof tests.
- Modify: `services/query-api/src/sql_templates.rs` for validated filter rendering.
- Modify: `services/query-api/src/mcp_query.rs` for shared safe SQL helpers on trace/log NLQ paths.
- Modify or create: `services/query-api/tests/nlq_sql_safety_integration.rs` for malicious IR coverage.
- Modify: `scripts/local-ci.sh` and `Dockerfile` so the default code gate can run integration tests when Docker is available.
- Modify: `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`, `archived/plans/2026-04-29-p8-s6b-local-llm-vllm.md`, `AGENT.md`, `GEMINI.md`, and `.github/workflows/build.yml` only in the governance-drift cleanup slice.

## Slice Contract

Source spec: `spec/04-tenancy-security.md`, `spec/08-ai-ml.md`, `spec/10-process.md §16.7-§16.10`, ADR-007, ADR-008, ADR-014, ADR-019, ADR-021, ADR-025, ADR-026.
Phase: out-of-band remediation; does not supersede active P4-S5.
Parent phase item: risk reduction across tenancy, NLQ, CI, and agent governance.
Acceptance target: query APIs cannot be used with an arbitrary tenant header, NLQ SQL generation rejects unsafe predicate values, the local gate has an explicit integration-test signal, and known governance drift has a single source of truth.
User/operator outcome: weaker failure modes become deterministic 4xx responses or local-ci failures before review.
Files or modules expected to change: query-api middleware/tests, NLQ SQL templates/tests, local-ci/Dockerfile, small governance docs.
Out of scope: OpenFGA graph authorization, SSO/OIDC, full DataFusion migration, production secret management, new roadmap capabilities.
Verification: focused Rust tests for each code slice, Testcontainers where real dependencies are touched, `bash scripts/local-ci.sh` before pushing code, and `git diff --check` for docs-only cleanup.
Baseline: before each code slice, run the current focused test command listed in that task and record whether it passes, fails, or misses coverage.
New errors introduced: none.
Telemetry impact: auth failures and SQL rejection paths must log structured reason fields without leaking credentials or raw untrusted query text.
Auth/tenancy impact: Task 1 intentionally strengthens query-api tenant authentication.
Data retention or migration impact: none expected.
Rollback path: each slice is independent and can be reverted without schema rollback.
ADR/spec sync: no ADR change expected if implementations harden existing decisions. Update ADR/spec files in the same PR if a slice changes the authorization model, NLQ execution architecture, or regression-gate policy.
Checkpoint question: are the highest-risk gaps now closed enough to continue normal roadmap delivery?
Next smallest slice: return to active P4-S5 unless a human reviewer promotes another finding.

---

### Task 1: Bind Query API Tenant Context To A Valid Credential

**Files:**
- Modify: `services/query-api/src/middleware/auth.rs`
- Modify: `services/query-api/src/main.rs`
- Modify: `services/query-api/tests/http_api_integration.rs`

- [ ] **Step 1: Add failing HTTP integration tests**

Add tests that prove the current spoofable behavior is closed:

```rust
#[tokio::test]
async fn query_api_rejects_missing_authorization_header() {
    // Build the real app with a PostgreSQL pool seeded by migrations and a lazy ClickHouse client.
    // Request: GET /v1/alerts/rules with X-Tenant-ID but no Authorization.
    // Expected after implementation: 401 Unauthorized.
}

#[tokio::test]
async fn query_api_rejects_tenant_header_not_owned_by_token() {
    // Seed or use dev-api-key-0000 for tenant 00000000-0000-0000-0000-000000000001.
    // Request: GET /v1/alerts/rules with Authorization: Bearer dev-api-key-0000
    // and X-Tenant-ID: 00000000-0000-0000-0000-000000000002.
    // Expected after implementation: 403 Forbidden.
}

#[tokio::test]
async fn query_api_accepts_matching_token_and_tenant_header() {
    // Request: GET /v1/alerts/rules with Authorization: Bearer dev-api-key-0000
    // and matching X-Tenant-ID.
    // Expected after implementation: 200 OK.
}
```

Run:

```bash
cargo test -p query-api --test http_api_integration query_api_rejects -- --nocapture
```

Expected: FAIL because `require_tenant` currently reads only `X-Tenant-ID`.

- [ ] **Step 2: Implement credential-bound tenant extraction**

Refactor `services/query-api/src/middleware/auth.rs` so `TenantContext` includes the credential role and is derived from both `Authorization` and `X-Tenant-ID`:

```rust
#[derive(Clone, Debug)]
pub struct TenantContext {
    pub tenant_id: Uuid,
    pub role: String,
}
```

Use the same API-key table semantics as `auth-service`: hash the bearer token with `auth_service::validate::sha256_hex`, query `api_keys` for `tenant_id`, `role`, and `revoked_at`, reject revoked keys, then compare the token tenant with the requested `X-Tenant-ID`. Return `401` for missing/invalid credentials and `403` for a valid token requesting another tenant.

- [ ] **Step 3: Keep bootstrap endpoints unauthenticated**

Confirm `services/query-api/src/main.rs` still leaves these routes outside the authenticated middleware:

```text
GET /v1/tenants
GET /v1/tenants/:id/environments
GET /health
```

All other `/v1/*` routes must use the strengthened middleware.

- [ ] **Step 4: Run focused checks**

Run:

```bash
cargo test -p query-api --test http_api_integration query_api_ -- --nocapture
cargo test -p query-api middleware::auth --lib
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add services/query-api/src/middleware/auth.rs services/query-api/src/main.rs services/query-api/tests/http_api_integration.rs
git commit -m "Bind query API tenant context to credentials"
```

---

### Task 2: Reject Unsafe NLQ SQL Predicate Values

**Files:**
- Modify: `services/query-api/src/sql_templates.rs`
- Modify: `services/query-api/src/mcp_query.rs`
- Modify or create: `services/query-api/tests/nlq_sql_safety_integration.rs`
- Modify: `tests/nlq/cases.json` only if NLQ behavior changes user-visible accepted syntax.

- [ ] **Step 1: Add failing unit tests for numeric and regex predicates**

In `services/query-api/src/sql_templates.rs`, add tests:

```rust
#[test]
fn numeric_filter_rejects_non_numeric_value() {
    let filter = NlqFilter {
        field: "duration_ms".into(),
        op: NlqFilterOp::Gt,
        value: "0 OR 1=1".into(),
    };
    let err = build_filter_expr_checked("duration_ms", filter.op, &filter.value).unwrap_err();
    assert_eq!(err, SqlTemplateError::InvalidFilterValue("duration_ms".into()));
}

#[test]
fn filter_clauses_wrap_each_predicate_in_parentheses() {
    let filters = vec![NlqFilter {
        field: "service_name".into(),
        op: NlqFilterOp::Eq,
        value: "checkout".into(),
    }];
    let sql = build_filter_clauses_checked(&filters).unwrap();
    assert!(sql.contains("AND (ms.service_name = 'checkout')"));
}
```

Expected: FAIL until checked helpers exist.

- [ ] **Step 2: Replace string-only filter rendering with checked rendering**

Introduce:

```rust
pub enum SqlTemplateError {
    // existing variants...
    InvalidFilterValue(String),
}

fn build_filter_expr_checked(
    col: &str,
    op: NlqFilterOp,
    value: &str,
) -> Result<String, SqlTemplateError> {
    let escaped = escape_string_value(value);
    match op {
        NlqFilterOp::Eq => Ok(format!("{col} = '{escaped}'")),
        NlqFilterOp::Ne => Ok(format!("{col} != '{escaped}'")),
        NlqFilterOp::Re | NlqFilterOp::Nre if value.len() <= 256 => {
            let expr = format!("match({col}, '{escaped}')");
            Ok(if op == NlqFilterOp::Nre { format!("NOT {expr}") } else { expr })
        }
        NlqFilterOp::Re | NlqFilterOp::Nre => Err(SqlTemplateError::InvalidFilterValue(col.into())),
        NlqFilterOp::Gt | NlqFilterOp::Gte | NlqFilterOp::Lt | NlqFilterOp::Lte => {
            let parsed: f64 = value.parse().map_err(|_| SqlTemplateError::InvalidFilterValue(col.into()))?;
            let numeric = parsed.to_string();
            Ok(match op {
                NlqFilterOp::Gt => format!("{col} > {numeric}"),
                NlqFilterOp::Gte => format!("{col} >= {numeric}"),
                NlqFilterOp::Lt => format!("{col} < {numeric}"),
                NlqFilterOp::Lte => format!("{col} <= {numeric}"),
                _ => unreachable!(),
            })
        }
    }
}
```

Then make `build_filter_clauses`, `build_log_filter_clauses`, and callers return `Result<String, SqlTemplateError>`. Each rendered predicate must be wrapped in parentheses.

- [ ] **Step 3: Harden trace NLQ SQL construction**

In `services/query-api/src/mcp_query.rs`, replace ad hoc `replace('\'', "\\'")` usage in `execute_trace_query` with `escape_string_value()` from `sql_templates.rs`. Add a unit or integration test where `ir.query` contains `' OR 1=1 --` and assert the generated SQL keeps the injected text inside the string literal and still contains the tenant predicate.

- [ ] **Step 4: Add an end-to-end rejection test**

Create or extend a query-api integration test:

```rust
#[tokio::test]
async fn mcp_query_rejects_malicious_numeric_filter_before_clickhouse_execution() {
    // POST /v1/mcp/query with a metric IR containing op "gt" and value "0 OR 1=1".
    // Use mode/fixtures that do not require a populated ClickHouse result.
    // Expected: 400 Bad Request with a stable JSON error.
}
```

- [ ] **Step 5: Run focused checks and NLQ gate if needed**

Run:

```bash
cargo test -p query-api sql_templates --lib
cargo test -p query-api --test nlq_sql_safety_integration -- --nocapture
```

If `tests/nlq/cases.json` changes, also run the NLQ quality gate against a running cluster:

```bash
python3 scripts/nlq-eval.py
```

- [ ] **Step 6: Commit**

```bash
git add services/query-api/src/sql_templates.rs services/query-api/src/mcp_query.rs services/query-api/tests/nlq_sql_safety_integration.rs tests/nlq/cases.json
git commit -m "Reject unsafe NLQ SQL predicates"
```

---

### Task 3: Make Integration-Test Coverage Explicit In Local CI

**Files:**
- Modify: `scripts/local-ci.sh`
- Modify: `Dockerfile`
- Test: focused script syntax checks plus one local-ci mode.

- [ ] **Step 1: Add a failing script-unit expectation**

Extend `tests/e2e/smoke_test_unit.sh` with an assertion that `scripts/local-ci.sh` has an explicit integration-test stage rather than relying on comments.

Expected: FAIL because `local-ci.sh` only has `cargo test --workspace --lib --bins`.

- [ ] **Step 2: Add an integration-test stage behind Docker availability**

In `scripts/local-ci.sh`, keep the fast unit stage and add:

```bash
if [[ $SKIP_DOCKER -eq 0 ]]; then
  step "Rust integration tests"
  cargo test --workspace --tests && ok "cargo integration tests" || fail "cargo integration tests"
else
  echo "SKIP  Rust integration tests (--skip-docker; Testcontainers require Docker)"
fi
```

This preserves `--skip-docker` as the documented replacement signal when Docker is unavailable.

- [ ] **Step 3: Align Dockerfile CI stage**

In `Dockerfile`, either run `cargo test --workspace --tests` in the `rust-ci` stage or add a clear comment that image builds intentionally do not run Testcontainers because nested Docker is unavailable. Prefer the former only if the build environment can expose Docker safely; otherwise make `local-ci.sh` the integration-test owner.

- [ ] **Step 4: Run verification**

Run:

```bash
bash -n scripts/local-ci.sh
bash -n tests/e2e/smoke_test_unit.sh
bash tests/e2e/smoke_test_unit.sh
```

If Docker is available, run:

```bash
bash scripts/local-ci.sh --skip-frontend --skip-helm --skip-smoke
```

Expected: PASS, with integration tests either executed or explicitly skipped only through `--skip-docker`.

- [ ] **Step 5: Commit**

```bash
git add scripts/local-ci.sh Dockerfile tests/e2e/smoke_test_unit.sh
git commit -m "Make integration tests explicit in local CI"
```

---

### Task 4: Close Known Governance Drift

**Files:**
- Modify: `archived/plans/2026-04-29-p8-s6b-local-llm-vllm.md`
- Modify: `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`
- Modify: `AGENT.md`
- Modify: `GEMINI.md`
- Modify: `.github/workflows/build.yml`

- [ ] **Step 1: Mark the archived P8-S6b plan superseded**

At the top of `archived/plans/2026-04-29-p8-s6b-local-llm-vllm.md`, add a short status note that ADR-027 superseded the backend-selector design with the unified `api_key` / `url` / `model` setup model.

- [ ] **Step 2: Correct the active plan note**

In `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`, update the P8-S6b/ADR-027 note to describe the shipped unified config behavior and remove references to a required backend selector.

- [ ] **Step 3: Collapse duplicated agent entry files**

Replace `AGENT.md` and `GEMINI.md` with thin pointers:

```markdown
# Repository Agent Instructions

See [AGENTS.md](AGENTS.md). That file is the canonical instruction source for this repository.
```

Leave `CLAUDE.md` untouched unless a human reviewer asks to make it a pointer as well.

- [ ] **Step 4: Clarify disabled GitHub CI**

At the top of `.github/workflows/build.yml`, add a comment explaining that PR triggers are intentionally disabled and `bash scripts/local-ci.sh` is the required local gate before pushing code.

- [ ] **Step 5: Run documentation hygiene**

Run:

```bash
git diff --check
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add archived/plans/2026-04-29-p8-s6b-local-llm-vllm.md docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md AGENT.md GEMINI.md .github/workflows/build.yml
git commit -m "Close governance drift"
```

---

## Verification Plan

Docs-only changes to this plan are exempt from `bash scripts/local-ci.sh`; run:

```bash
git diff --check
```

Implementation PRs created from this plan must run the focused checks listed in their task, then:

```bash
bash scripts/local-ci.sh
```

Use `--skip-docker`, `--skip-frontend`, `--skip-helm`, or `--skip-smoke` only when the relevant tool is unavailable, and record the skipped stage plus replacement signal in the PR.

## Planning Review

**Active plan reference:** `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`

**Plan alignment:**
- [x] Task found in plan: no; this is intentional out-of-band risk remediation.
- [x] Scope matches plan description: yes; it does not alter P4-S5 or the value-first roadmap.
- [x] Dependencies satisfied: yes for planning. Implementation slices must still read overlapping ADRs before code changes.

**Flags:**
- No Phase 1 boundary violation.
- No roadmap capability added.
- Plan update required only if a human decides to schedule any remediation as the next active slice.

**Verdict:** ALIGNED as out-of-band remediation, not as a replacement for P4-S5.

## ADR/Spec Synchronization

No ADR or spec update is included in this planning document. The plan hardens existing ADR decisions: ADR-007 tenant isolation, ADR-008 authorization, ADR-014/ADR-021 AI advisory boundaries, ADR-019 CI scripts, ADR-025 Testcontainers, and ADR-026 no proprietary DSL. Implementation PRs must update ADRs/specs if they change the auth model, NLQ execution architecture, or CI policy rather than enforcing the current one.
