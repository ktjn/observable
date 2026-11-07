# Changelog

All notable changes to the `observable-catalog` component are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
