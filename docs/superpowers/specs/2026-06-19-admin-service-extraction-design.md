# Design: Extract `admin-service`

**Date:** 2026-06-19
**Status:** Proposed. Supersedes/refines the "Repository/tenant-scoping layer for query-api" and
NLQ-extraction findings in `docs/superpowers/specs/2026-06-19-service-layer-architecture-review.md`
by giving privilege-management code its own trust boundary ahead of (and independent from) those.

## Motivation

Member management, API key/token issuance, platform config, and usage reporting are
privilege-granting and privilege-reading operations, currently served by the same `query-api`
binary and `AppState` that serves the trace/log/metric read path. A vulnerability or bug in any
high-traffic query-api handler shares a process boundary with the code that can mint API keys and
grant `tenant_admin` roles. This design extracts that surface into its own service for security
isolation, independent of the size/blast-radius motivations already captured in the architecture
review.

## Scope

**Moving to `services/admin-service`** (from `services/query-api/src/`):
- `admin_members.rs` (287 lines) — member list/add/update-role/remove/revoke-sessions
- `tokens.rs` (305 lines) — API key/token lifecycle
- `config.rs` (339 lines) — platform config, LLM settings/keys
- `usage.rs` — tenant usage reporting

**Not moving:** `discovery.rs` (`/v1/tenants/{id}/environments` and other tenant discovery reads
stay in query-api — they're not privilege-bearing). Fleet management (`/admin/fleet`) stays
unimplemented per the existing roadmap item; when built, it should target admin-service, not
query-api.

## Architecture

```
Frontend (/admin/*)
  → nginx/ingress path rules:
      /v1/admin/*              → admin-service
      /v1/tokens*              → admin-service
      /v1/tenants/config       → admin-service
      /v1/tenants/usage-report → admin-service
      /v1/tenants/*  (other)   → query-api  (unchanged)

admin-service (new, Rust/Axum)
  AppState { db: PgPool, ch: ClickHouseClient (usage-report only), auth_service_url }
  writes: user_tenant_roles, api_keys, platform_config
  reads: ClickHouse (usage-report aggregates only)

auth-service (unchanged)
  /internal/validate — reads api_keys, user_tenant_roles (now written by admin-service instead
  of query-api; no interface change, no circular dependency)

query-api (shrinks)
  drops admin_members.rs, tokens.rs, config.rs, usage.rs
  keeps discovery.rs and all trace/log/metric/dashboard/alert/incident/SLO/NLQ handlers
```

URL paths are unchanged — only the routing target changes. Frontend admin pages
(`MemberManagementPage`, `TenantConfigurationPage`, `BillingReportPage`,
`apps/frontend/src/api/admin-members.ts`) require no code changes, only the ingress/nginx rule
update.

## Shared `observable-auth` crate — DONE 2026-06-20

**Status: complete.** See `archived/plans/2026-06-20-observable-auth-crate.md` for the
implementation plan and `libs/observable-auth/src/lib.rs` for the result.

**Correction to this section's original framing:** it described the duplication as each service
"reimplementing `verify_session_jwt`/role-claim parsing independently." That premise was wrong —
neither `query-api` nor `ingest-gateway` did local JWT verification; both already called
`auth-service` over HTTP (`query-api`'s session-cookie path hit `/internal/validate-session`;
`ingest-gateway`'s API-key path hit `/internal/validate`). The actual duplication was the
bearer/cookie header-extraction boilerplate and two slightly different `TenantContext` shapes —
and, more significantly, `query-api`'s API-key path didn't call `auth-service` at all: it queried
`api_keys` directly, bypassing `auth-service`'s audit logging entirely.

What was actually built in `libs/observable-auth`:
- `verify_api_key(http, auth_service_url, api_key) -> Result<ApiKeyContext, AuthError>` — POSTs to
  `/internal/validate`.
- `verify_session(http, auth_service_url, session_token) -> Result<SessionContext, AuthError>` —
  POSTs to `/internal/validate-session`.
- `ApiKeyContext { tenant_id, role, environment }`, `SessionContext { tenant_id, user_id, role }` —
  kept distinct rather than unified into one `TenantContext`, since the two services' own
  `TenantContext` shapes carry genuinely different fields for genuinely different purposes.
- `AuthError` enum with `impl From<AuthError> for axum::http::StatusCode`.
- `extract_bearer_token`, `extract_session_cookie`, `extract_tenant_id_header` header helpers.
- A `require_admin`-style role guard was **not** added here — it remains scoped to admin-service's
  handlers when that slice is built (Slices 2-3 below, not yet started), since it's
  admin-specific, not shared with query-api/ingest-gateway's read paths.

`query-api` and `ingest-gateway` are migrated. As a result of this work, `query-api`'s API-key
path now also routes through `auth-service`'s `/internal/validate`, closing the audit-trail gap.

## Data Flow

1. Admin user authenticates via existing session flow (auth-service OIDC/Zitadel) — unchanged.
2. Frontend calls `/v1/admin/members` etc. with session cookie.
3. Ingress routes to admin-service.
4. admin-service verifies session JWT via `observable-auth`, checks `tenant_admin` role,
   performs the write against PostgreSQL.
5. auth-service's `/internal/validate` (used by ingest-gateway for API key checks) continues
   reading the same `api_keys` table — now written exclusively by admin-service. No change to
   auth-service's code or contract.

## Error Handling / Rollback

- admin-service is a new deployable; rollout is additive — deploy it, flip ingress routing rules,
  remove the now-dead handlers from query-api in a follow-up PR once admin-service is verified in
  production. Two-step migration avoids a single big-bang cutover.
- Rollback: revert ingress routing rules to point back at query-api (keep the handlers in
  query-api until admin-service has run in production for one deploy cycle, then remove them in
  the follow-up PR).

## Testing

- Existing Testcontainers integration tests (`postgres_tokens_integration.rs`,
  `session_auth_integration.rs`, `postgres_tenants_integration.rs`) relocate to admin-service's
  test suite, unchanged in substance.
- Add one cross-service integration test: admin-service creates an API key, auth-service's
  `/internal/validate` successfully validates it — proves the write/read boundary works across
  the service split.
- `observable-auth` crate gets its own unit tests for JWT verification edge cases (expiry,
  malformed claims, role parsing) shared by all three consuming services.

## Sequencing

This is independent of the NLQ extraction and repository-layer findings in the architecture
review — it can be promoted on its own. Suggested slice order if implemented:
1. **DONE 2026-06-20.** `observable-auth` crate extraction + migrate query-api and ingest-gateway
   to it. (Not fully behavior-neutral as originally planned — see the correction above: closing
   query-api's API-key audit-trail gap was an intentional, separately-reviewed behavior change
   within this slice, not scope creep.)
2. Scaffold `admin-service`, move the four handler modules, wire ingress routing. Not started.
3. Remove the moved handlers from query-api. Not started.
