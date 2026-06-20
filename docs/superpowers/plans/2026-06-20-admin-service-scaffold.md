# Admin-Service Scaffold Implementation Plan (Slice 2 of 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Scaffold a new standalone service, `services/admin-service`, and move the four
privilege-granting modules (`admin_members.rs`, `tokens.rs`, `config.rs`, `usage.rs`) into it as
new code — duplicated, not deleted from `query-api` yet — then point ingress routing at the new
service. Roadmap: Slice 2 of 3 of the admin-service extraction (ADR-033), continuing
`archived/plans/2026-06-20-observable-auth-crate.md` (Slice 1, merged).

**Architecture:** Per `docs/superpowers/specs/2026-06-19-admin-service-extraction-design.md`'s
two-step rollout design, this slice is additive: admin-service gets its own copies of the four
handler modules and their tests, query-api's existing copies are left in place (now dead once
ingress stops routing to them), and only ingress routing changes production behavior. Removing the
dead code from query-api is Slice 3, deliberately separate so a routing-rule revert is the
rollback path if admin-service misbehaves in production, without needing to restore deleted code.

**Tech Stack:** Rust/Axum, matching `services/auth-service`'s scaffold exactly (it's the smallest
existing service and the closest structural template).

## Global Constraints

- New crate at `services/admin-service`, registered in root `Cargo.toml`'s `[workspace] members`.
- `AdminServiceAppState { db: PgPool, ch: clickhouse::Client, auth_service_url: String, metrics: Arc<AdminServiceMetrics> }`
  — `ch` is needed only by `usage.rs`; the other three modules use `db` only. No `planner`/`llm`
  fields (those are query-api's NLQ-specific `AppState` fields, not used by any of the four moved
  modules).
- `require_tenant` middleware and `TenantContext` (currently `services/query-api/src/middleware/auth.rs`,
  158 lines, calling `observable_auth::verify_api_key`/`verify_session`) must be duplicated into
  `services/admin-service/src/middleware/auth.rs` verbatim — `libs/observable-auth` (Slice 1)
  provides the low-level HTTP wrappers and header helpers this file calls, but not the axum
  middleware function itself. Do not modify `observable-auth` in this slice; if the duplication
  reveals the axum-middleware layer itself should move into the shared crate too, note it as a
  follow-up rather than doing it now (scope discipline — that's a Slice-1-adjacent decision, not
  this slice's job).
- `admin_members.rs`'s `require_admin(&TenantContext)` role guard (currently
  `services/query-api/src/admin_members.rs` lines 57-63) moves as-is into admin-service — per the
  design doc, this guard stays admin-service-scoped, not promoted into `observable-auth`.
- `config.rs::list_llm_models` needs `OpenAiLlmCaller::from_key`/`list_models` from
  `services/query-api/src/llm_adapter.rs` (lines 52-98) — **do not** depend on the whole 2819-line
  `llm_adapter.rs` module (it's almost entirely NLQ/AI logic admin-service has no other use for).
  Duplicate only the minimal piece: a small `OpenAiLlmCaller` struct with `from_key`/`list_models`
  (no `LlmCaller` trait impl, no `async_trait`, no NLQ imports — just `async-openai`'s
  `Client`/`OpenAIConfig` directly), in a new `services/admin-service/src/llm_probe.rs`.
- Self-observability: no shared `observable-observability` crate exists yet (confirmed, still
  Deferred-tier per the roadmap) — admin-service's `src/observability.rs` is a fresh copy-paste of
  `services/auth-service/src/observability.rs`'s pattern (`AdminServiceMetrics` with
  `http_requests_total`/`http_request_duration_seconds`/`http_in_flight_requests`, `readyz()`,
  `metrics()`, `record_http_metrics` middleware), renamed, not a new shared abstraction.
- Frontend requires zero code changes (confirmed: `apps/frontend/src/api/admin-members.ts` and
  related call sites use relative paths only, no hardcoded query-api host/port anywhere in
  `apps/frontend/src`).
- Each handler-module task in this plan only **adds** files to `services/admin-service/` — it does
  not touch `services/query-api/` at all (no risk to the running production path until the final
  ingress task). The final ingress task is the only one that changes routing behavior.
- `cargo fmt --all` after every Rust edit, before staging.
- Testcontainers tests for each moved module go into `services/admin-service/tests/`, following
  the exact existing pattern of the corresponding query-api test file (don't invent new test
  infrastructure).
- **`postgres_tenants_integration.rs` does NOT relocate** — verified its test bodies
  (`list_tenants_returns_seeded_tenant`, `list_tenant_environments_*`) exercise `discovery.rs`'s
  tenant-listing reads, not any of the four modules moving in this slice. It stays in query-api.
  (The design doc's sequencing notes listed it as relocating; that was inaccurate — this plan
  corrects it, matching the same kind of premise-check Slice 1 already did once.)

### Task 1: Scaffold `admin-service` with no handlers (health/readyz/metrics only)

Prove the skeleton builds, runs, and is wired into the workspace/Docker/compose stack before any
handler logic moves — isolates infrastructure risk from business-logic risk.

1. `services/admin-service/Cargo.toml`: lib+bin split matching `services/auth-service/Cargo.toml`'s
   exact dependency style (`domain = { path = "../../libs/domain" }`,
   `observable-auth = { path = "../../libs/observable-auth" }`, workspace-inherited
   `axum`/`tokio`/`sqlx`/`clickhouse`/`serde`/`uuid`/`reqwest`/`tracing`/`anyhow`,
   `prometheus = { version = "0.14.0", features = ["process"] }`, `async-openai` for the LLM probe
   in a later task). Dev-deps: `testcontainers 0.27.3`, `testcontainers-modules 0.15.0` with
   `features = ["postgres", "clickhouse"]` (usage.rs needs both containers in tests).
2. `services/admin-service/src/main.rs`: `domain::telemetry::init_self_observability_telemetry("admin-service")`,
   build `AdminServiceAppState`, `Router::new().route("/health", ...).route("/readyz", get(observability::readyz)).route("/metrics", get(observability::metrics))`,
   `.layer(axum::middleware::from_fn_with_state(state.clone(), observability::record_http_metrics))`,
   `.layer(TraceLayer::new_for_http()...)` — mirror `services/auth-service/src/main.rs`'s structure
   exactly for these parts. No business routes yet.
3. `services/admin-service/src/observability.rs`: `AdminServiceMetrics` + `readyz`/`metrics`/
   `record_http_metrics`, copy-pasted from `services/auth-service/src/observability.rs` with names
   renamed (per Global Constraints — no shared crate exists to depend on instead).
4. Add `"services/admin-service"` to root `Cargo.toml`'s `[workspace] members`.
5. `Dockerfile`: add `admin-service` to the `rust-builder` stage's binary copy block (alongside
   `auth-service`/`query-api`, lines ~45-53) and the `runtime` stage's `COPY --from=rust-builder`
   lines (~69-74).
6. `docker-compose.yml`: add an `admin-service:` block mirroring `auth-service:`'s pattern
   (lines 183-212) — `image: observable-services:local`, same `build`, `command: ["admin-service"]`,
   `environment: { <<: *platform-self-observability-env, DATABASE_URL: ..., CLICKHOUSE_URL/USER/PASSWORD: ...,
   AUTH_SERVICE_URL: http://auth-service:4319, ADMIN_SERVICE_PORT: "<pick an unused port>" }`,
   `healthcheck` against `/health`, `depends_on: { postgres-setup: ..., auth-service: { condition: service_healthy } }`
   (admin-service needs auth-service up for credential checks, same dependency `query-api` already
   has — check query-api's `depends_on` block, lines ~279+, for the exact pattern to mirror,
   including ClickHouse setup dependency).
7. Verify: `cargo build -p admin-service`, then `docker compose build admin-service` and
   `docker compose up admin-service -d` locally if Docker is available, confirming `/health` and
   `/readyz` respond. If full compose stack verification isn't practical in this environment,
   `cargo build`/`cargo run -p admin-service` with manually-set env vars is sufficient — note which
   verification level was actually performed in the report.

### Task 2: Move `admin_members.rs` + shared middleware

Depends on Task 1 (needs `AdminServiceAppState`/`observability.rs` to exist). This task also
creates the shared `middleware/auth.rs` duplication that Tasks 3-5 will reuse — do this one first
since it's the natural place to establish that file.

1. Duplicate `services/query-api/src/middleware/auth.rs` (158 lines) into
   `services/admin-service/src/middleware/auth.rs` verbatim, adjusting only the `AppState` type it
   references (query-api's `AppState` → admin-service's `AdminServiceAppState`) and any import
   paths. No logic changes — this is the same `require_tenant`/`TenantContext`/`validate_session`/
   `verify_credentials` code, just recompiled against admin-service's smaller state.
2. Copy `services/query-api/src/admin_members.rs` (287 lines) to
   `services/admin-service/src/admin_members.rs`, adjusting imports (`crate::traces::AppState` →
   `crate::AdminServiceAppState` or wherever it's defined, `crate::middleware::auth::TenantContext`
   stays the same shape but now resolves to admin-service's own copy). Move the `require_admin`
   guard (lines 57-63) along with it — it's local to this file already.
3. Wire the 5 routes (`GET/POST /v1/admin/members`, `PUT/DELETE/POST .../{user_id}/...`) into
   `main.rs`, matching query-api's exact route table (`services/query-api/src/main.rs` lines
   139-152) but with admin-service's own state.
4. Extract the admin_members test functions currently embedded in
   `services/query-api/tests/http_api_integration.rs` (the `build_admin_members_app` helper at
   line 2256 and the tests from line 2327 onward — `list_members_returns_tenant_members`,
   `add_member_by_email_succeeds_for_known_user`, `add_member_returns_404_for_unknown_email`,
   `update_role_changes_member_role`, `update_role_returns_403_for_self`, and any others in that
   block) into a new `services/admin-service/tests/admin_members_integration.rs`, adapted to build
   an admin-service `Router` instead of query-api's. **Do not delete the originals from
   query-api's test file in this task** — query-api's copy of the handler still exists and is
   still tested until Slice 3 removes it; duplicating the test, not moving it, keeps both code
   paths covered during the additive rollout.
5. Verify: `cargo test -p admin-service` (new tests pass) and `cargo test -p query-api` (existing
   tests still pass unmodified) and `cargo fmt --all`.

### Task 3: Move `tokens.rs`

Depends on Task 2 (reuses `middleware/auth.rs`).

1. Copy `services/query-api/src/tokens.rs` (305 lines) to `services/admin-service/src/tokens.rs`,
   adjusting imports as in Task 2.
2. Wire the 6 routes (`GET/POST /v1/tokens`, and the `{id}` sub-routes for revoke/renew/restore/
   permanent-delete) into `main.rs`, matching `services/query-api/src/main.rs` lines 224-229.
3. Copy `services/query-api/tests/postgres_tokens_integration.rs` to
   `services/admin-service/tests/postgres_tokens_integration.rs` wholesale (this file is dedicated
   to tokens.rs, confirmed via research — a clean relocation, not an extraction like Task 2's).
   Adapt only the app-builder helper to admin-service's state/router. Leave the original in
   query-api untouched (same additive-rollout reasoning as Task 2).
4. Verify: `cargo test -p admin-service -p query-api` and `cargo fmt --all`.

### Task 4: Move `config.rs` + minimal LLM probe

Depends on Task 2.

1. Create `services/admin-service/src/llm_probe.rs`: a minimal `OpenAiLlmCaller` struct with
   `from_key(api_key, url, model) -> Self` and `async fn list_models(&self) -> Result<Vec<String>, String>`,
   ported from `services/query-api/src/llm_adapter.rs` lines 52-98 — **omit** the `LlmCaller` trait
   impl (`call`, lines 100+) and its `async_trait`/NLQ-specific imports entirely; admin-service only
   needs the connectivity-probe/model-listing behavior `config.rs::list_llm_models` calls, not the
   chat-completion path.
2. Copy `services/query-api/src/config.rs` (339 lines) to `services/admin-service/src/config.rs`,
   changing its `use crate::llm_adapter::OpenAiLlmCaller;` (line 190) to
   `use crate::llm_probe::OpenAiLlmCaller;`.
3. Wire the 4 routes (`GET /v1/config`, `PUT /v1/config/llm`, `POST /v1/config/llm/models`,
   `PUT /v1/config/llm-key`) into `main.rs`, matching `services/query-api/src/main.rs` lines
   217-223.
4. Copy `services/query-api/tests/postgres_config_integration.rs` to
   `services/admin-service/tests/postgres_config_integration.rs` wholesale (dedicated file,
   confirmed clean relocation). Adapt the app-builder helper only.
5. Verify: `cargo test -p admin-service -p query-api` and `cargo fmt --all`.

### Task 5: Move `usage.rs`

Depends on Task 2. Independent of Tasks 3-4 (no shared new files beyond `middleware/auth.rs`).

1. Copy `services/query-api/src/usage.rs` (231 lines) to `services/admin-service/src/usage.rs`,
   adjusting imports. This is the only one of the four modules using `state.ch` (ClickHouse) —
   confirm `AdminServiceAppState`'s `ch` field (added in Task 1) is wired through correctly.
2. Wire the 1 route (`GET /v1/tenants/usage-report`) into `main.rs`, matching
   `services/query-api/src/main.rs` lines 173-176.
3. Extract the usage-report test functions from `services/query-api/tests/http_api_integration.rs`
   (`get_tenant_usage_report_scopes_to_tenant_and_interval` at line 2026,
   `get_tenant_usage_report_returns_zeroes_for_empty_interval` at line 2226, and their shared setup
   helpers) into `services/admin-service/tests/usage_integration.rs`, adapted to admin-service's
   Router/state — same extraction approach as Task 2, originals stay in query-api for now.
4. Verify: `cargo test -p admin-service -p query-api` and `cargo fmt --all`.

### Task 6: Wire ingress routing

Depends on Tasks 2-5 (all four modules must exist and work in admin-service before traffic is
routed to it). This is the only task in this slice that changes production routing behavior.

1. `apps/frontend/nginx.conf`: add `location ^~` blocks for `/v1/admin/`, `/v1/tokens`,
   `/v1/tenants/config`, `/v1/tenants/usage-report`, each `proxy_pass http://admin-service:<port>`
   with the same `proxy_set_header` lines as the existing `/v1/auth/` block (lines 12-18) — these
   `^~` blocks must appear **before** the generic `location /v1/ { proxy_pass http://query-api:8090; }`
   block (line 20) so nginx's prefix-match-with-`^~` precedence routes these paths to admin-service
   instead of falling through to the catch-all.
   - Note: `/v1/tenants/config` and `/v1/tenants/usage-report` are sub-paths of the broader
     `/v1/tenants` prefix that query-api's `discovery.rs` also serves (tenant listing, environments)
     — these two specific `^~` blocks must be scoped exactly to those two paths, not a blanket
     `/v1/tenants/` block, or they'd wrongly capture `discovery.rs`'s still-query-api-served routes
     too. Verify the nginx config's location-matching behavior handles this (longest/most-specific
     `^~` prefix wins) before considering this task done — a wrong match here misroutes the
     tenant-discovery endpoints, a regression worse than not moving anything.
2. Confirm `docker-compose.yml`'s frontend/nginx service (if it templates `nginx.conf` via env vars
   for service hostnames, vs. a static file) doesn't need a corresponding env var added — check how
   the existing `/v1/auth/` block's `auth-service` hostname is resolved (static hostname matching
   the compose service name, most likely, given the file doesn't look templated) and follow the
   same static-hostname approach for `admin-service`.
3. Verify: with the full `docker compose up` stack running (if Docker is available in this
   environment), exercise one endpoint per moved module through the frontend's nginx proxy (e.g.
   `curl http://localhost:<frontend-port>/v1/admin/members` with a valid session/API key) and
   confirm it reaches admin-service, not query-api (check each service's logs or add a temporary
   distinguishing log line). If full-stack verification isn't practical here, verify by reading the
   nginx config's resulting location-matching order against the four new paths and the existing
   catch-all, and note this limitation explicitly in the report — do not claim end-to-end
   verification you didn't actually perform.

## Verification (full slice)

- `cargo build --workspace` (note: building the *entire* workspace at once has previously shown an
  environment-specific `rustc` crash unrelated to code correctness in this repo — if that recurs,
  fall back to `cargo build -p admin-service -p query-api -p auth-service` etc. individually, which
  has reliably worked).
- `cargo test -p admin-service -p query-api` (Testcontainers tests need Docker).
- `cargo fmt --all --check`.
- Manual/docker-compose: confirm admin-service's `/health`/`/readyz`/`/metrics` respond, and that
  ingress routes the four path groups to admin-service per Task 6's verification step.

## Rollback

- Tasks 1-5 are purely additive (new files only) — reverting any of them is a standard code revert
  with zero production impact, since nothing routes to admin-service until Task 6.
- Task 6 (ingress routing) is the one task with real rollback risk: if admin-service misbehaves
  in production after this slice ships, revert `apps/frontend/nginx.conf`'s new `location` blocks
  to route those paths back to `query-api` (whose handlers are still present and functional,
  not yet removed — that removal is Slice 3, deliberately deferred past this rollback window).
