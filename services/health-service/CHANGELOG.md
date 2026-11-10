# Changelog

## [Unreleased]

### Added

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
