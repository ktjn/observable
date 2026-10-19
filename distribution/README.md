# Distribution

This directory holds the Observable distribution artifacts that pin a tested,
compatible set of component versions. It is the monorepo-local precursor to the
`observable-distribution` component described in
[docs/component-decomposition.md](../docs/component-decomposition.md) and
[ADR-035](../spec/adr/ADR-035-component-independence.md); it moves to its own
repository once the components build and release independently.

## `manifest.yaml`

The distribution manifest pins one version per target component plus the
product version. `observable-distribution` consumes it to install a pinned
component set rather than tracking a single source commit.

It is the authoritative source of per-component versions:

- `productVersion` must equal the root `VERSION`.
- Each component version must equal its own package version: the Cargo
  `package.version` of its service crate, or `apps/frontend/package.json` for
  `web`. `contracts` has no single owning package yet and is manifest-only.
- Component versions may diverge from `productVersion`.

To release a component independently, bump its manifest entry, its package
version, and its Helm pin (`charts/observable/values.yaml`
`services.<name>.image.tag`) in the same pull request. Editing only some of them
fails validation.

```bash
bash scripts/check-distribution-manifest.sh   # also run by scripts/verify-version.sh
bash scripts/component-version.sh query       # prints one component's version
```

## Per-component images

Each deployable component has its own container image, built from the
`component-runtime` target of the root [Dockerfile](../Dockerfile) (the frontend
from `apps/frontend/Dockerfile`). The component-builder stage builds only the
requested Cargo package, so a change to one component does not rebuild another
component's binary.

```bash
# All components
bash scripts/build-component-images.sh

# A subset, with a registry prefix and tag
REGISTRY=ghcr.io/ktjn TAG=0.1.0 bash scripts/build-component-images.sh query alerting

# Tag each image with its own manifest component version
REGISTRY=ghcr.io/ktjn TAG=manifest bash scripts/build-component-images.sh
```

Every image carries the `org.opencontainers.image.version` label set to its
manifest component version, whatever `TAG` is. `TAG` defaults to `local`.

Component name → image → Cargo package:

| Component | Image | Source |
| --- | --- | --- |
| `auth` | `observable-auth` | `services/auth-service` |
| `control` | `observable-control` | `services/admin-service` |
| `ingest` | `observable-ingest` | `services/ingest-gateway` |
| `process` | `observable-process` | `services/stream-processor` |
| `store-clickhouse` | `observable-store-clickhouse` | `services/storage-writer` |
| `query` | `observable-query` | `services/query-api` |
| `alerting` | `observable-alerting` | `services/alert-evaluator` |
| `web` | `observable-web` | `apps/frontend` |

`docker-compose.yml` runs each Rust service from its own
`observable-<component>:local` image. The smoke, perf, and soak runners use the
`verification-tools` Dockerfile target (`observable-verification:local`).

CI builds only the components a change affects. Run
`python3 scripts/ci_changed_components.py BASE HEAD` to see the set for a diff.

## Pinned install

`compose.pinned.yaml` is a Compose override that pins every deployable component
image to its manifest version, so the distribution installs the exact tested set
rather than the `local` developer tags. It is generated from `manifest.yaml`:

```bash
bash scripts/render-pinned-compose.sh          # regenerate the override
bash scripts/render-pinned-compose.sh --check  # fail if out of date (run by scripts/local-ci.sh)
```

Install the pinned set and run the end-to-end smoke test against it:

```bash
bash scripts/pinned-install.sh
```

That builds every component image tagged with its manifest version
(`TAG=manifest`), brings up `docker-compose.yml` plus `compose.pinned.yaml`
(including the pinned `observable-web` UI, waiting for its healthcheck), and runs
`smoke-test` against the pinned set. Set `COMPOSE_PROJECT_NAME` to isolate the run.

Helm (`charts/observable`) pins each component as
`services.<name>.image: { repository, tag }`; the tags must equal this
manifest's component versions. `kind-test.sh` and `dev-reload.sh` build and load
the per-component images. Only the release workflow still publishes the shared
`observable-services` image, until per-component release publishing lands.

## Local immutable-release planning

`python scripts/plan_component_release.py INPUT.json` evaluates the
[immutable release policy](../VERSIONING.md#immutable-component-releases) without
building images, contacting the registry, or changing tags. It prints one JSON
intent (`publish`, `reuse`, or `complete-evidence`); rejected input exits 1 with
no plan on stdout. Every intent has `publication_authorized: false`.

Input has `request`, `observations`, and optional `history` keys. This example
describes a first publication after two definitive missing-tag observations:

```json
{
  "request": {
    "component": "query",
    "version": "0.1.0",
    "source_revision": "1111111111111111111111111111111111111111",
    "input_algorithm": "fixture-v1",
    "input_digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    "platforms": ["linux/amd64"]
  },
  "observations": [
    {"status": "missing", "component": "query", "version": "0.1.0"},
    {"status": "missing", "component": "query", "version": "0.1.0"}
  ]
}
```

This is illustrative data, not release evidence. Component names match the image
builder (`store-clickhouse`, not the manifest's `storeClickhouse`); `contracts`
has no runtime image. Versions must be valid SemVer usable as OCI tags: build
metadata containing `+` is rejected. Revisions are full Git object IDs; digests
are lowercase SHA256. Platforms are a nonempty set of Linux platform strings;
ordering does not affect identity.

A found observation is `{"status": "found", "record": RECORD}`. `RECORD` has
exactly these keys:

| Key | Meaning |
| --- | --- |
| `identity` | Same fields as `request`, from the original publication |
| `digest` | Original image's OCI manifest digest (`sha256:` plus 64 hex digits) |
| `identity_verified` | Boolean `true`, asserted by a trusted verifier |
| `sbom_subject` | Verified attestation subject digest, or `null` if missing |
| `provenance_subject` | Verified attestation subject digest, or `null` if missing |

`history`, when supplied, is the same record format for this exact
component/version from authoritative release history. History may have incomplete
attestations, but its identity must be verified. A missing historical image or a
changed historical digest/identity rejects the plan. A found image with matching
verified inputs reuses its original digest and source revision, even when the
request's source revision differs. Missing attestations produce `complete-evidence`
for that digest; attestations naming a different subject reject the plan.

Only `found` and definitive `missing` observations are accepted. All errors,
unknown statuses, unverified identity, duplicate JSON keys, and unexpected fields
reject the plan. A missing observation must target the requested component/version.
The API `plan_release(request, inspect, history=None)` calls `inspect()` again
after an initial miss when there is no historical publication. A newly found
matching tag becomes reuse/evidence completion; a conflicting tag or inspection
error rejects the intent. The CLI requires both
observations for that path and rejects unused observations.

**Trust boundary and remaining gates:** the planner checks the consistency of
assertions, not their authenticity. A hash and `identity_verified: true` in a JSON
file do not verify build inputs or attestations. `input_algorithm` must name the
verifier's documented, versioned fingerprint algorithm (for example, `fixture-v1`
above is only for tests); the planner treats its name as opaque and rejects a
mismatch with published identity. Before enabling publishing, implement a trusted
build-input collector/verifier, registry/history adapter, live reinspection,
serialization and overwrite prevention at publication, and digest-bound release
evidence. Offline observations cannot establish freshness or prevent a race after
the final lookup. Existing build-selection logic is not a fingerprint algorithm.

### Committed source evidence

`python scripts/collect_component_source.py query --ref HEAD` inventories the
selected component's committed source. `--repo PATH` selects another local clone.
The collector reads Git trees and blobs at the resolved full commit ID, without
reading checkout source bytes or extracting files. Dirty files and host line-ending
conversion cannot change the snapshot. Git/manifest/layout failures exit 1 with no
inventory on stdout and no raw upstream diagnostics.

The `component-source-v1` algorithm computes each file's SHA256 from its raw Git
blob bytes, then hashes canonical JSON with `algorithm`, `component`, and `files`
keys. Each file record contains `path`, Git `mode`, and `digest`; records are sorted
by path. Canonical JSON uses sorted object keys, ASCII escapes, and compact comma/
colon separators. The full commit ID is output as `source_revision` outside this
hash, so an unrelated documentation commit preserves source identity.

Rust selection includes the service's normal/build dependency closure (including
target-specific dependencies), all workspace member manifests, root `Cargo.toml`
and `Cargo.lock`, all `proto/` files, root Dockerfile and `.dockerignore`, and tracked
build-selection/workflow/collector scripts. All member manifests matter because
Cargo resolution and the current cargo-chef stage read workspace metadata. Unrelated
service and dev-only dependency source do not enter the selected closure. Files
within selected crates are conservatively included even when Docker would ignore
them. Web selection includes root package/lock files, root `.dockerignore`, all
tracked frontend files, and those build scripts; the separate playground build's
Rust sources are not inputs to the current web OCI recipe.

The collector rejects missing required files, unknown crate layouts, duplicate
packages, unresolved local production/build dependencies, dependency overrides,
selected symlinks/submodules, build scripts requiring additional selection review,
and expanded npm workspaces. It pins the two reviewed Dockerfile content hashes:
a changed recipe must be reviewed alongside source selection before updating the
guard. Changes to the algorithm's selection or canonicalization require an algorithm
version bump. Tests run through the existing script regression suite.

Existing `services/.junie/` agent notes are classified as non-build metadata and
excluded from the unknown-crate check; an otherwise unowned source directory fails.

This output is **source-only evidence**, marked `identity_verified: false`. Its
`source_digest` is not the planner's complete `input_digest`, and it cannot authorize
image reuse or publication. The current recipes still resolve mutable base images,
apt packages, toolchains, and downloaded dependencies; platform/build arguments,
external material resolution, checkout/build binding, and attestation verification
remain required before a trusted full build identity can be constructed.
