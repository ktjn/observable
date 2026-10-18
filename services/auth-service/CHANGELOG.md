# Changelog

All notable changes to the `observable-auth` component are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-07-18

Initial component baseline extracted from the `0.1.0` platform release.

### Added

- OIDC browser login, session cookies, and logout.
- API-key validation for `ingest-gateway` and `query-api` via `/internal/validate`.
- Tenant, membership, role, and API-key lifecycle APIs.
- Credential audit log for authentication decisions.
