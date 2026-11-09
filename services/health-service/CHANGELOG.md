# Changelog

## [Unreleased]

### Added

- Phase 1 C2.2 adds the worker's scoped ports: `CatalogInputs` reads catalog inputs with the
  tenant's own worker credential and discards answers for another tenant, `Evidence` reads the
  latest evidence per signal for an exact tenant, `service.name` and nonempty environment
  through a `readonly = 2` ClickHouse session, and `scope` selects a service's identities in one
  environment class. Evaluation wiring follows in C2.3.

## [0.1.0] - 2026-11-08

### Added

- Phase 1 C2.1 adds the health-service runtime with liveness, schema readiness and HTTP metrics.
- Migration 058 provisions the dedicated health schema and login. The health assessment API lands with the authorized provider in C2.3.
