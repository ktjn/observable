# Changelog

All notable changes to the `observable-control` component are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- `DELETE /v1/tenants/{id}` purges each component through its owner instead of deleting `auth.*` and `alerting.*` rows, which the scoped `observable_control` login cannot reach, and now covers catalog and health data. It records the request (migration `065_create_control_tenant_purges.sql`), deletes ClickHouse telemetry and the `control` schema, then calls auth last so the caller's session survives an earlier failure. Every step is idempotent and a failure is 503 instead of a silently skipped ClickHouse table. The response adds `pending_components`.

### Added

- `GET /internal/tenant-purges?component=` and `POST /internal/tenant-purges/{tenant_id}/acks` (internal token): the purge request feed for alerting, catalog and health, which depend on control and so cannot be called by it. The catalog is served a request only after alerting acknowledged it; the request completes when all three acknowledged.

### Removed

- `/v1/tokens*` and `/v1/admin/members*`: served by auth-service, which owns `api_keys` and `user_tenant_roles`. The scoped `observable_control` login could not read those tables.

### Fixed

- `GET /v1/tenants` (with a session) and `GET /v1/tenants/{id}/environments` read memberships and key environments from auth-service (`AuthDirectory`) instead of `user_tenant_roles` and `api_keys`, which the scoped `observable_control` login cannot read. An auth outage returns 503.
- Environment classes (`/v1/tenants/{id}/environment-classes`, `/internal/environment-classes`) failed with 500 under the scoped `observable_control` login because they read `api_keys`, which the control role cannot access. Key environments now come from auth-service `/internal/key-environments` (`AUTH_SERVICE_URL`, `INTERNAL_SERVICE_TOKEN`); when auth cannot answer the listing returns 503 instead of hiding unclassified environments. This also unblocks catalog contexts and health inputs.

### Added

- `GET /internal/observability-assets?tenant_id=` (internal token; catalog consumer, phase 1 S6):
  dashboards whose service panels name one service, with the panels' single environment filter
  and the earliest owner grant.

- Environment classes (phase 1 S4, OBS-01-03): migration `050_create_control_environment_classes.sql`
  (`control.environment_classes`), `GET /v1/tenants/{id}/environment-classes` (any tenant role),
  `PUT`/`DELETE /v1/tenants/{id}/environment-classes/{environment}` (tenant_admin; `production` or
  `non_production`, invalid input 422) and `GET /internal/environment-classes?tenant_id=` for the
  catalog. Environments seen on API keys but never classified are listed as `unclassified`.

## [0.5.0] - 2026-11-01

### Added

- Multi-tenant retention and ingest quota management endpoints (`GET /v1/tenants/:id/config`, `PUT /v1/tenants/:id/config`).
- Comprehensive GDPR and multi-tenant hard-purge endpoint (`DELETE /v1/tenants/:id`) clearing all PostgreSQL control-plane entities and issuing ClickHouse telemetry deletion mutations.
- Immutable audit log tracking for tenant purge events in `credential_audit_log` (`action = 'tenant_purge'`).
- Immutable audit logging for API token lifecycle operations (`token_create`, `token_revoke`, `token_renew`, `token_restore`, `token_delete`).
- Actionable usage, cost, and cardinality diagnostics exposed in `GET /v1/tenants/usage-report`, including active metric series, top cardinality metrics breakdown, tenant quota posture, and token operations.
- Integration tests in `tests/postgres_tenant_purge_integration.rs`, `tests/usage_integration.rs`, and expanded `tests/auth_tenant_isolation_integration.rs`.

## [0.1.0] - 2026-07-18

Initial component baseline extracted from the `0.1.0` platform release.

### Added

- Tenant, project, and environment APIs.
- Dashboard, dashboard-panel, saved-view, and grant CRUD.
- Schema registry (`schema_entries`) and semantic annotation APIs.
- Deployment markers and change events, including `deployment.markers.v1` publication.
- Platform configuration (including LLM provider settings).
