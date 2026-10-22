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
verifier's documented, versioned fingerprint algorithm; `component-input-v1`
([declared build-input identity](#declared-build-input-identity)) is the current
declared-input algorithm, while `fixture-v1` above is only for tests. The planner
treats the name as opaque and rejects a mismatch with published identity. Before
enabling publishing, implement a trusted verifier for those declared inputs (the
collector resolves and fingerprints them but authenticates nothing), a
registry/history adapter, live reinspection, serialization and overwrite prevention
at publication, and digest-bound release evidence. Offline observations cannot
establish freshness or prevent a race after the final lookup. Existing
build-selection logic is not a fingerprint algorithm.

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
image reuse or publication. The recipes digest-pin their base and frontend images,
but the declared inputs are still resolved by the separate
[declared build-input identity](#declared-build-input-identity) step, and the
materials apt, rustup, crates.io and npm resolve at build time stay mutable;
registry authentication, build-argument values, checkout/build binding, and
attestation verification remain required before a trusted full build identity can
be constructed.

### Declared build-input identity

```text
python scripts/collect_component_inputs.py query [--ref HEAD] [--repo PATH] [--platform linux/arm64]
```

`component-input-v1` fingerprints one component image's *declared* build inputs from committed
bytes: the `component-source-v1` source digest, the reviewed recipe (path, build target, content
digest and Dockerfile frontend), every external image reference reachable from that target's stage
graph, and the target platforms. It reports the six `component`, `version`, `source_revision`,
`input_algorithm`, `input_digest` and `platforms` request-identity fields the
[release planner](#local-immutable-release-planning) requires, plus the source/recipe/image evidence,
so the fingerprint algorithm is no longer a fixture placeholder.

Both OCI recipes pin every `FROM` image and the `# syntax=` frontend by `@sha256:` index digest
(Docker Hub multi-platform index digests). A mutable tag, an unsupported stage flag, an instruction
before the first stage, an unknown build target, an unresolved `COPY --from`, an absent or misplaced
frontend digest, or an unreviewed recipe change (the reviewed `RECIPES` hashes in
`collect_component_source.py` are enforced first) all fail closed. Only the stages reachable from the
build target contribute `images` and `unresolved_materials` entries, so an unrelated stage
(`verification-tools`, `runtime`) cannot add inputs; the whole-file recipe digest still covers those
stages, so editing one is caught by the reviewed-hash gate. Platforms are restricted to
`linux/amd64` and `linux/arm64`, the platforms the local snapshot builder can produce.

Output is deterministic and lists `images` (reference, digest, and the reaching stages) and
`unresolved_materials` (apt packages, rustup components, and build-time download URLs the reachable
stages install or fetch), alongside the source/recipe evidence and `input_algorithm`/`input_digest`.

**Remaining gaps:** every result keeps `identity_verified: false`. The recorded digests are not
authenticated against a registry, no externally supplied build-argument value is covered, and the
content that apt, rustup, crates.io and npm resolve at build time stays mutable even though the
recipe and lockfiles that declare it are inside the digest. Registry authentication of the pinned
digests, build-argument binding, complete external-material identity, and authenticated release
history remain release gates before this shape can satisfy the planner's trusted-record interface.

### Read-only registry inspection

`python scripts/inspect_component_registry.py query 0.1.0` requests the exact
component tag from `https://ghcr.io/v2/ktjn/observable-query/manifests/0.1.0`.
Only the eight known deployable repositories and OCI-compatible SemVer tags are
accepted. The implementation follows the
[OCI manifest pull protocol](https://github.com/opencontainers/distribution-spec/blob/main/spec.md#pulling-manifests).

It uses GET only, accepts OCI manifests/indexes and Docker schema-2 manifests/lists,
and verifies SHA256 over the raw response bytes against a mandatory, unambiguous
`Docker-Content-Digest`. It rejects malformed/duplicate-key JSON, invalid manifest
structure or descriptors, mismatched media types, ambiguous headers/framing,
compressed bodies, and responses larger than 4 MiB. Socket operations have a
10-second timeout; response reads use a shrinking 20-second request budget,
including status/header and chunk-trailer parsing. DNS resolution remains subject
to the host resolver's behavior. No retries or redirects occur.

A valid HTTP 404 JSON error containing exclusively `MANIFEST_UNKNOWN` produces
`{"status":"missing","component":"query","version":"0.1.0"}`. Other 404s,
authentication/authorization failures, rate limits, server errors, TLS/network
failures, truncation, and invalid responses exit 1 with no observation on stdout.
Diagnostics do not echo upstream bodies, credential values, or exception messages.

Found output includes `status`, `component`, `version`, `image`, `digest`,
`media_type`, and `identity_verified: false`. This verifies only the returned
manifest's integrity. It does not fetch layers, verify platforms, validate build
identity, authenticate SBOM/provenance, or establish authoritative release history.
The planner deliberately rejects this found shape: a trusted adapter must first
verify those release properties and construct a complete planner record.

For an authenticated read, supply an already-scoped registry bearer via
`--bearer-stdin` (up to 16 KiB, plus a terminal newline). Do not put credentials in
arguments. The library's `inspect(..., bearer=...)` accepts the same token in
memory. It never reads host credential stores or obtains a token automatically;
an unauthenticated challenge fails closed. Requests remain on the fixed GHCR
origin, so redirects cannot forward the bearer elsewhere.

Regression tests use an isolated loopback HTTP server for actual transport reads,
including slow headers/trailers, chunked bodies, redirects, limits, and two fresh
missing observations through the planner. No production backend or containerized
dependency implementation changes, so Testcontainers is not applicable to this
tooling slice. These fixtures do not establish live GHCR authentication or
published-artifact acceptance. Publishing remains disabled pending complete build
identity, trusted history/attestations, and race protection at publication.

### Authenticated image attestations

```text
python scripts/verify_component_attestations.py query sha256:DIGEST_HEX FULL_SOURCE_COMMIT
```

Replace `DIGEST_HEX` with 64 lowercase hexadecimal characters and `FULL_SOURCE_COMMIT`
with the approved full Git object ID (40 or 64 lowercase hexadecimal characters).
Only the eight known runtime components are accepted. The adapter constructs an
immutable `oci://ghcr.io/ktjn/observable-<component>@sha256:...` target; it never
accepts a mutable tag, arbitrary image URL, repository, workflow, host or extra flags.

Installed trusted `gh` performs two cryptographic verifications: SLSA v1 provenance
(`https://slsa.dev/provenance/v1`) and SPDX (`https://spdx.dev/Document`). Both use
fixed `github.com`, repository `ktjn/observable`, signer workflow
`ktjn/observable/.github/workflows/release-artifacts.yml`, GitHub Actions OIDC issuer,
the expected source **and signer** commit, and rejection of self-hosted runners.
The current workflow is direct; a reusable builder or extracted repository needs
a reviewed trust-policy update. The caller must select an approved expected commit:
the adapter does not establish membership in main or release-tag eligibility.

The adapter invokes `gh` directly with an argument array, closed stdin and a
30-second timeout per predicate. It captures subprocess diagnostics in memory and
never echoes or persists them. Verification JSON larger than 16 MiB is rejected
after capture; this is not a streaming memory bound. Strict duplicate-key parsing
requires nonempty results, in-toto v1 statements, the requested predicate, a nonempty
predicate object and exactly one matching component image/SHA256 subject per result.
All returned statements must match. Signature/certificate verification is delegated
to `gh`; a JSON file or a claimed verified flag is never an input to this CLI.
See [GitHub CLI verification](https://cli.github.com/manual/gh_attestation_verify).

Success returns only `component`, image repository, `digest`, `source_revision`,
`sbom_subject`, `provenance_subject`, `attestations_verified: true` and
`identity_verified: false`. No raw predicate, certificate or bundle is returned.
This proves constrained-workflow signatures about the image digest, not SBOM schema
validity/completeness, per-platform coverage, complete external build materials,
reproducibility, authenticated release history or immutable tags. The release planner
rejects this partial evidence shape; a later trusted full-input adapter must combine
and verify the remaining properties before constructing a release record.

Existing GitHub/GHCR authentication must already be configured for `gh`; the wrapper
does not log in, read/export tokens or accept credentials in arguments. Missing
attestations, authentication/network failures, timeout, missing tools, nonzero exit
or invalid output all exit 1 with no evidence on stdout and one generic diagnostic.
No partial result is emitted when only one predicate succeeds. It never signs,
uploads, publishes or changes a registry tag.

Regression tests exercise the adapter's policy and parsing through a controlled
runner. Installed `gh 2.102.0` supports the enforced flags; a live read-only check
against an unpublished query digest rejected with sanitized output. No positive
live component verification is claimed: the current release workflow still signs
only `observable-services`. No backend/dependency boundary changes, so Testcontainers
does not apply; fixtures do not substitute for future live signed-component acceptance.

### Committed snapshot builds

```text
python scripts/build_component_snapshot.py web --ref HEAD --output NEW_DIRECTORY
```

Replace `NEW_DIRECTORY` with a new directory under an existing parent. The command
rejects existing output directories and never overwrites their contents. `--repo`
selects a local clone; `--platform` selects `linux/amd64` (default) or `linux/arm64`.
Only a fixed local snapshot tag is built, using `docker buildx build --load` and
tar stdin. There is no push, registry destination, arbitrary Docker option, or
prebuilt CI binary path. Existing developer/Compose image builds are unchanged.

The context contains raw regular Git blobs at one resolved full commit ID. Dirty
and untracked files never enter it; Git archive export/line-ending transformations
are not used. Sorted paths, fixed ownership/timestamps and preserved executable
modes make the tar repeatable. Symlinks, submodules, and unsafe path names in the
copied context are rejected before Docker runs.

Web includes root package files, `.dockerignore`, and the committed frontend tree.
Rust includes Cargo root files, `.dockerignore`, Dockerfile, and **all** committed
`libs/`, `services/`, and `proto/` files: cargo-chef prepares/cooks the whole
workspace even when the final service build is scoped. This broader context digest
is recorded separately from the component's transitive source digest. Reviewed
recipe guards still apply. Component package versions must match the distribution
manifest at that same source revision.

The fresh output directory contains `context.tar`, `context.json`, `source.json`,
`request.json`, BuildKit's `metadata.json`, and, only after success, `result.json`.
The execution API rejects changed requests, altered tar bytes, and stale metadata.
It hashes the same in-memory tar bytes passed to Docker, then checks the loaded
image's revision/version/source/context labels and requested platform. Classic
image stores must match BuildKit's configuration digest to the local image ID;
containerd stores must match the local root descriptor **and** image ID to BuildKit's
image digest. The result records which binding was checked. See
[Buildx metadata](https://docs.docker.com/reference/cli/docker/buildx/build/#metadata-file).

Rust builds then run `ldd` on the mapped service binary in an ephemeral container
with networking disabled and image pulling prohibited, using the verified image ID
rather than the mutable snapshot tag. A nonzero exit or `not found` in either output
stream rejects the build, since `ldd` can report missing libraries while exiting zero.
Only then is `dynamic-linkage` added to `runtime_checks` in the success result. Web records an
empty list for this Rust-specific check. This verifies loader dependencies, not
service startup, readiness, database/broker connectivity, or full-system behavior.

Failures retain the attempt directory without a success record, and raw subprocess
diagnostics are not echoed or stored by the wrapper. Use a new directory for another
attempt. Successful images remain available under the recorded local snapshot tag;
rebuilding that local tag may change its digest. This does not publish or replace a
component release tag.

All results retain `identity_verified: false`. BuildKit metadata and local labels
bind this local build to source/context assertions; they do not authenticate a
published release, lock external build materials, prove reproducibility, verify
attestations/history, or prevent registry races. Those remain release gates.

Validation includes a real `linux/amd64` web snapshot build and local container
checks for index/referenced JavaScript HTTP 200. Upstream API names were mapped to
loopback solely to allow standalone nginx startup; backend APIs were not exercised.
The ephemeral container was stopped. All seven Rust `linux/amd64` component images
have also been built from committed source and passed isolated dynamic-library
checks. Their temporary verification containers auto-removed. A real web
`linux/arm64` build and nginx static-serving check also passed under Docker Desktop
emulation on an x86_64 host; see the [arm64 evidence](../docs/component-web-arm64-verification.md).
Rust arm64 execution, native arm64 hardware, service connectivity and cluster
integration remain unverified by these image checks.
