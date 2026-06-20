# ADR-033: Admin Service Extraction

**Date:** 2026-06-19
**Status:** Proposed
**Authors:** Claude (architecture review), ktjn
**Deciders:** Project Stakeholders
**Review date:** 2026-06-19

## Context

Member management, API key/token lifecycle, and platform config (including LLM provider keys)
currently live in `query-api` (`admin_members.rs`, `tokens.rs`, `config.rs`, `usage.rs`,
~1,260 lines), sharing that service's process, `AppState`, and deployment cadence with the
trace/log/metric read path — the platform's highest-traffic, most latency-sensitive surface.
These admin operations are privilege-granting (they can mint API keys and assign `tenant_admin`
roles), which is a materially different trust level than read-only telemetry queries.

This was identified during a 2026-06-19 service-layer architecture review
(`docs/superpowers/specs/2026-06-19-service-layer-architecture-review.md`, Finding 7) and detailed
in `docs/superpowers/specs/2026-06-19-admin-service-extraction-design.md`.

## Decision

Extract member management, token/API-key lifecycle, platform config, and usage reporting into a
new standalone service, `services/admin-service`, deployed and scaled independently from
`query-api`. It becomes the sole writer of `user_tenant_roles`, `api_keys`, and `platform_config`;
`auth-service` keeps its existing read-only role validating against those tables — no contract
change there. URL paths are unchanged; only ingress routing targets change, so the frontend
requires no code changes.

As part of the same slice sequence, extract a shared `observable-auth` crate (`libs/observable-auth`,
completed 2026-06-20, see `archived/plans/2026-06-20-observable-auth-crate.md`). **Correction to
this ADR's original framing:** neither `query-api` nor `ingest-gateway` did local session-JWT
verification — both already delegated to `auth-service` over HTTP. The crate instead provides
shared HTTP-client wrappers around `auth-service`'s existing `/internal/validate` and
`/internal/validate-session` endpoints, plus the bearer/cookie header-extraction helpers that were
genuinely duplicated. Building this surfaced a real bug along the way: `query-api`'s API-key path
bypassed `auth-service` entirely and queried `api_keys` directly, producing no audit trail; it now
routes through `/internal/validate` like `ingest-gateway` already did, closing that gap.

## Consequences

**Easier:**
- Privilege-granting code (member/role management, key issuance) has its own process and trust
  boundary, separate from the high-traffic read path.
- query-api shrinks by ~1,260 lines, reducing its blast radius independent of the NLQ/AI
  extraction also recommended by the same review.
- Auth-service-delegated credential checks (API-key and session) have one canonical client
  implementation (`libs/observable-auth`) instead of duplicated header-extraction and divergent
  `TenantContext` shapes across query-api and ingest-gateway. (Not "one canonical JWT
  implementation instead of three" as originally stated here — see the correction above.)

**Harder:**
- One more service to deploy, monitor, and version; admin-service needs its own self-observability
  wiring (readyz/metrics) matching the pattern other services already have.
- Migration requires a two-step rollout (deploy admin-service, flip ingress routing, then remove
  the now-dead handlers from query-api) rather than a single atomic change.
- Closing query-api's API-key audit-trail gap (see above) means every API-key-authenticated
  query-api request now makes a network round-trip to auth-service instead of a local Postgres
  query — an availability coupling query-api didn't previously have on this path.
  `ingest-gateway` already has this same coupling for its own API-key checks, so this brings
  query-api in line rather than introducing a new class of risk.

**Constrained:**
- Future admin features (e.g., the planned Fleet Management UI, Admin Console RBAC/quota views)
  should target admin-service, not query-api, once this lands.

## Alternatives Considered

### Option A: Leave admin handlers in query-api, add internal isolation only
Use a separate `AppState` sub-struct or a stricter middleware boundary within the same query-api
binary. Rejected: this doesn't achieve actual privilege isolation — a memory-safety or
auth-bypass bug anywhere in the same process still has the same blast radius regardless of
internal module boundaries, since it's one trust domain at the OS/network level.

### Option B: Merge admin surface into auth-service instead of a new service
auth-service already owns sessions/roles/API key validation, so member/token/config management
could fold into it rather than creating a new service. Rejected: auth-service's current scope is
narrowly read-path validation (`/internal/validate`) plus session issuance; folding in
read-write admin management would itself turn auth-service into a second oversized service and
mix two different concerns (runtime authn/authz checks vs. privilege administration) in one
process — the opposite of the isolation this decision is trying to achieve.

## Related

- `docs/superpowers/specs/2026-06-19-service-layer-architecture-review.md` (Finding 7)
- `docs/superpowers/specs/2026-06-19-admin-service-extraction-design.md` — full design,
  architecture diagram, rollout/rollback plan
- `docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md` §7 — backlog entry
- ADR-008 (Authorization Model) — RBAC model is unaffected by this split, only relocated
- ADR-004 (Rust for Data Plane Services) — admin-service is Rust/Axum, consistent
