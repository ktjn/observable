# Versioning and Release Policy

## Version scheme

Observable uses [Semantic Versioning](https://semver.org/) (MAJOR.MINOR.PATCH).

## Pre-1.0 stability

While the version is below 1.0.0:

- **Storage schemas** (ClickHouse tables, PostgreSQL tables) may change between minor versions.
  Migrations are provided but rollback is not guaranteed.
- **HTTP APIs** (`/v1/*` endpoints) may add fields, change response shapes, or remove
  endpoints between minor versions.
- **Helm values** may be renamed, restructured, or removed between minor versions.
- **Configuration environment variables** may be renamed or removed.
- **Upgrade procedures** may require manual steps documented in release notes.

Breaking changes are noted in release notes. There is no deprecation period before 1.0.

## After 1.0

Starting with 1.0.0, Observable will follow standard semver:

- **PATCH** releases contain bug fixes and security patches only.
- **MINOR** releases add functionality in a backward-compatible manner.
- **MAJOR** releases may contain breaking changes, with a documented migration path.

## Supported versions

Only the latest release receives security fixes. There is no long-term-support (LTS) branch.
See [SECURITY.md](SECURITY.md) for the vulnerability reporting process.

## Release artifacts

Each release includes:

- Container images (tagged `v{VERSION}`)
- Helm chart (versioned to match the application version)
- Changelog
