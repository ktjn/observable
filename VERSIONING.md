# Versioning and Release Policy

## Version source of truth

The repository root [`VERSION`](VERSION) file is the authoritative Observable product version.
Release-facing metadata must match it, including:

- Rust workspace package versions under `libs/` and `services/`.
- `apps/frontend/package.json`.
- The Observable Helm chart `version` and `appVersion`.
- Git release tags, which use the form `v{VERSION}`.

Run `bash scripts/verify-version.sh` before opening a release pull request. CI runs the same check
for pull requests, `main`, and release tags.

To bump the product version, update `VERSION` and every location reported by the verifier in one
pull request. Do not derive the product version independently from branch names, commit counts, or
mutable image tags.

## Tag-bound artifact builds

The `Release Artifacts` workflow runs only for tags matching `v*`. Before building, it verifies that:

- The tag is exactly `v{VERSION}`.
- All release-facing metadata matches `VERSION`.
- The tagged commit is contained in `origin/main`.

The workflow produces a `linux/amd64` OCI image archive containing all Observable Rust services, a
packaged Observable Helm chart, and a `SHA256SUMS` manifest. These are retained as GitHub Actions
artifacts for 30 days.

The same workflow publishes the runtime image to:

```text
ghcr.io/ktjn/observable-services:v{VERSION}
```

The published tag is a multi-architecture manifest for `linux/amd64` and `linux/arm64`. No mutable
`latest` tag is published. SBOM generation, provenance attestations, and signing remain separate
release steps so their policy can be reviewed independently.

## Version scheme

Observable uses [Semantic Versioning](https://semver.org/) (MAJOR.MINOR.PATCH).

## Pre-1.0 stability

While the version is below 1.0.0:

- **Storage schemas** (ClickHouse tables, PostgreSQL tables) may change between minor versions.
  Migrations are provided but rollback is not guaranteed.
- **HTTP APIs** (`/v1/*` endpoints) may add fields, change response shapes, or remove
  endpoints between minor versions.
- **Helm values** may be renamed, restructured, or removed.
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
