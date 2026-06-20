# `observable-auth` Crate Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Extract a shared `libs/observable-auth` crate for the bearer-token/session-cookie
extraction boilerplate and `TenantContext` shapes currently duplicated (with drift) between
`query-api` and `ingest-gateway`, and close an audit-trail gap discovered while researching this:
`query-api`'s API-key path bypasses `auth-service` and queries `api_keys` directly, so those
checks are never written to `auth-service`'s audit log (`ingest-gateway`'s API-key checks already
go through `auth-service`'s `/internal/validate` and are audited). Roadmap: this is Slice 1 of 3
in the (Deferred §7, explicitly promoted by user request 2026-06-20) "Extract NLQ/AI from
query-api" / admin-service-extraction work — specifically the `observable-auth` crate called out in
`spec/adr/ADR-033-admin-service-extraction.md` and
`docs/superpowers/specs/2026-06-19-admin-service-extraction-design.md`.

**Correction to the prior design docs:** ADR-033 and the design doc describe the duplication as
"each reimplements `verify_session_jwt`/role-claim parsing independently" and propose a
`verify_session_jwt(token, secret) -> Result<TenantContext, AuthError>` function. This is
inaccurate — **neither service does local JWT verification today**. Both delegate to
`auth-service` over HTTP:
- `query-api`'s session-cookie path calls `auth-service`'s `POST /internal/validate-session`.
- `query-api`'s API-key path does its **own direct Postgres query** against `api_keys` (bypassing
  `auth-service` entirely — this is the audit gap).
- `ingest-gateway`'s only path calls `auth-service`'s `POST /internal/validate` for API keys.

The real duplication is: per-service bearer/cookie header-extraction code, two slightly different
`TenantContext` shapes (`query-api`: `{tenant_id, user_id: Option<Uuid>, role}`; `ingest-gateway`:
`{tenant_id, role, environment}` plus a `can_ingest()` method), and `query-api` re-implementing
`auth-service`'s API-key lookup instead of calling it. This plan extracts shared HTTP-client
wrappers around `auth-service`'s two existing endpoints plus the header-extraction helpers — it
does **not** add local JWT verification (there is nothing to extract there) and does **not**
unify the two `TenantContext` shapes into one (they carry genuinely different fields for genuinely
different purposes — ingest-gateway has no concept of `user_id`, query-api has no concept of
`environment`/`can_ingest`). Update both ADR-033 and the design doc to reflect this correction as
part of this work (Task 4).

**Tech Stack:** Rust workspace crate, matching `libs/domain`'s existing pattern (workspace-inherited
`axum`/`reqwest`/`serde`/`uuid` dependencies).

## Global Constraints

- New crate at `libs/observable-auth`, added to the workspace `[workspace] members` list in the
  root `Cargo.toml` (currently lists `libs/domain` and six services — follow that exact pattern).
- `Cargo.toml` deps: workspace-inherited `axum`, `reqwest`, `serde`, `uuid`, `anyhow`, `tracing` —
  do not add new dependency versions outside what the workspace already pins.
- The crate exposes, at minimum:
  - `pub async fn verify_api_key(http: &reqwest::Client, auth_service_url: &str, api_key: &str) -> Result<ApiKeyContext, AuthError>` —
    POSTs to `{auth_service_url}/internal/validate` with `{"api_key": api_key}`, matching
    `auth-service`'s existing `ValidateRequest`/`ValidateResponse` shape exactly (`tenant_id: Uuid`,
    `role: String`, `environment: String`). Maps a non-2xx response to `AuthError::Unauthorized`.
  - `pub async fn verify_session(http: &reqwest::Client, auth_service_url: &str, session_token: &str) -> Result<SessionContext, AuthError>` —
    POSTs to `{auth_service_url}/internal/validate-session` with `{"session_token": session_token}`,
    matching the existing response shape (`user_id: String` (UUID), `tenant_id: String` (UUID),
    `role: String`) — parse both UUID strings, mapping parse failure to
    `AuthError::Internal`.
  - `pub struct ApiKeyContext { pub tenant_id: Uuid, pub role: String, pub environment: String }`
  - `pub struct SessionContext { pub tenant_id: Uuid, pub user_id: Uuid, pub role: String }`
  - `pub enum AuthError { Unauthorized, Forbidden, ServiceUnavailable, Internal }` with an
    `impl From<AuthError> for axum::http::StatusCode` (Unauthorized→401, Forbidden→403,
    ServiceUnavailable→503, Internal→500) so callers can `.map_err(StatusCode::from)?` or similar.
  - Header-extraction helpers, lifted verbatim from `query-api/src/middleware/auth.rs`'s private
    functions (made `pub` in the new crate): `extract_bearer_token(&HeaderMap) -> Result<Option<String>, AuthError>`,
    `extract_session_cookie(&HeaderMap) -> Option<String>`,
    `extract_tenant_id_header(&HeaderMap) -> Result<Uuid, AuthError>` (returns
    `AuthError::Unauthorized` if absent, a distinguishable variant or reuse `Unauthorized` for
    malformed — check the existing code's BAD_REQUEST-vs-UNAUTHORIZED distinction on malformed vs
    missing and preserve it, adding an `AuthError::BadRequest` variant if needed rather than
    collapsing that distinction).
- Do **not** change `query-api`'s or `ingest-gateway`'s own `TenantContext` struct shapes or public
  behavior beyond Task 3's audit-gap fix — this is a refactor-plus-one-fix, not a behavior overhaul.
  Existing tests for both middlewares must continue passing unmodified in spirit (test bodies may
  need import-path updates only).
- `cargo fmt --all` after every Rust edit, before staging.
- Add unit tests for the new crate's header-extraction and HTTP-wrapper functions (the HTTP
  wrappers can be tested with a local mock server — check what's already used elsewhere in the
  workspace, e.g. `wiremock` if present in any `Cargo.toml`, or a minimal `tokio::net::TcpListener`
  stub if not — prefer whatever pattern already exists rather than introducing a new test-double
  approach).

### Task 1: Scaffold `libs/observable-auth` and migrate `ingest-gateway`

`ingest-gateway` is the simpler consumer (API-key-only, no session path) — migrate it first to
prove the crate's API key verification wrapper works before touching query-api's more complex
dual-path middleware.

1. Create `libs/observable-auth/Cargo.toml` and `src/lib.rs` per Global Constraints: `ApiKeyContext`,
   `SessionContext`, `AuthError`, `verify_api_key`, `verify_session`, and the three header-extraction
   helpers. Write unit tests for the header-extraction helpers (pure functions, no I/O) and for
   `verify_api_key`/`verify_session` against a local mock HTTP server returning both success and
   error responses.
2. Add `libs/observable-auth` to the root `Cargo.toml`'s `[workspace] members`.
3. In `services/ingest-gateway/src/auth.rs`: replace the inline bearer-extraction + the
   `state.validate_api_key` call's request-building with `observable_auth::verify_api_key`. Keep
   `ingest-gateway`'s own `TenantContext { tenant_id, role, environment }` struct and `can_ingest()`
   method — construct it from the returned `ApiKeyContext` (`tenant_id`, `role`, `environment` map
   directly). Add `observable-auth` as a path dependency in
   `services/ingest-gateway/Cargo.toml`.
4. Check whether `AppState::validate_api_key` (in `services/ingest-gateway/src/main.rs`, including
   its `#[cfg(test)] stub_tenant` branch used by existing tests) can be simplified to call
   `observable_auth::verify_api_key` directly, or whether the stub branch needs to stay for test
   convenience — preserve the existing test-stubbing capability either way; do not break
   `ingest-gateway`'s existing test suite.
5. Run `cargo test -p ingest-gateway -p observable-auth` and `cargo fmt --all`.

### Task 2: Migrate `query-api`'s session-cookie path

Depends on Task 1's crate existing. This task only touches the session path (the part of
`query-api/src/middleware/auth.rs` that already calls `auth-service` over HTTP) — leave the
API-key path for Task 3, which changes its behavior (the audit-gap fix) and should be reviewable
separately from this pure refactor.

1. Add `observable-auth` as a path dependency in `services/query-api/Cargo.toml`.
2. In `services/query-api/src/middleware/auth.rs`: replace `validate_session`'s body with a call to
   `observable_auth::verify_session`, mapping the returned `SessionContext` into query-api's own
   `TenantContext { tenant_id, user_id: Some(session.user_id), role: session.role }`. Replace the
   private `extract_session_cookie`/`extract_bearer_token`/`extract_tenant_id` functions' bodies
   with calls into the crate's public equivalents (or remove them and call the crate functions
   directly at each call site — whichever keeps the diff smaller and the call sites readable).
3. Run `cargo test -p query-api` (existing `session_auth_integration.rs` Testcontainers test and any
   unit tests in `auth.rs` must still pass unmodified in behavior) and `cargo fmt --all`.

### Task 3: Migrate `query-api`'s API-key path to close the audit gap

Depends on Task 2 (same file, sequential to avoid merge conflicts on `auth.rs`). This is the one
task in this plan with a real behavior change: today, `query-api`'s API-key checks query
`api_keys` directly and are never audited; after this task, they go through `auth-service`'s
`/internal/validate` (same endpoint `ingest-gateway` already uses), gaining the existing
`audit::write` allow/deny logging in `auth-service`.

1. Replace `query-api/src/middleware/auth.rs`'s `verify_credentials` function (the direct
   `sqlx::query_as` against `api_keys`) with a call to `observable_auth::verify_api_key`, passing
   the `auth_service_url` extension already present in the request (it's already wired for the
   session path — reuse it, no new extension needed).
2. After getting the `ApiKeyContext` back, preserve the existing tenant-ownership check: compare
   `ApiKeyContext.tenant_id` against the `X-Tenant-ID` header value already extracted, return
   `StatusCode::FORBIDDEN` on mismatch — `auth-service`'s `/internal/validate` doesn't take a
   tenant_id parameter, so this check must stay in query-api's middleware (mirrors what
   `ingest-gateway` does NOT need, since ingest-gateway has no concept of "requested tenant"
   distinct from "key's tenant").
3. Remove the now-dead local Postgres query and the `ApiKeyRow`/SHA-256-hashing code from
   `auth.rs` (the hashing now happens inside `auth-service`'s `lookup_api_key`, called over HTTP).
   The `PgPool` extension may become unused in this file if nothing else in it queries Postgres
   directly — check `require_tenant`'s cross-tenant-switch logic (the `user_tenant_roles` query)
   still needs it; if so, keep the `PgPool` extension requirement, just remove the now-dead
   API-key-specific query.
4. Update or add a Testcontainers/integration test asserting an API-key-authenticated query-api
   request now produces an `auth_audit_log` (or whatever the audit table is named — check
   `auth-service/src/audit.rs`) entry, mirroring whatever test (if any) already asserts this for
   ingest-gateway's API-key path. If no such test exists for ingest-gateway either, add one
   covering query-api's new behavior at minimum, and note the ingest-gateway gap as a follow-up
   rather than expanding this task's scope to add it there too.
5. Run `cargo test -p query-api` (full suite, including Testcontainers — this task changes runtime
   behavior, run everything, not just `auth.rs`'s own tests) and `cargo fmt --all`.

### Task 4: Documentation correction and closeout

Depends on Tasks 1-3 being complete (describes what was actually built, including the correction).

1. Update `spec/adr/ADR-033-admin-service-extraction.md`'s "Shared `observable-auth` crate" framing
   (and the Decision section's mention of it) to describe what's actually true: HTTP-client
   wrappers around `auth-service`'s two existing endpoints plus header-extraction helpers, not
   local JWT verification — and note the audit-gap fix as an additional outcome of this slice.
2. Update `docs/superpowers/specs/2026-06-19-admin-service-extraction-design.md`'s "Shared
   `observable-auth` crate" section the same way, and mark its Sequencing §'s step 1 as done with a
   pointer to this plan (once archived).
3. Add a line to `docs/agent-context.md` recording this slice's completion (libs/observable-auth
   extracted, query-api/ingest-gateway migrated, audit gap closed), matching the existing closeout
   style.
4. This plan does **not** close out the full roadmap §7 "Extract admin-service" item — only its
   first sub-slice. Leave that item's checkbox state as-is in
   `docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md`; note in agent-context that
   slice 1 of 3 is done and slices 2-3 (scaffold admin-service, move handlers) remain.
5. Move this plan file to `archived/plans/`.

No automated verification beyond confirming the doc edits render correctly — docs-only task.

## Verification (full slice)

- `cargo fmt --all --check` and `cargo test -p observable-auth -p ingest-gateway -p query-api`
  (Testcontainers tests need Docker).
- Manual: hit `query-api` with a valid API key + `X-Tenant-ID`, confirm the request succeeds and a
  new row appears in `auth-service`'s audit log; hit it with a mismatched tenant ID, confirm 403;
  hit `ingest-gateway` with a valid/invalid key, confirm unchanged 200/401/403 behavior.

## Rollback

- Standard code revert. No schema/migration changes. If the audit-gap fix (Task 3) causes
  unexpected latency or availability coupling (query-api's API-key auth now depends on
  auth-service being reachable, same as ingest-gateway already does), revert Task 3's commit only
  — Tasks 1-2 (crate extraction, session-path migration) are independently revertible and carry no
  behavior change risk.
