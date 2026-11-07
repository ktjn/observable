# Changelog

All notable changes to the `observable-catalog` component are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-07

Component skeleton for the business service catalog (domain `service-model`, 2.0 line).

### Added

- `services/catalog-service` with `/health`, `/readyz` (PostgreSQL reachability) and `/metrics`
  (`catalog_service_http_*`), self-observability telemetry and HTTP probe tests.
- Migration `046_create_catalog_schema.sql`: `catalog` schema, `observable_catalog_role` scoped to
  it, and login user `observable_catalog`.
