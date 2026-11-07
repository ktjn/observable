# Changelog

All notable changes to the `observable-auth` component are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `decide-access` policy decision point (ADR-041): `POST /internal/authz/decide` and `POST /internal/authz/filter` over a PostgreSQL policy store in the `auth` schema (migration `047_create_authz_policy_store.sql`: role grants by classification, purpose rules, time-limited elevations, `access_decision_log`). Fails closed; every deny is `403 {"decision":"deny"}` regardless of reason.
- `lookup-principals`: `POST /internal/principals/resolve` returns active, departed or unknown per user for one tenant.
- Shared decision semantics, HTTP clients, programmable mocks and real/mock contract suites in `libs/observable-auth` (`authz`, `principals`, `contract`).

### Changed

- auth-service requires `INTERNAL_SERVICE_TOKEN` outside dev mode; the new `/internal/authz/*` and `/internal/principals/*` routes reject callers without it.

## [0.5.0] - 2026-11-01

### Added

- Audit logging for cross-tenant context switches and unauthorized tenant access attempts in `validate_session_handler` (`action = 'tenant_select'`).
- Audit logging for expired and revoked session validation attempts (`denial_reason = 'session_revoked_or_expired'`).

## [0.1.0] - 2026-07-18

Initial component baseline extracted from the `0.1.0` platform release.

### Added

- OIDC browser login, session cookies, and logout.
- API-key validation for `ingest-gateway` and `query-api` via `/internal/validate`.
- Tenant, membership, role, and API-key lifecycle APIs.
- Credential audit log for authentication decisions.
