# Admin-Service Cleanup Implementation Plan (Slice 3 of 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Delete the four handler modules from `query-api` that Slice 2 duplicated into
`admin-service` (`admin_members.rs`, `tokens.rs`, `config.rs`, `usage.rs`) — they are dead code
today, unreachable since ingress routes their paths to `admin-service`. This completes ADR-033's
two-step rollout. Promoted now per explicit user request, ahead of the design doc's suggested
"wait for a production deploy cycle" caveat — that caveat exists for production safety; this repo
has no live production deployment yet, so the wait condition doesn't apply.

**Architecture:** Pure deletion + cleanup, no new behavior. `admin-service` already serves these
paths in production (per ingress routing, Slice 2); query-api's copies have had zero traffic since
that routing change. Removing them shrinks query-api and removes the duplication-drift risk the
final Slice 2 review flagged as the cost of the two-step rollout.

## Global Constraints

- Do **not** touch `services/query-api/src/middleware/auth.rs` (`require_tenant`/`TenantContext`)
  — it's used by 18+ other still-live query-api handlers (traces, logs, metrics, alerts, incidents,
  SLOs, notifications, NLQ, etc.), not just the four modules being deleted.
- Do **not** remove `OpenAiLlmCaller` from `services/query-api/src/llm_adapter.rs` — confirmed
  still used by `main.rs`'s `/v1/nlq` chat path (`from_env`) and `llm_adapter.rs`'s own
  `handle_nlq_query` (`from_key` fallback). Only `config.rs`'s call site to it disappears with that
  file's deletion.
- `services/admin-service/` is the new source of truth for these four feature areas going
  forward — do not delete or modify anything there in this slice, only query-api.
- `cargo fmt --all` after every Rust edit, before staging.
- After deleting `tokens.rs` (the only one of the four using compile-time-checked `sqlx::query!`
  macros), regenerate query-api's `.sqlx/` offline cache via `cargo sqlx prepare` against a real
  Postgres connection rather than hand-pruning specific cache files — safer than guessing which
  cache entries were uniquely attributable to the deleted file.

### Task 1: Delete the four handler modules and their wiring

1. Delete `services/query-api/src/admin_members.rs`, `tokens.rs`, `config.rs`, `usage.rs`.
2. `services/query-api/src/lib.rs`: remove the `mod admin_members;`, `mod tokens;`, `mod config;`,
   `mod usage;` declarations (confirmed at lines 1, 5, 25, 27 — re-check exact line numbers at
   implementation time, the file may have shifted since this plan was written).
3. `services/query-api/src/main.rs`: remove the route registrations for these four modules —
   confirmed at approximately lines 139-151 (admin_members), 173-176 (usage), 217-223 (config),
   224-229 (tokens). Remove only these specific `.route(...)` calls; leave every other route
   (traces, logs, metrics, alerts, incidents, SLOs, notifications, NLQ, schemas, dashboards,
   discovery, reliability) completely untouched.
4. `services/query-api/Cargo.toml`: remove the `rand` and `sha2` dependencies — confirmed used
   only inside `tokens.rs` (verify this yourself before removing; if either turns out to have a
   second, non-obvious call site elsewhere in query-api, keep it and note why in your report rather
   than silently leaving it removed and breaking the build).
5. Run `cargo build -p query-api` to confirm it compiles (it should fail in obvious ways until
   step 6's test cleanup, if test files still reference the deleted modules — that's expected and
   handled by Task 2, but get the **library** crate building clean first as a checkpoint).
6. Commit your work with a descriptive message (do not push yet — Task 2 needs to land before
   `cargo test` can pass cleanly, and splitting across two commits keeps each one buildable enough
   to bisect if something goes wrong, but both tasks' commits will need to ship together in this
   PR regardless).

### Task 2: Delete the now-dead tests and regenerate the sqlx cache

Depends on Task 1 (the source modules must already be gone, otherwise you can't tell which test
helpers became orphaned by checking for compile errors).

1. Delete `services/query-api/tests/postgres_tokens_integration.rs` and
   `postgres_config_integration.rs` wholesale — both are dedicated entirely to the deleted modules.
2. In `services/query-api/tests/http_api_integration.rs` (large shared file), surgically remove:
   - The usage-report tests (`get_tenant_usage_report_scopes_to_tenant_and_interval`,
     `get_tenant_usage_report_returns_zeroes_for_empty_interval`, confirmed around lines 2026-2256)
     and any setup helpers used only by them.
   - The admin_members test block (`build_admin_members_app` helper and its ~9 test functions,
     confirmed running from around line 2254 to the file's end) and any setup helpers
     (`seed_user`, `seed_user_with_id`, `seed_member`) used only by them.
   - `llm_models_returns_ok_false_when_unreachable` (confirmed around lines 1270-1314) — this test
     exercises `config::list_llm_models` directly and was missed by the Slice 2 plan's original
     test-relocation list; it must go now since `config.rs` no longer exists in query-api.
   - Read the surrounding context carefully before deleting each block — confirm via `cargo build
     --tests -p query-api` that you're removing exactly the right boundaries (a leftover reference
     to a deleted module is a compile error, which is your correctness signal here) and not
     accidentally deleting a helper or test that's still used by something else in this large
     shared file.
3. Run `cargo build --tests -p query-api` until it's clean — this is the main correctness check
   for this task. Iterate: each remaining compile error names exactly what still references deleted
   code.
4. Run `cargo sqlx prepare` (or whatever the project's established invocation is — check
   `AGENTS.md`/`docs/agent-context.md`/CI config for the exact command this repo uses, since `sqlx`
   offline-cache regeneration needs a live Postgres connection and the right env vars) against a
   real Postgres instance to regenerate `services/query-api/.sqlx/`, removing any cache entries that
   are no longer referenced by any remaining `query!`/`query_as!`/`query_scalar!` macro in the
   crate.
5. Run the full `cargo test -p query-api` suite (Testcontainers-backed, needs Docker) to confirm
   nothing else broke, and `cargo fmt --all`.
6. Commit your work with a descriptive message (do not push).

### Task 3: Documentation closeout

Depends on Tasks 1-2 (describes what was actually deleted).

1. `docs/agent-context.md`: update the "Tenant Usage Report" section (or wherever usage.rs is
   referenced as living in query-api — confirmed around lines 138-144) to point at
   `services/admin-service/src/usage.rs` instead. Apply the same correction anywhere else
   `admin_members.rs`/`tokens.rs`/`config.rs` are described as query-api files.
2. Append a Slice 3 completion note to the existing Slice 2 entry in `docs/agent-context.md`
   (confirmed around line 27) rather than rewriting it — e.g. "Slice 3 complete 2026-06-20:
   query-api's now-dead duplicate handlers removed; admin-service is the sole implementation."
3. `spec/adr/ADR-033-admin-service-extraction.md`: update the status header from "Partially
   Implemented (Slices 1-2 of 3 complete...)" to "Implemented (all 3 slices complete, 2026-06-20)".
4. `docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md`: change the "Extract admin-service"
   line's `[~]` (in-progress) marker to `[x]` (done), noting all three slices complete.
5. Move this plan file to `archived/plans/`.

No automated verification beyond confirming the doc edits render correctly — docs-only task.

## Verification (full slice)

- `cargo build -p query-api` and `cargo build --tests -p query-api` clean.
- `cargo test -p query-api` full suite passing (Testcontainers needs Docker).
- `cargo fmt --all --check`.
- Manual: confirm `services/admin-service` is unaffected (not touched by this slice) — a quick
  `cargo build -p admin-service` is a sufficient sanity check.

## Rollback

- This is a deletion-only slice with no new runtime behavior — if something goes wrong, `git
  revert` restores query-api's deleted modules exactly as they were (still dead code in
  production, since ingress still points at admin-service either way; reverting this slice doesn't
  change production routing at all, only what's compiled into the query-api binary).
