# Versioning and Release Policy

## Version source of truth

The repository root [`VERSION`](VERSION) file is the authoritative Observable product version.
Release-facing metadata must match it, including:

- Rust workspace package versions under `libs/`.
- `productVersion` in [`distribution/manifest.yaml`](distribution/manifest.yaml).
- The Observable Helm chart `version` and `appVersion`.
- Git release tags, which use the form `v{VERSION}`.

Deployable component packages (`services/*` crates and `apps/frontend/package.json`) carry their
own component version from `distribution/manifest.yaml` instead; see
[Component versions](#component-versions).

Run `bash scripts/verify-version.sh` before opening a release pull request. CI runs the same check
for pull requests, `main`, and release tags.

To bump the product version, update `VERSION` and every location reported by the verifier in one
pull request. Do not derive the product version independently from branch names, commit counts, or
mutable image tags.

### Component-decomposition transition

[ADR-035](spec/adr/ADR-035-component-independence.md) changes the target release model, but not the
current release mechanics immediately.

During roadmap `0.2`:

- the root `VERSION` remains the authoritative product version
- the shared `observable-services` image remains a transitional artifact
- component versions must not diverge merely by editing workspace package versions

### Component versions

[`distribution/manifest.yaml`](distribution/manifest.yaml) is the authoritative source of
per-component versions. `scripts/check-distribution-manifest.sh` (also run by
`scripts/verify-version.sh`) enforces:

- `productVersion` equals `VERSION`.
- Each component's package version equals its manifest entry: the `services/*` crate for Rust
  components, `apps/frontend/package.json` for `web`. `contracts` is manifest-only until it has a
  single owning package.
- Each component's Helm image pin (`charts/observable/values.yaml` `services.<name>.image`) names
  its `observable-<component>` image and equals its manifest entry.
- Each component has a per-component `CHANGELOG.md` (Keep a Changelog format) with an entry for its
  manifest version; `scripts/check-component-changelogs.sh` enforces this.
- Component versions may diverge from `productVersion`.

To release a component, bump its manifest entry, package version, and Helm pin together, and add a
changelog entry, following SemVer for that component's own contract. A partial edit fails validation.
`scripts/build-component-images.sh` labels each image with its component version
(`org.opencontainers.image.version`) and, with `TAG=manifest`, tags it with that version.

At the independent-artifact milestone, each deployable component owns its own SemVer lifecycle and
OCI image. `observable-distribution` then owns the product version and a manifest of the exact
component versions verified together. Product releases no longer require rebuilding unchanged
components.

The migration and exit criteria are defined in
[docs/component-decomposition.md](docs/component-decomposition.md).

### Immutable component releases

This is the policy for Phase 7 per-component publishing; it is not yet implemented by the release
workflow. It does not change the transitional shared-image release path below.

- Publish deployable components as `ghcr.io/ktjn/observable-<component>:<version>`, using the
  manifest version without a `v` prefix and the existing image names (including
  `observable-store-clickhouse`). `contracts` has no runtime image.
- A published component version identifies one immutable OCI manifest digest. Never overwrite or
  delete and recreate its tag, including on retries. A product release that keeps that component
  version reuses the existing digest without rebuilding or pushing it. Rollback reuses the earlier
  digest; it does not rebuild an earlier source revision.
- Compare component inputs with the source recorded for that component's published version, not
  merely with the previous product tag. A change to release inputs requires a new component version
  before publishing: service source, transitive production/build dependencies, lockfiles, generated
  artifacts, migrations/assets included in the image, Docker build context and recipe inputs, and
  target platforms/base-image digests and build arguments. Include playground/WASM dependencies only
  when the artifact actually consumes them; the separate Pages build is not the web OCI build.
  Product-only metadata outside those inputs does not force unchanged components to release.
- Record enough release identity to verify reuse: component name/version, source revision, build
  input identity, target platforms, and resulting OCI digest, with SBOM and provenance bound to that
  digest. Build input identity must use a documented, versioned algorithm; the current changed-path
  build selector alone is not proof of release identity. If identity cannot be verified or inputs
  differ under an existing version, fail and require investigation or a version bump.
- Inspect the registry before building. An existing tag is reusable only after identity and required
  release evidence are verified. Only a definitive missing-tag response permits a first publication;
  authentication, authorization, network, and registry errors fail closed. A missing image claimed
  by an earlier published release is a recovery failure, not permission to recreate its tag.
- Serialize writers for each component/version and prevent tag replacement at the publication
  boundary. A preflight lookup alone is insufficient: if a concurrent publication appears, verify
  and reuse its identity/digest or fail without overwriting it. Publishing must remain disabled
  until the implementation can enforce this rule.
- Complete and verify each component's release evidence before promoting a distribution manifest.
  After a partial failure, retain already published artifacts and retry by verifying/reusing them;
  never roll back publication by deleting or replacing a tag. Missing attestations may be retried
  against the original digest without rebuilding the image.

The next slice must implement and test this publish/reuse/reject decision locally, including
unchanged components across product releases, changed inputs without a bump, registry errors,
partial retries, and concurrent writers. Registry publishing and published-artifact installation
evidence remain separate acceptance gates; this policy does not establish either.

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

The packaged Helm chart is published to:

```text
oci://ghcr.io/ktjn/charts/observable
```

tagged `{VERSION}` to match the chart's `version` in `charts/observable/Chart.yaml`. Install
directly from the registry with:

```text
helm install observable oci://ghcr.io/ktjn/charts/observable --version {VERSION}
```

The published container image carries an SPDX SBOM and SLSA build-provenance attestation,
generated and attached during this workflow via GitHub's native attestation actions. Verify
either with:

```text
gh attestation verify oci://ghcr.io/ktjn/observable-services:v{VERSION} --owner ktjn
```

The attested SBOM is generated by scanning the `linux/amd64` build only, even though the
attestation's subject digest is the multi-architecture manifest; packages unique to the
`linux/arm64` build are not reflected in the SBOM.

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
