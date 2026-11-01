# Changelog

All notable changes to the `observable-control` component are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.0] - 2026-11-01

### Added

- Multi-tenant retention and ingest quota management endpoints (`GET /v1/tenants/:id/config`, `PUT /v1/tenants/:id/config`).
- Comprehensive GDPR and multi-tenant hard-purge endpoint (`DELETE /v1/tenants/:id`) clearing all PostgreSQL control-plane entities and issuing ClickHouse telemetry deletion mutations.
- Immutable audit log tracking for tenant purge events in `credential_audit_log` (`action = 'tenant_purge'`).
- Integration tests in `tests/postgres_tenant_purge_integration.rs`.

## [0.1.0] - 2026-07-18

Initial component baseline extracted from the `0.1.0` platform release.

### Added

- Tenant, project, and environment APIs.
- Dashboard, dashboard-panel, saved-view, and grant CRUD.
- Schema registry (`schema_entries`) and semantic annotation APIs.
- Deployment markers and change events, including `deployment.markers.v1` publication.
- Platform configuration (including LLM provider settings).
