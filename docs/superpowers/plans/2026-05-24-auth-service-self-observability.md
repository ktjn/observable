# auth-service Self-Observability Slice 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose `/readyz` and `/metrics` on `auth-service` with Prometheus-formatted HTTP metrics and dependency-aware readiness checks for PostgreSQL.

**Architecture:** Keep this slice narrow and representative. Implement the new observability surface inside `services/auth-service` so the identity service proves the probe contract against its real database dependency without expanding the scope to Zitadel health checks or cross-service refactors in the same iteration. Add a reusable observability module for request metrics and scrape output, keep `/health` unchanged as a cheap liveness probe, and make `/readyz` fail closed when the PostgreSQL connection is unavailable.

**Tech Stack:** Rust 2024, Axum 0.8, SQLx/PostgreSQL, Prometheus text exposition, tower::ServiceExt::oneshot, Testcontainers, Cargo workspace dependencies.

---

### Task 1: Add the auth-service observability module and route wiring

**Files:**
- Modify: `services/auth-service/Cargo.toml`
- Modify: `services/auth-service/src/lib.rs`
- Modify: `services/auth-service/src/oidc.rs`
- Modify: `services/auth-service/src/main.rs`
- Create: `services/auth-service/src/observability.rs`

**Source spec:** `spec/17-self-observability.md`

**Acceptance target:** `auth-service` exposes `/readyz` and `/metrics` alongside the existing `/health` route, and the new routes are public probes rather than tenant-authenticated API surface.

**User/operator outcome:** Operators can scrape `auth-service` for service-level request metrics and can tell whether the service can reach PostgreSQL before relying on login/session validation behavior.

**Out of scope:** Do not touch the other Rust services in this slice. Do not change Helm, Docker Compose, or the `/health` route contract yet. Do not add Zitadel liveness checks in this slice.

**Implementation shape:**
- Add `prometheus = { version = "0.14.0", features = ["process"] }` to the `auth-service` crate.
- Add a `services/auth-service/src/observability.rs` module that owns:
  - a Prometheus `Registry`
  - request counters broken down by `method` and `status`
  - request duration histograms broken down by `method` and `status`
  - a render helper that emits Prometheus text format
  - a thin Axum middleware hook for increment/observe/decrement
- Extend `oidc::OidcState` with an `Arc<observability::AuthServiceMetrics>` so handlers and middleware share the same registry.
- Wire `GET /readyz` to run a minimal PostgreSQL connectivity check and return `200 OK` only when the database responds.
- Wire `GET /metrics` to return Prometheus text with `Content-Type: text/plain; version=0.0.4; charset=utf-8`.

**Verification:**
- `cargo fmt --all`
- Focused auth-service tests for `/readyz` and `/metrics`
- `bash scripts/local-ci.sh --skip-docker --skip-frontend --skip-helm --skip-smoke` if Docker/Node are unavailable

**Baseline:** `auth-service` currently has `/health` only; readiness and Prometheus exposition are absent.

**New errors introduced:** none expected. The probe routes must return 5xx on dependency failure rather than panic.

**Telemetry impact:** Adds independent scrapeable service metrics for request rate, error rate, duration, and in-flight requests. Recursive OTLP telemetry remains unchanged.

**Auth/tenancy impact:** `/readyz` and `/metrics` stay public and must not require tenant context or auth headers.

**Data retention or migration impact:** none.

**Rollback path:** Remove the new `observability` module and the new routes; `/health` stays intact and the service regresses cleanly to the previous state.

**ADR/spec sync:** No ADR change required. This slice implements existing self-observability requirements in `spec/17-self-observability.md` and the deployment guidance in `spec/12-deployment.md` without changing the architecture decision set.

**Checkpoint question:** does the first auth-service observability slice prove the probe contract on the identity service's database dependency, or do we need to widen the slice before touching the remaining services?

**Next smallest slice:** apply the same `/readyz` + `/metrics` pattern to `storage-writer`, then other simple HTTP services, before adding queue-backed readiness checks.

### Task 2: Add HTTP integration coverage for probe behavior

**Files:**
- Modify: `services/auth-service/tests/http_api_integration.rs`

**Source spec:** `spec/17-self-observability.md`

**Acceptance target:** The new probe routes are covered end-to-end through `tower::ServiceExt::oneshot`.

**User/operator outcome:** Future refactors cannot silently drop the probe routes or change readiness semantics without failing tests.

**Out of scope:** Do not add browser, frontend, or dashboard tests in this slice.

**Test cases to add:**
- `/readyz` returns `200 OK` when the seeded PostgreSQL container is reachable.
- `/readyz` returns `503 Service Unavailable` when PostgreSQL is intentionally pointed at an unavailable endpoint.
- `/metrics` returns `200 OK`, `Content-Type` is Prometheus text, and the body includes the auth-service request metric names.

**Implementation notes for the tests:**
- Reuse the existing Testcontainers helpers in the auth-service test suite.
- Build a minimal router with the new auth-service observability state and call it through `oneshot`.
- Assert on the response status and a small stable substring of the Prometheus payload rather than the whole exposition body.

**Verification:**
- `cargo test -p auth-service --test http_api_integration readyz -- --nocapture`
- `cargo test -p auth-service --test http_api_integration metrics -- --nocapture`
- `cargo test -p auth-service --test http_api_integration -- --nocapture`

**Baseline:** No probe coverage exists today.

**New errors introduced:** none expected.

**Telemetry impact:** Confirms the scrape endpoint exposes the intended request metrics and that the readiness probe fails closed.

**Auth/tenancy impact:** Probe routes remain unauthenticated by design.

**Data retention or migration impact:** none.

**Rollback path:** Delete the probe tests if the route contract is intentionally reverted.

**ADR/spec sync:** No ADR change required.

**Checkpoint question:** do the probe tests exercise the real router path strongly enough to catch route-layer regressions without needing extra fixtures?

**Next smallest slice:** extend the same pattern to `storage-writer` so the remaining simple HTTP services share the same ready/scrape contract.
