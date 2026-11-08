# Changelog

All notable changes to the `observable-catalog` component are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

- Phase 1 C1 adds `GET /v1/catalog/services/{id}/bindings` for authorized reads of
  stored telemetry bindings without running a coverage assessment.

Providers and dependencies (phase 1 S7).

### Added

- Migration `056_create_catalog_providers.sql`: `catalog.providers` (kind, materiality, contract
  reference, contract owner, classification) and `catalog.provider_dependencies` (a service or
  journey function depending on a provider, possibly not yet registered).
- `manage-providers`: `/v1/catalog/providers` (list, create), `/v1/catalog/providers/{id}` (get
  with dependent functions and contract owner state, replace, delete),
  `/v1/catalog/providers/dependencies` (create by provider id or name, delete),
  `/v1/catalog/services/{id}/dependencies`, `/v1/catalog/journeys/{id}/dependencies`.
  Registering a provider links dependencies of that name; deleting one makes them unregistered.
- `GET /v1/catalog/providers/gaps`: `unregistered_provider` for critical functions, and
  `missing_contract_owner`, `contract_owner_departed`, `contract_owner_unresolved` and
  `unmapped_material_provider` for material providers. Critical services with an unregistered
  provider get the exception `unregistered_provider`.
- Access on `catalog.provider` via `decide-access`; dependents and providers the caller may not
  read are withheld, never identified or counted.

### Changed

- The `observability-asset` data model is folded into `coverage-record`, so the catalog keeps
  four owned data models with the new `provider-record` (COMP-03).

Asset producers (phase 1 S6).

### Added

- `POST /internal/catalog/assets/sync` (internal token): a producer reports the full set of its
  assets of one kind for a tenant; listed assets whose service resolves are registered with their
  owner, the producer's other assets of that kind are retired, and keys held by another component
  are never taken over. Returns `{registered, skipped, retired}`. Used by observable-alerting.
- Control dashboards are pulled from `GET /internal/observability-assets` on admin-service
  (`manage-control-plane`) every `CATALOG_ASSET_PULL_INTERVAL_SECONDS` (default 60) for tenants
  with catalog services; a failed read leaves the tenant's registry unchanged.

### Changed

- A producer identity without an environment resolves only when its bindings name exactly one
  service. `owning_component` `catalog` is reserved for assets registered through the API.

Monitoring coverage and the observability asset registry (phase 1 S5).

### Added

- Migration `053_create_catalog_coverage.sql`: telemetry bindings, coverage policies with
  approval, observability assets, coverage claims, assessment runs and items, and coverage gaps.
- Telemetry bindings (`PUT /v1/catalog/services/{id}/bindings`) and coverage policies
  (`/v1/catalog/coverage/policies`, tenant_admin; approval by a session user, withdrawn on change).
- Coverage assessment against `store-telemetry` (ClickHouse) and registered assets:
  `GET /v1/catalog/services/{id}/coverage`, `GET /v1/catalog/coverage`,
  `POST /v1/catalog/coverage/assess`. Status is `covered`, `partial`, `uncovered` or `unknown`;
  every demonstrated item carries an evidence reference; claims (`PUT .../coverage/claims/{item}`)
  never count as evidence.
- Asset registry (`govern-observability-assets`): `/v1/catalog/assets` (list, `ownerless=true`,
  register and retire) and producer routes `/internal/catalog/assets/{kind}/{external_id}`
  (internal token; service by id or bound telemetry identity).
- Gaps (`/v1/catalog/gaps`): opened for unmet items of critical services and for critical
  journeys without full coverage; remediate (owner, due date), accept (session user, rationale,
  future review date) and close (only when remediated with evidence or under an approved,
  unexpired acceptance).
- Coverage exceptions on critical services: `no_approved_coverage_policy`, `coverage_partial`,
  `coverage_uncovered`, `coverage_unknown`.
- `CLICKHOUSE_URL`, `CLICKHOUSE_USER`, `CLICKHOUSE_PASSWORD` configuration.

Critical journeys, scope and operating contexts (phase 1 S4).

### Added

- Migration `051_create_catalog_journeys.sql`: `catalog.journeys` (criticality, in-scope flag with
  rationale, owner, classification) and ordered `catalog.journey_services`.
- `/v1/catalog/journeys` (create, list, get, update), `/v1/catalog/scope` (in-scope services and
  journeys, excluded journeys) and `/v1/catalog/contexts` (production and non-production
  environments from `manage-control-plane`, business-critical services). Journeys are decided as
  `catalog.journey`; a journey never identifies or counts services the caller may not read.
- Journey exceptions in `/v1/catalog/exceptions` (`journeys`): critical journey not in scope, no
  services, retired service, missing, departed or unresolved owner.
- `context=business_critical` on `/v1/catalog/services` and `/v1/catalog/scope`; environment
  contexts are rejected on catalog records rather than guessed.
- `ADMIN_SERVICE_URL` configuration; `/v1/catalog/contexts` returns
  `503 {"error":"control_plane_unavailable"}` when environment classes cannot be read.

### Changed

- `/v1/catalog/exceptions` returns `{items, total, journeys}`.

Service records behind `manage-service-catalog` (phase 1 S3).

### Added

- Migration `048_create_catalog_service_records.sql`: `catalog.services` (business and operational
  name, domain, criticality, lifecycle, classification, support route), aliases with retirement,
  teams and members, business/operational owners (user or team) and outcomes with approval.
- Migration `049_authz_grants_catalog_team.sql`: default `catalog.team` grants in the auth policy
  store.
- `/v1/catalog` API: services (create, list, get, update with alias add/retire), owners, outcomes
  and approval, search, exceptions, teams and members. Requires a credential via
  `validate-credential`; every read and write is decided by `decide-access` (lists and search via
  `filter`); owner users are checked through `lookup-principals`.
- Hidden and missing services return the same `404 {"error":"not_found"}`; an unavailable decision
  point returns `503 {"error":"access_decision_unavailable"}`.
- `AUTH_SERVICE_URL` and `INTERNAL_SERVICE_TOKEN` configuration (compose, local stack, Helm).

## [0.1.0] - 2026-11-07

Component skeleton for the business service catalog (domain `service-model`, 2.0 line).

### Added

- `services/catalog-service` with `/health`, `/readyz` (PostgreSQL reachability) and `/metrics`
  (`catalog_service_http_*`), self-observability telemetry and HTTP probe tests.
- Migration `046_create_catalog_schema.sql`: `catalog` schema, `observable_catalog_role` scoped to
  it, and login user `observable_catalog`.
