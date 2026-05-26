# P4-S9 Boundary-Focused Security Review

**Date:** 2026-05-26  
**Scope:** auth, tenancy, query (NLQ SQL), and ingest boundaries.

---

## 1. Auth Boundary — query-api

**Mechanism:** `services/query-api/src/middleware/auth.rs` `require_tenant`

**Path A — API key:** `Authorization: Bearer <key>` + `X-Tenant-ID`. The raw key is SHA-256 hashed before the `api_keys` lookup so the database never stores cleartext secrets. `revoked_at` is checked. `tenant_id` ownership is verified (key must belong to the claimed tenant). **Result: PASS.**

**Path B — Session cookie:** `Cookie: session=<jwt>` forwarded to `auth-service POST /internal/validate-session`. If the session's tenant differs from `X-Tenant-ID`, a `user_tenant_roles` cross-tenant check is performed before granting access. **Result: PASS.**

**Fallback behaviour:** If Path A fails with `UNAUTHORIZED` (key not found), the middleware falls through to Path B. All other errors from Path A short-circuit. This is correct: it allows CLI callers who pass a Bearer token as a session fallback. **Result: PASS.**

**Bootstrap endpoints:** `GET /v1/tenants` and `GET /v1/tenants/{id}/environments` are outside the `require_tenant` middleware layer. Without a session or bearer token, `GET /v1/tenants` returns the full list of tenant names and UUIDs. This is intentional for the initial tenant-selector bootstrap (the selector loads before auth context is chosen). **Classification: INFO — accepted risk for v1; revisit when SCIM or multi-tenant isolation becomes a requirement.**

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

**catalog_field identifier (FIXED — this PR):** Before this fix, `catalog_field` from the NLQ IR was used directly as a SQL column alias (`AS {field}`) and in `GROUP BY {field}` without sanitisation. An attacker or misconfigured LLM could inject SQL by providing a value such as `x FROM observable.metric_series WHERE 1=1--`. Fixed by `validate_sql_identifier()` which restricts the field to `[a-zA-Z0-9_]{1..64}` and returns `Err(InvalidFilterValue)` otherwise. **Severity before fix: MEDIUM. Result after fix: PASS.**

**group_by alias injection (FIXED — this PR):** Before this fix, `group_by` field names were used as SQL column aliases (`AS {g}`) without sanitisation. Fixed by the same `validate_sql_identifier()` guard; invalid entries are silently dropped with a warning. **Severity before fix: LOW. Result after fix: PASS.**

---

## 5. Ingest Boundary — storage-writer internal endpoints

**Mechanism:** `services/storage-writer/src/main.rs`

`POST /internal/spans`, `/internal/logs`, `/internal/metrics` have **no authentication**. These endpoints are only reachable by `stream-processor` on the internal Docker/k8s network (not exposed via the Ingress or Gateway). Data written here has already been authenticated at the ingest-gateway boundary and tenant-stamped before publication to Redpanda.

**Risk:** If network isolation breaks, any host on the internal network can write arbitrary telemetry data to any tenant. This is an accepted design trade-off for v1: the internal boundary does not replicate the ingest-gateway auth because the cost outweighs the risk given current network segmentation.

**Classification: INFO — accepted risk for v1; revisit when mTLS or service-mesh auth is introduced.**

---

## 6. Checkpoint: Are Any Findings Severe Enough to Block v1?

**No.** The two SQL identifier-injection findings (MEDIUM and LOW) are fixed in this PR. All remaining findings are classified INFO and documented with rationale.

| Boundary | Finding | Severity | Disposition |
|---|---|---|---|
| Auth (query-api) | Bearer + session dual-path | — | PASS |
| Auth (query-api) | Bootstrap tenant list exposed | INFO | Accepted |
| Auth (ingest-gateway) | Bearer only, role enforcement | — | PASS |
| Tenancy | Parameterised queries + row validation | — | PASS |
| Query — NLQ | `catalog_field` identifier injection | MEDIUM | **FIXED** |
| Query — NLQ | `group_by` alias injection | LOW | **FIXED** |
| Ingest | storage-writer internal no-auth | INFO | Accepted |
| Ingest | Cardinality observe-only | INFO | Accepted |
