# P4-S9 Boundary-Focused Security Review Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Review auth, tenancy, query, and ingest boundaries; document explicit findings; and fix the two identifier-injection vulnerabilities in the NLQ SQL template library that are severe enough to block v1.

**Architecture:** Four boundaries are reviewed in turn: auth (query-api + ingest-gateway), tenancy, query (NLQ SQL templates), and ingest (storage-writer internal). Two blocking findings in the SQL template library are fixed with a `validate_sql_identifier()` allowlist guard applied to `catalog_field` and `group_by` aliases before they appear as SQL identifiers. Findings are summarised in `docs/security-review-p4-s9.md`.

**Tech Stack:** Rust 2024 edition, existing unit-test suite in `sql_templates.rs`, no new dependencies.

---

## Findings Summary (ahead of tasks)

| Boundary | Finding | Severity | Disposition |
|---|---|---|---|
| Auth (query-api) | Bearer + session dual-path with correct fallback | — | PASS |
| Auth (query-api) | `/v1/tenants` returns all tenant names without auth | INFO | Document; intentional bootstrap design |
| Auth (ingest-gateway) | Bearer only (no session), role enforcement | — | PASS |
| Tenancy | Every ClickHouse query binds `tenant_id` via parameters | — | PASS |
| Tenancy | `validate_trace_rows_for_tenant` double-checks rows | — | PASS |
| Query — NLQ | `catalog_field` used as SQL identifier alias without allowlist | **MEDIUM** | **FIX** |
| Query — NLQ | `group_by` field names used as SQL aliases without allowlist | **LOW** | **FIX** |
| Ingest | storage-writer `/internal/*` has no auth; network-only isolation | INFO | Document |
| Ingest | cardinality budget is observe-only (warns, never rejects) | INFO | Document |

---

## Task 1: Extend `SqlTemplateError` and add identifier validator

**Files:**
- Modify: `services/query-api/src/sql_templates.rs`

The existing `SqlTemplateError::InvalidFilterValue(String)` can reuse for identifier failures. Add a private `validate_sql_identifier` helper next to `escape_string_value`.

- [ ] **Step 1: Add failing tests for identifier injection in catalog and group_by**

Open `services/query-api/src/sql_templates.rs` and add the following tests inside the `#[cfg(test)] mod tests` block (after the existing "Catalog" tests):

```rust
// ── Identifier injection guard ────────────────────────────────────────────────

#[test]
fn catalog_field_with_sql_injection_is_rejected() {
    let mut ir = catalog_ir();
    ir.catalog_field = Some("x FROM observable.metric_series WHERE 1=1--".into());
    let ctx = catalog_ctx_for(&ir);
    assert!(
        generate_sql(&ctx).is_err(),
        "catalog_field with SQL injection must return Err"
    );
}

#[test]
fn catalog_field_with_space_is_rejected() {
    let mut ir = catalog_ir();
    ir.catalog_field = Some("service name".into());
    let ctx = catalog_ctx_for(&ir);
    assert!(
        generate_sql(&ctx).is_err(),
        "catalog_field with spaces must return Err"
    );
}

#[test]
fn catalog_field_alphanumeric_underscore_is_accepted() {
    let mut ir = catalog_ir();
    ir.catalog_field = Some("service_name".into());
    let ctx = catalog_ctx_for(&ir);
    let sql = generate_sql(&ctx).expect("valid identifier must succeed");
    assert!(sql.contains("service_name"));
}

#[test]
fn group_by_with_sql_injection_is_silently_dropped() {
    // group_by identifiers that fail validation are silently removed from the query.
    let mut ir = base_ir(NlqOperation::Timeseries);
    ir.group_by = vec!["service_name".into(), "bad; DROP TABLE--".into()];
    let ctx = ctx_for(&ir);
    let sql = generate_sql(&ctx).expect("valid fields must succeed even with bad ones present");
    // The valid field must appear.
    assert!(sql.contains("ms.service_name AS service_name"), "valid field missing: {sql}");
    // The injected string must NOT appear.
    assert!(
        !sql.contains("DROP"),
        "injection must be stripped from SQL: {sql}"
    );
}

#[test]
fn group_by_alphanumeric_underscore_is_accepted() {
    let mut ir = base_ir(NlqOperation::Timeseries);
    ir.group_by = vec!["environment".into()];
    let ctx = ctx_for(&ir);
    let sql = generate_sql(&ctx).expect("valid group_by must succeed");
    assert!(sql.contains("ms.environment AS environment"), "environment field missing: {sql}");
}
```

- [ ] **Step 2: Run the new tests to confirm they fail**

```
cargo test -p query-api sql_templates::tests::catalog_field_with_sql_injection -- --nocapture
cargo test -p query-api sql_templates::tests::group_by_with_sql_injection -- --nocapture
```

Expected: both tests FAIL (the injection is not yet rejected).

---

## Task 2: Implement `validate_sql_identifier` and fix `catalog_sql`

**Files:**
- Modify: `services/query-api/src/sql_templates.rs`

- [ ] **Step 1: Add the validator function**

Insert this function in the `// ── Helper functions ──` section, right after `escape_string_value`:

```rust
/// Validates that `s` is a safe SQL identifier: ASCII alphanumeric plus `_`, max 64 chars.
///
/// Use this for any user-supplied value that appears as a SQL identifier (column alias,
/// GROUP BY field name, catalog field name) rather than as a quoted string value.
/// Quoted string values go through `escape_string_value` instead.
fn validate_sql_identifier(s: &str) -> Result<(), SqlTemplateError> {
    if s.is_empty() || s.len() > 64 {
        return Err(SqlTemplateError::InvalidFilterValue(s.to_string()));
    }
    if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        Ok(())
    } else {
        Err(SqlTemplateError::InvalidFilterValue(s.to_string()))
    }
}
```

- [ ] **Step 2: Call `validate_sql_identifier` in `catalog_sql`**

In `catalog_sql`, add the validation call immediately after the `field` variable is extracted:

```rust
fn catalog_sql(ctx: &SqlContext) -> Result<String, SqlTemplateError> {
    let field = ctx
        .ir
        .catalog_field
        .as_deref()
        .ok_or(SqlTemplateError::MissingCatalogField)?;

    validate_sql_identifier(field)?;

    let col_expr = map_filter_field(field);
    let filters = build_filter_clauses_checked(&ctx.ir.filters)?;
    // ... rest unchanged
```

- [ ] **Step 3: Run the catalog injection test to confirm it now passes**

```
cargo test -p query-api sql_templates::tests::catalog_field_with_sql_injection -- --nocapture
cargo test -p query-api sql_templates::tests::catalog_field_with_space_is_rejected -- --nocapture
cargo test -p query-api sql_templates::tests::catalog_field_alphanumeric_underscore_is_accepted -- --nocapture
```

Expected: all three PASS.

- [ ] **Step 4: Run full sql_templates tests to confirm no regressions**

```
cargo test -p query-api sql_templates -- --nocapture
```

Expected: all tests pass.

---

## Task 3: Fix `build_group_by` alias injection

**Files:**
- Modify: `services/query-api/src/sql_templates.rs`

Invalid `group_by` identifiers are silently dropped (rather than returning an error) because group_by is additive — dropping a bad field degrades results gracefully without breaking valid NLQ responses for the rest of the IR.

- [ ] **Step 1: Rewrite `build_group_by` to filter invalid identifiers**

Replace the current `build_group_by` implementation with:

```rust
/// Builds `(extra_select_cols, group_by_extension)` for a group_by list.
///
/// Returns:
/// - `extra_select_cols`: `,\n    <col> AS <col>` fragment for SELECT
/// - `group_by_extension`: `, <col>` fragment appended after `GROUP BY bucket`
///
/// Entries that fail `validate_sql_identifier` are silently dropped with a warning.
fn build_group_by(group_by: &[String]) -> (String, String) {
    if group_by.is_empty() {
        return (String::new(), String::new());
    }
    let valid: Vec<&str> = group_by
        .iter()
        .filter(|g| {
            if validate_sql_identifier(g).is_err() {
                tracing::warn!(field = %g, "group_by field rejected: not a valid SQL identifier");
                false
            } else {
                true
            }
        })
        .map(String::as_str)
        .collect();

    if valid.is_empty() {
        return (String::new(), String::new());
    }

    let select_part: String = valid
        .iter()
        .map(|g| {
            let col = map_filter_field(g);
            format!(",\n    {col} AS {g}")
        })
        .collect();
    let group_part: String = valid
        .iter()
        .map(|g| {
            let col = map_filter_field(g);
            format!(", {col}")
        })
        .collect();
    (select_part, group_part)
}
```

- [ ] **Step 2: Run the group_by injection tests**

```
cargo test -p query-api sql_templates::tests::group_by_with_sql_injection -- --nocapture
cargo test -p query-api sql_templates::tests::group_by_alphanumeric_underscore_is_accepted -- --nocapture
```

Expected: both PASS.

- [ ] **Step 3: Run full sql_templates tests**

```
cargo test -p query-api sql_templates -- --nocapture
```

Expected: all pass.

- [ ] **Step 4: Run `cargo fmt --all`**

```
cargo fmt --all
```

Expected: no diff.

- [ ] **Step 5: Commit**

```
git add services/query-api/src/sql_templates.rs
git commit -m "fix(query-api): validate catalog_field and group_by as SQL identifiers (RF-3)"
```

---

## Task 4: Write the security review findings document

**Files:**
- Create: `docs/security-review-p4-s9.md`

- [ ] **Step 1: Create the document**

```markdown
# P4-S9 Boundary-Focused Security Review

**Date:** 2026-05-26  
**Reviewer:** Agent (P4-S9 plan)  
**Scope:** auth, tenancy, query (NLQ SQL), and ingest boundaries in the current main branch.

---

## 1. Auth Boundary — query-api

**Mechanism:** `services/query-api/src/middleware/auth.rs` `require_tenant`

**Path A — API key:** `Authorization: Bearer <key>` + `X-Tenant-ID`. The raw key is SHA-256 hashed before the `api_keys` lookup so the database never stores cleartext secrets. `revoked_at` is checked. `tenant_id` ownership is verified (key must belong to the claimed tenant). **Result: PASS.**

**Path B — Session cookie:** `Cookie: session=<jwt>` forwarded to `auth-service POST /internal/validate-session`. If the session's tenant differs from `X-Tenant-ID`, a `user_tenant_roles` cross-tenant check is performed before granting access. **Result: PASS.**

**Fallback behaviour:** If Path A fails with `UNAUTHORIZED` (key not found), the middleware falls through to Path B. All other errors from Path A short-circuit. This is correct: it allows CLI callers who pass a Bearer token as a session fallback. **Result: PASS.**

**Bootstrap endpoints:** `GET /v1/tenants` and `GET /v1/tenants/{id}/environments` are outside the `require_tenant` middleware layer. Without a session or bearer token, `GET /v1/tenants` returns the full list of tenant names and UUIDs. This is intentional for the initial tenant-selector bootstrap (the selector loads before auth context is chosen), documented in `docs/agent-context.md`. **Classification: INFO — accepted risk for v1, revisit when SCIM or multi-tenant isolation becomes a requirement.**

---

## 2. Auth Boundary — ingest-gateway

**Mechanism:** `services/ingest-gateway/src/auth.rs` `auth_middleware`

Bearer token only (no session cookie path — correct; ingest is SDK/CLI only). Token forwarded to `auth-service POST /internal/validate`. Role checked via `can_ingest()` — only `member` and `admin` roles can ingest; `viewer` is rejected with 403. **Result: PASS.**

**Rate limiters:** Per-tenant keyed rate limiters (governor crate) on traces, logs, and metrics. Limits are enforced at the handler level. **Result: PASS.**

**Cardinality budget:** `MetricCardinalityBudget` is observe-only — it warns when a tenant exceeds the budget but never rejects ingestion. This is a deliberate fail-open design to avoid losing telemetry. **Classification: INFO — acceptable for v1; enforcement is a future slice.**

---

## 3. Tenancy Boundary — query-api

Every authenticated handler receives `TenantContext` via `Extension<TenantContext>` injected by the `require_tenant` middleware. ClickHouse queries bind `tenant_id` via parameterised placeholders (`?`), not string interpolation. The `validate_trace_rows_for_tenant` function performs a second-pass row check on returned spans to defend against any ClickHouse JOIN edge case that might leak cross-tenant rows. **Result: PASS.**

---

## 4. Query Boundary — NLQ SQL templates

**Mechanism:** `services/query-api/src/sql_templates.rs`

**String values:** All filter values, metric names, and log query terms go through `escape_string_value` (escapes `\` and `'`) before being inlined into single-quoted ClickHouse string literals. Numeric operators require the value to parse as `f64`; non-numeric values are rejected with `InvalidFilterValue`. Regex patterns are capped at 256 characters to prevent ReDoS. **Result: PASS.**

**Tenant ID and time expressions:** Tenant UUIDs are formatted via the `Uuid` Display impl (hyphen-separated hex, no special chars). Time expressions are restricted to `now`, `now-{N}{unit}`, or all-digit Unix nanosecond literals; anything else is rejected. **Result: PASS.**

**catalog_field identifier (FIXED — this PR):** Before this fix, `catalog_field` from the NLQ IR was used directly as a SQL column alias (`AS {field}`) and in `GROUP BY {field}` without sanitisation. An attacker or misconfigured LLM could inject SQL by providing a value such as `x, 1=(SELECT 1)--`. Fixed by `validate_sql_identifier()` which restricts the field to `[a-zA-Z0-9_]{1,64}` and returns `Err(InvalidFilterValue)` otherwise. **Severity before fix: MEDIUM. Result after fix: PASS.**

**group_by alias injection (FIXED — this PR):** Before this fix, `group_by` field names were used as SQL column aliases (`AS {g}`) without sanitisation. The GROUP BY clause itself was safe (used `map_filter_field` output), but the SELECT alias was not. Fixed by the same `validate_sql_identifier()` guard; invalid entries are silently dropped with a warning. **Severity before fix: LOW. Result after fix: PASS.**

---

## 5. Ingest Boundary — storage-writer internal endpoints

**Mechanism:** `services/storage-writer/src/main.rs`

`POST /internal/spans`, `/internal/logs`, `/internal/metrics` have **no authentication**. These endpoints are only reachable by `stream-processor` on the internal Docker/k8s network (not exposed via the Ingress or Gateway). Data written here has already been authenticated at the ingest-gateway boundary and tenant-stamped before publication to Redpanda.

**Risk:** If network isolation breaks, any host on the internal network can write arbitrary telemetry data to any tenant. This is an accepted design trade-off: the internal boundary does not replicate the ingest-gateway auth because the cost (session/key management for service-to-service) outweighs the risk given current network segmentation.

**Classification: INFO — accepted risk for v1; revisit when mTLS or service-mesh auth is introduced.**

---

## 6. Checkpoint Answer

**Are any findings severe enough to block v1?**

The two SQL identifier injection findings (`catalog_field` and `group_by` alias) are fixed in this PR. All remaining findings are classified INFO and documented with rationale. No findings remain that require remediation before v1.

**v1 security posture summary:**
- Auth and tenancy boundaries are correct and tested.
- Query boundary SQL injection surface is now fully covered.
- Internal service boundary relies on network isolation (accepted).
- Bootstrap tenant list exposure is intentional and bounded.
```

- [ ] **Step 2: Verify the file was saved correctly (check line count)**

```
cargo fmt --all
```

Expected: no diff (docs file, not Rust).

- [ ] **Step 3: Commit**

```
git add docs/security-review-p4-s9.md
git commit -m "docs(security): add P4-S9 boundary-focused security review findings"
```

---

## Task 5: Update roadmap and agent-context

**Files:**
- Modify: `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`
- Modify: `docs/agent-context.md`
- Move: `docs/superpowers/plans/2026-05-26-p4-s9-boundary-security-review.md` → `archived/plans/`

- [ ] **Step 1: Mark P4-S9 complete in the roadmap**

In `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`, find:

```markdown
- [ ] **P4-S9: Complete boundary-focused security review**
  - Outcome: auth, tenancy, query, and ingest boundaries have explicit review notes.
  - Checkpoint: are any findings severe enough to block v1?
```

Replace with:

```markdown
- [x] **P4-S9: Complete boundary-focused security review** (COMPLETED 2026-05-26)
  - Outcome: auth, tenancy, query, and ingest boundaries have explicit review notes in `docs/security-review-p4-s9.md`.
  - Two SQL identifier-injection findings (catalog_field and group_by alias in NLQ SQL templates) were fixed with a `validate_sql_identifier()` allowlist guard.
  - Checkpoint: No findings are severe enough to block v1. The two Medium/Low SQL findings are fixed. Remaining INFO items (bootstrap tenant list, cardinality observe-only, storage-writer no-auth) are documented with rationale.
```

- [ ] **Step 2: Update agent-context.md**

In `docs/agent-context.md`, find the line:

```
- Active detailed implementation plan: none — RF-6 self-observability complete across all services; next up is P4-S9 boundary-focused security review.
```

Replace with:

```
- Active detailed implementation plan: none — P4-S9 boundary security review complete; Phase 4 exit gate is now satisfiable. Next: P5 pause-point review or P4-S3b/P4-S4 if required by a v1 customer.
```

Add a new section after the `## Self-Observability (RF-6, completed 2026-05-26)` section:

```markdown
## Security Review (P4-S9, completed 2026-05-26)

- Full findings in `docs/security-review-p4-s9.md`.
- Two SQL identifier-injection findings fixed in `services/query-api/src/sql_templates.rs`:
  - `catalog_field` (NLQ catalog operation) now validated via `validate_sql_identifier()` before use as SQL alias and GROUP BY identifier.
  - `group_by` aliases validated by the same guard; invalid entries are silently dropped with a warning rather than failing the whole query.
- All other findings (bootstrap tenant list, cardinality observe-only, storage-writer internal no-auth) classified INFO and accepted for v1.
- Phase 4 exit gate: all P4 mandatory slices are now complete (P4-S2, P4-S3, P4-S5, P4-S6, P4-S7, P4-S8, P4-S9). RF-6 complete. Phase 5 pause-point review should precede new P5 work.
```

- [ ] **Step 3: Move the detailed plan to archived**

```bash
mv docs/superpowers/plans/2026-05-26-p4-s9-boundary-security-review.md archived/plans/
```

- [ ] **Step 4: Update the completed/archived plans list in agent-context.md**

In `docs/agent-context.md`, add to the "Completed / archived detailed plans" list:

```
  - `archived/plans/2026-05-26-p4-s9-boundary-security-review.md` — P4-S9 boundary security review; two NLQ SQL identifier-injection fixes; findings summary at `docs/security-review-p4-s9.md`
```

- [ ] **Step 5: Run `cargo fmt --all` and `cargo build -p query-api`**

```
cargo fmt --all
cargo build -p query-api
```

Expected: clean build.

- [ ] **Step 6: Commit**

```
git add docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md docs/agent-context.md archived/plans/2026-05-26-p4-s9-boundary-security-review.md
git commit -m "chore(docs): mark P4-S9 complete, archive plan, update agent-context"
```

---

## Verification Checklist (run before opening the PR)

- [ ] `cargo fmt --all` — no diffs
- [ ] `cargo build -p query-api` — compiles cleanly
- [ ] `cargo test -p query-api sql_templates -- --nocapture` — all tests pass, including the 5 new ones
- [ ] `docs/security-review-p4-s9.md` exists and is committed
- [ ] `archived/plans/2026-05-26-p4-s9-boundary-security-review.md` exists
- [ ] `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md` shows P4-S9 as `[x]`
- [ ] `docs/agent-context.md` references P4-S9 completion and next steps

---

## Slice Summary

| Boundary | Before | After |
|---|---|---|
| Auth (query-api) | PASS | PASS |
| Auth (ingest-gateway) | PASS | PASS |
| Tenancy | PASS | PASS |
| Query NLQ — catalog_field | MEDIUM injection | PASS (allowlist) |
| Query NLQ — group_by alias | LOW injection | PASS (allowlist) |
| Ingest (storage-writer) | INFO documented | INFO documented |

**Rollback:** Revert the `validate_sql_identifier` calls in `catalog_sql` and `build_group_by`. NLQ queries continue to function; only the identifier injection protection is removed.

**ADR/spec sync:** No ADR change required. The `validate_sql_identifier` guard implements the field allowlisting described in `spec/` under RF-3 NLQ SQL safety without changing the NLQ IR contract.

**Next slice:** Phase 4 exit gate is now satisfiable. Recommended: run the Phase 4 pause-point questions (supportability, restore, permissions) before promoting the first new Phase 5 slice.
