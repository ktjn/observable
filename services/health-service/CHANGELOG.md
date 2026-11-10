# Changelog

## [Unreleased]

### Added

- Phase 2 E6: the health read discloses the rule's `freshness_seconds` (null when withheld), so composed views can state the freshness bound.
- Phase 2 E4: data-quality issues (migration 072), reconciled by the evaluation worker per service and class. `source_stopped` opens when a bound identity had evidence within the lookback and none for one freshness bound, stays open while it is silent, and resolves when evidence returns or the identity is unbound. `inconsistent_inputs` opens for ambiguous catalog inputs and `evaluation_failed` when evidence cannot be read; both resolve on the next clean evaluation. Retired and planned services have no open issues. Tenant purge deletes issues.
- `GET /v1/health/services/{id}/issues?environment_class=` returns open issues and those resolved in the last 7 days, with `open` count. Checks follow the health read under `health.issue` (migration 073); denied evidence returns the restricted response. The health and history reads share these checks (`guard`).
- `GET /internal/health/self-status` (internal token; an unset token refuses every caller) reports the evaluation worker's last pass and counts with no tenant data. `state` is `stale` once the last pass is older than twice `HEALTH_EVALUATION_INTERVAL_SECONDS`, so a stopped worker is visible.
- Phase 2 E3: state transitions (migration 070) are recorded in the snapshot's transaction whenever the stored state changes, not on every evaluation. An evaluation gap longer than the freshness bound first records `unknown` (`stale`) at the moment the previous evidence aged out. Transitions are append-only for the health role; `HEALTH_HISTORY_RETENTION_DAYS` (default 400) prunes older rows after each pass, and tenant purge deletes them.
- `GET /v1/health/services/{id}/history?environment_class=&since=&until=` (default 7 days, at most 90) returns transitions and derived interruptions: onset, `worst_state`, `resolved` or `unresolved`, exact `ended_at` and `duration_seconds` only when known, otherwise `ended_after`/`ended_before` bounds and `evidence_gap`. Checks follow the health read: `decide-access` read on `health.history` (migration 071) before any lookup, the caller's catalog read, and every `health.evidence` contributor; a denied contributor returns the restricted response with no transitions.
- Phase 2 E2: maintenance windows (migration 068). `POST /v1/health/services/{id}/maintenance` requests a window for one environment class (at most 7 days, starting within 90 days, reason 1-500 characters); `POST /v1/health/maintenance/{id}/approve` and `/cancel`; `GET /v1/health/services/{id}/maintenance` lists windows with `pending`, `approved`, `active`, `expired` or `cancelled`. Writes need a user session and either `decide-access` write on `health.maintenance` (tenant_admin by default, migration 069) or being the service's active operational owner in person. The requester cannot approve; the schema enforces it too, and a trigger keeps scope, period, reason and recorded actors from being rewritten. Tenant purge deletes windows.
- During an approved, current, uncancelled window, `degraded` and `unavailable` are disclosed as `planned_maintenance` with `underlying_state` and a `maintenance` summary; `normal` and `unknown` are unchanged. The health read fails closed (`evaluation_unavailable`) when windows cannot be read.
- Phase 2 E1: rule conditions `error_ratio` and `latency_quantile` measure server spans (`span_kind = 'SERVER'`) of every identity in scope over `window_seconds`. A breached condition yields its state while its evidence is current: the worst breach wins, `unavailable` only from an error ratio of 1 (every request failing), latency only `degraded`. Fewer than `min_requests` requests is `insufficient` (state `unknown`), so absence of data is never `unavailable`. Conditions are `required` by default; snapshot conditions add `kind`, `outcome`, `value`, `requests` and `yields`.
- `GET /v1/health/rules` returns the caller tenant's effective rule (`decide-access` read on `health.rule`). `PUT /v1/health/rules` validates and publishes a new immutable revision (`tenant-{tenant_id}`) recording `published_by`; it needs `decide-access` write on `health.rule` (tenant_admin by default) and a user session. Migration 066 adds `published_by`, tenant rule id and publisher checks, and revokes UPDATE on `health.rules`; migration 067 adds the `health.rule` grants.
- Tenant purge: a background worker (`TENANT_PURGE_INTERVAL_SECONDS`, default 60) pulls pending requests for `health` from observable-control (`GET /internal/tenant-purges`, requires `ADMIN_SERVICE_URL`), deletes the tenant's rows in its own schema in one transaction as the scoped login, and acknowledges only after the commit. Platform-default rules are kept.
- Worker credential provisioning: `scripts/health-worker-credential.py issue|revoke` registers only the token digest and writes the plaintext to the worker's credentials file (0600). Helm `services.healthService.workerCredentials.existingSecret` mounts it and sets `HEALTH_WORKER_CREDENTIALS_FILE`; `scripts/local-stack.sh up` rotates a dev-tenant credential.
- Phase 1 C2.3 adds `GET /v1/health/services/{id}?environment_class=`. It authenticates the
  caller, checks `decide-access` on `health.assessment` before any lookup, authorizes the
  service through the caller's own catalog read, and withholds the whole assessment when a
  `health.evidence` contributor is denied. Missing, hidden, planned and retired services share
  one 404; policy and catalog outages return 503 with no health fields.
- The evaluation worker (`HEALTH_EVALUATION_INTERVAL_SECONDS`, worker credentials from
  `HEALTH_WORKER_CREDENTIALS_FILE`) stores one snapshot per tenant, service and class in
  migration 061, bound to the rule revision and the evaluated identities. An older evaluation
  never replaces a newer one, and freshness is judged at read time. The seeded
  `platform-default` rule yields only `normal` or `unknown`.
- Provider mock `mocks/assess-service-health.v0.1.mjs` with scenarios shared with the real
  contract test `tests/health_read_contract_integration.rs`.
- Scope treats `planned` services as non-live.

- Phase 1 C2.2 adds the worker's scoped ports: `CatalogInputs` reads catalog inputs with the
  tenant's own worker credential and discards answers for another tenant, `Evidence` reads the
  latest evidence per signal for an exact tenant, `service.name` and nonempty environment
  through a `readonly = 2` ClickHouse session, and `scope` selects a service's identities in one
  environment class. Evaluation wiring follows in C2.3.

## [0.1.0] - 2026-11-08

### Added

- Phase 1 C2.1 adds the health-service runtime with liveness, schema readiness and HTTP metrics.
- Migration 058 provisions the dedicated health schema and login. The health assessment API lands with the authorized provider in C2.3.
