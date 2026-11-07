# Changelog

All notable changes to the `observable-catalog` component are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
