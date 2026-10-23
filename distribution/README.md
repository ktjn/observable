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
verifier's documented, versioned fingerprint algorithm; `component-input-v2`
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

`component-input-v2` fingerprints one component image's *declared* build inputs from committed
bytes: the `component-source-v1` source digest, the reviewed recipe (path, build target, content
digest and Dockerfile frontend), every external image reference reachable from that target's stage
graph, target platforms and effective supported `build_args`. It reports the six `component`, `version`, `source_revision`,
`input_algorithm`, `input_digest` and `platforms` request-identity fields the
[release planner](#local-immutable-release-planning) requires, plus the source/recipe/image evidence,
so the fingerprint algorithm is no longer a fixture placeholder.

Version 1 omitted effective arguments; its digest is not interchangeable with v2. Rust always
records the mapped Cargo package as `SERVICE` and rejects caller overrides. Web records both
`VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE=self` and
`VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT=` by default. Either web value can be set using
repeated `--build-arg NAME=VALUE`, for example:

```text
python scripts/collect_component_inputs.py web --build-arg VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE=observer_instance --build-arg VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT=https://observer.example/otlp
```

Only these two public web values are supported. Modes are `self` or `observer_instance`;
unknown/duplicate names, missing `=`, non-string/control-character values and URL credentials
are rejected without echoing values. Arguments are persisted in evidence and embedded in public
JavaScript: never supply secrets. Empty endpoint values are retained explicitly, `=` inside a
value is preserved, and input-map order does not change the fingerprint. Omitted and explicit
defaults have identical identity. New reachable ARG declarations or changed defaults require a
reviewed resolver-policy update in addition to a reviewed recipe hash.

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

**Remaining gaps:** every collector result keeps `identity_verified: false`. The collector alone
does not check a registry; the [recipe-image verifier](#pinned-recipe-image-verification) supplies
separate registry-bound manifest evidence. The
content that apt, rustup, crates.io and npm resolve at build time stays mutable even though the
recipe and lockfiles that declare it are inside the digest. Complete external-material identity and authenticated release
history remain release gates before this shape can satisfy the planner's trusted-record interface.
The argument map covers the supported recipe arguments, not arbitrary Docker options, secret
mounts, proxy settings or automatically provided BuildKit arguments. Selected target platforms
are fingerprinted separately.

### Pinned recipe-image verification

```text
python scripts/verify_component_base_images.py web --platform linux/amd64 --platform linux/arm64
```

The verifier recomputes declared inputs from one resolved committed ref (`--ref`, default `HEAD`)
in a local clone (`--repo`). It accepts the same supported web `--build-arg NAME=VALUE` values as
the collector. It accepts no imported verification JSON, arbitrary registry URL, credential or
repository override. The caller selects the approved source revision.

Only the reviewed Docker Hub repositories `library/node`, `library/nginx`, `library/debian`,
`lukemathwalker/cargo-chef` and `docker/dockerfile` are supported. All references are validated
before any network request. Anonymous pull tokens are requested from fixed
`https://auth.docker.io/token`, for service `registry.docker.io` and one
`repository:<name>:pull` scope. No login, host credential discovery, refresh/offline token request,
push permission or registry write occurs. Tokens stay in memory and are sent only to
`https://registry-1.docker.io` for that repository's digest-addressed manifest reads; neither
tokens nor raw responses/errors are emitted or persisted. Alternate authentication challenges and
redirects are not followed. See [Docker token authentication](https://docs.docker.com/reference/api/registry/auth/).

Root pins must identify OCI or Docker schema-2 indexes. Raw root bytes must match both the
committed SHA256 pin and the unambiguous `Docker-Content-Digest` header. For each requested platform
the index must have exactly one compatible direct manifest descriptor. Baseline `linux/amd64`
(absent/empty variant or `v1`) and `linux/arm64` (absent/empty variant or `v8`) are supported;
required OS/CPU features and OS versions are not assumed. Unrelated platform/attestation entries
are not selected. The child is read by digest and its raw SHA256, digest header, byte size and
media type must agree with the index descriptor. Nested indexes are not accepted as selected
platform manifests. See [OCI platform descriptors](https://github.com/opencontainers/image-spec/blob/v1.1.1/image-index.md).

Both fixed-origin clients share bounded HTTPS GET mechanics: responses are limited to 4 MiB,
socket operations to 10 seconds and response reads to a shrinking 20-second per-request budget,
including status/headers/chunk trailers. DNS still depends on the host resolver. Invalid/duplicate
JSON or headers, ambiguous/truncated framing, encoded/oversized responses, missing manifests,
authentication errors, rate limits and transport failures all reject the entire operation with
one generic diagnostic and no evidence on stdout. No partial success, retries, proxy discovery or
redirect forwarding occurs. The GHCR inspector's existing origin policy remains unchanged.

Success returns component/source/input/platform identity fields plus image references, roles
(`base`/`frontend`), root digests and selected child digest/media/size records.
`registry_images_verified: true` means TLS registry content matched the committed digest
declarations. `identity_verified` and `publication_authorized` remain `false`, and the release
planner rejects this partial evidence. No publisher signature, config architecture, config/layer
availability, SBOM, vulnerability status, actual build usage, reproducibility, complete material
identity or release history is proven. Platform association is the verified index's declaration,
not an independent architecture measurement. The Dockerfile frontend runs on BuildKit's executor,
which may differ from the output target: checking its requested target-platform manifests proves
availability only, not which frontend image a builder actually selected.

Live read-only verification on 2026-10-22 passed for `web` and `query` at committed revision
`fdfa7f7340444ba5b11dfa352afa926b2fe3630d`, requesting both platforms. Each checked three indexes
(two bases and the frontend) and six child manifests, covering all five distinct repositories.
Loopback HTTP fixtures cover failure and credential-isolation cases; the unchanged GHCR protocol
suite covers the shared framing/timeout machinery. No backend dependency behavior changed, so
Testcontainers does not apply to this tooling slice. Full local CI and actual v2 image builds
remain separate checks; this command needs no Docker engine.

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
Only a derived local snapshot tag is built, using `docker buildx build --load` and
tar stdin. There is no push, registry destination, arbitrary Docker option, or
prebuilt CI binary path. Existing developer/Compose image builds are unchanged.
The two web `--build-arg NAME=VALUE` overrides above are supported here as well. All effective
values, including the empty default endpoint, are passed explicitly to Docker, so no bare
`--build-arg NAME` can inherit a host environment value. Rust SERVICE selection cannot be overridden.
The local tag is `observable-<component>:snapshot-<hash>`, with SHA256 over canonical
`source_revision`, `platform` and `input_digest`; different argument identities get different
tags. The returned immutable image ID remains authoritative.

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

The fresh output directory contains `context.tar`, `context.json`, `source.json`, `inputs.json`,
`request.json`, BuildKit's `metadata.json`, and, only after success, `result.json`.
The execution API rejects changed requests, altered tar bytes, and stale metadata.
It hashes the same in-memory tar bytes passed to Docker and recomputes declared input evidence
from that tar's unique regular recipe entry. Reviewed recipe guards and supported argument
declarations are enforced again; request arguments, input digest and saved evidence must agree.
The independent parsed request is used throughout execution so caller mutation during a long
build cannot change the recorded outcome. Old prepared attempts lacking v2 evidence must use a
fresh output directory. It then checks the loaded image's revision/version/source/context and
input-algorithm/digest labels and requested platform. Classic
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

The v2 argument-binding path additionally passed a real linux/amd64 snapshot build at source
`37cbccca72f41299e44127fd854ecdfbe4c17a60`, with mode `observer_instance` and the public endpoint
`https://collector.example.test/otlp`. The effective overrides changed the input digest from defaults;
explicit Docker arguments, inspected image labels and immutable digest binding agreed. The emitted
JavaScript assets contained that endpoint. Image
`sha256:37b2d8f4e491650835e382f9e0bdac4f6141a68cc817c922b9da441c95466994` binds input digest
`sha256:ef2be45f568d6fdec9c9d39b25d8d9bc9ae5ae9ed9ca190e3d34444011895073`.
This closes the earlier Docker-unavailable argument-execution gap; the older image checks above
remain distinct historical evidence. It proves public argument consumption, not complete materials,
trusted provenance or release authorization.

## Verified npm materials and offline installation

The web dependency preparer reads `package.json`, `package-lock.json` and
`apps/frontend/package.json` from one approved Git revision, without using dirty checkout files:

```bash
python scripts/prepare_component_npm.py fetch --ref APPROVED_COMMIT --output /fresh/npm-bundle
python scripts/prepare_component_npm.py install --ref APPROVED_COMMIT --bundle /fresh/npm-bundle --output /fresh/npm-install
```

Both output directories must be new. Fetch accepts lockfile-v3 SHA512 archives only from HTTPS
`registry.npmjs.org`, rejects redirects and credentials, verifies downloaded bytes and safe archive
layout, and writes a content-addressed inventory. Four downloads run at most; rejection cancels
queued work. Limits are 64 MiB per compressed archive, 512 MiB per expanded archive and 1 GiB of
compressed material per bundle. Traversal, duplicate paths, links and special files are rejected.

Install rederives the expected inventory from the caller's approved Git metadata and rechecks every
archive before invoking npm. It stages copies, creates an isolated cache and empty user/global
configuration, restricts the child environment, and uses the installed Node/npm CLI directly.
[`npm cache add`](https://docs.npmjs.com/cli/v11/commands/npm-cache/) consumes local archives;
[`npm ci`](https://docs.npmjs.com/cli/v11/commands/npm-ci/) runs with `--offline --ignore-scripts`,
audit/funding disabled and npm log files disabled. The normal checkout and host cache are untouched.
Success requires unchanged metadata bytes and produces `result.json` only after installation succeeds.

Local acceptance on Windows/amd64 with Node 24.22.1 and npm 11.19.1 verified 421 unique archives
covering 434 external lock records at source `d1af6f400c78a94b8e167a90d8a9ad09ebfc9c6a`, then
installed from those materials offline. Optional package installation follows the native platform;
the bundle includes all locked platform archives. This proves npm's offline install path, not OS
network isolation, lifecycle execution, compilation or an offline OCI build. Cargo, apt, rustup,
builder inputs, authenticated history and publication race protection remain separate gates.
`identity_verified` and `publication_authorized` remain false. Failed attempts can leave partial
output; use a fresh destination for a retry. No dependency or recipe change is made by this tooling.

## Native web compilation from verified materials

Compile an approved commit using the bundle above and a fresh destination:

```bash
python scripts/build_component_web.py --ref APPROVED_COMMIT --bundle /fresh/npm-bundle --output /fresh/web-build
```

`--build-arg NAME=VALUE` accepts the same two reviewed public self-observability arguments as
[declared build inputs](#declared-build-input-identity). This helper performs a fresh isolated offline
npm installation, stages raw committed frontend source, then invokes the nearest locked TypeScript
and Vite entrypoints directly with Node. TypeScript can be workspace-local while Vite is hoisted.
It requires the reviewed `tsc && vite build` package script, uses explicit production mode and
`NODE_ENV=production`, and does not execute npm lifecycle scripts or a shell. Inherited Node/Vite,
proxy and npm options are excluded from the compiler environment.

Committed `node_modules`, `dist`, `.npmrc` and production `.env*` files are rejected before npm runs;
`.env.playground` and `.env.example` are omitted, consistent with the image's production env-file
exclusion. Only committed frontend files are staged, so ignored/untracked playground Wasm and dirty
checkout changes cannot enter the build. Selected source and npm metadata must remain byte-identical
after compilation. Nonempty `index.html` and emitted JavaScript are required; artifact links and
special files are rejected, with at most 100,000 files and 512 MiB of total static output.

`result.json` records `native-web-build-v1`, source/material digests, explicit arguments, host platform,
and a sorted relative-path/size/SHA256 inventory of `dependencies/workspace/apps/frontend/dist`.
Its `artifact_digest` hashes that canonical inventory. The nested npm install success record is
separate: only the top-level result proves both compiler steps and artifact checks passed.
Failed attempts retain diagnostic workspace state but emit no build success record or raw compiler
output; retry in a fresh directory.

Local acceptance compiled source `3b92b0aa549715738c68114a183d0feeb0294a1c` on Windows/amd64 from the
421-archive verified bundle, producing 11 static files. This establishes actual TypeScript/Vite
consumption and usable native compiler dependencies. It does not establish network isolation,
trusted Node/compiler provenance, reproducibility, Docker image equivalence or runtime/browser
acceptance. `network_isolation_verified`, `identity_verified` and `publication_authorized` remain
false. Network-isolated OCI compilation is a separate gate, exercised for web amd64 by the
[offline image builder](#offline-web-oci-image-builds); complete image identity remains open.

## Offline web OCI image builds

With Docker Desktop/BuildKit available and a verified bundle, build a local image from an approved
commit in a fresh output directory:

```bash
python scripts/build_component_web_image.py --ref APPROVED_COMMIT --bundle /fresh/npm-bundle --output /fresh/web-image --platform linux/amd64
```

The only supported targets are `linux/amd64` and `linux/arm64`. The same two reviewed public
`--build-arg NAME=VALUE` options are accepted. Source and material selection reuse the guards above.
Every archive is revalidated before Docker runs. A deterministic tar context contains only selected
committed source, metadata, verified archives and the reviewed offline runner. Its `.dockerignore`
is deliberately empty: the context is already explicitly selected and contains no checkout files.

`offline-web-image-v1` generates a distinct Dockerfile using the original approved Node, nginx and
Dockerfile-frontend digest pins. The generated recipe and exact context have separate SHA256 digests;
`declared_input_digest` associates the original recipe's input declaration but does not claim that
original recipe was executed. The helper sends the verified context through stdin and supplies
`--network none --no-cache --load`. Every generated `RUN` also specifies `--network=none`.
There are no mounted npm caches, credentials, secrets, SSH forwarding, arbitrary Docker flags,
release tags or pushes. BuildKit may resolve/pull the pinned base/frontend images using its network;
network denial applies to `RUN` execution, not the registry transport or builder control plane.

Inside the Node image, a private cache is populated from verified local archives in bounded batches.
Empty user/global npm config, a restricted environment and `--offline --include=dev --ignore-scripts`
prevent online fallback, lifecycle execution and accidental omission of compiler dev dependencies.
The nearest locked TypeScript and Vite entrypoints run directly with Node, explicit production mode
and reviewed public environment values. Source hashes are checked before and after compilation;
nonempty index/JavaScript output, regular artifact files, a 512 MiB bound and 100,000-file limit are
enforced before nginx receives the static files. Runtime nginx configuration stays committed source.

`request.json` records source/material/platform/arguments and generated recipe/context digests.
`result.json` is written only after Docker metadata and immutable image inspection agree on digest,
platform and evidence labels. Runtime checks should use its `image_id`, never a mutable local tag.
This local mode does not set a tag. A failed attempt may retain context/partial builder output;
retry with a fresh destination. Raw build diagnostics are not emitted by the helper.

Local acceptance on Docker Engine 29.9.2 / Buildx 0.38.1 built `linux/amd64` and emulated
`linux/arm64` from source `6ccfb925a07284243be19810ca5f595cc2813a92` and the 421-archive bundle:

- amd64 image ID/digest: `sha256:6e8cf6006b637047e116024272f31222decb08a5a5ce01163d9a8389440169bc`.
- Emulated arm64 image ID/digest: `sha256:bec6aa81ced4107848996b3888b530fcde764e0f2b049203e3bd67fd4cd1791a`.
- Exact generated context: `sha256:63c7c08ced43171abf048bf98f3d9b1c773cf729290f3370291401ed5f0827d5`.
- Both images served index, both referenced JS/CSS assets and `/traces` fallback with `--network none` and
  no published ports. Temporary host mappings for auth-service, alert-evaluator, admin-service and
  query-api allowed committed nginx upstream names to resolve; upstream APIs were not tested.
- An uncached build with an empty private npm cache failed, rather than downloading dependencies.
  A separate uncached `RUN --network=none` had no external network interface and could not connect
  to a numeric-IP HTTP endpoint.

`run_network_disabled` records the local BuildKit invocation evidence. `identity_verified` and
`publication_authorized` remain false: this does not authenticate the builder, base-image publishers,
complete toolchain provenance, release history or reproducibility. Rust external materials,
native arm64 hardware acceptance, publication race protection, signed component releases, browser/API and
full-system acceptance remain open. The normal release workflow and source Dockerfiles are unchanged.

## Verified Cargo materials and frozen resolution

Prepare the registry archives declared by an approved Git snapshot, then reconstruct a fresh local
directory source and workspace:

```bash
python scripts/prepare_component_cargo.py fetch --ref APPROVED_COMMIT --output /fresh/cargo-bundle
python scripts/prepare_component_cargo.py stage --ref APPROVED_COMMIT --bundle /fresh/cargo-bundle --output /fresh/cargo-stage
```

`cargo-materials-v1` accepts Cargo.lock v4, crates.io registry entries with SHA256 checksums, and local
packages whose names/versions match the committed workspace manifests. It checks direct, inherited,
target-specific, dev/build and unused workspace dependency declarations. Git/alternate registries,
source overrides, unresolved/escaping local paths, local build scripts and injected Cargo/toolchain
configuration require review and are rejected. Metadata and source come from immutable Git blobs;
dirty or untracked files cannot enter the stage. All 17 workspace members are staged because Cargo
resolution covers the workspace, even when compiling one deployable package.

Fetch constructs fixed HTTPS `static.crates.io` archive URLs, using no credentials, proxy discovery
or redirects. Four downloads run at most; queued work is cancelled on failure. SHA256 bytes and
safe tar layout must agree before each archive is retained. Limits are 64 MiB compressed and 512 MiB
expanded per archive, 1 GiB compressed per bundle and 1 GiB total reconstructed vendor file bytes.
Traversal, duplicate entries, links, special files, foreign package roots, reserved checksum files
and nonzero trailing archive content are rejected. Bundle manifests bind metadata and package
archive identities; the recorded origin revision is informational, not authenticated release history.

Stage rederives expectations from the caller's Git metadata and rechecks the exact archive bytes
before writing any vendor source. It creates per-file SHA256 `.cargo-checksum.json` records, a
private committed workspace and a `.cargo/config.toml` replacing crates.io with `../vendor` and
setting offline mode. It never accepts caller-supplied vendor checksum assertions. Unchanged Cargo
metadata can reuse the bundle across source commits; the staged workspace always comes from the
caller's approved revision. Fresh destinations are required; failures can leave partial output.

The generated layout uses Cargo's documented [directory-source replacement](https://doc.rust-lang.org/cargo/reference/source-replacement.html#directory-sources).
Those checksum files protect against accidental changes; archive checksums rederived from Git are
the material verification boundary. Cargo's [`--frozen`](https://doc.rust-lang.org/cargo/commands/cargo-vendor.html#manifest-options)
combines locked resolution and offline mode. The tool does not invoke networked `cargo vendor` or
read the host's registry/cache/configuration. Staging produces `result.json` with material/workspace
digests, package count and `directory_source_verified`; it does not itself run Cargo or compile code.

Local acceptance at source `a54735ce9d48990729f962bb8d83b180d5184886` downloaded and verified all
526 registry archives. The reconstructed directory source resolved 543 packages across 17 members
using `cargo metadata --frozen --format-version 1` in the existing digest-pinned Rust 1.97.0 builder,
with `--network none`, read-only workspace/vendor mounts, an empty private Cargo home and a cleared
environment. Hiding required serde vendor material caused exit 101 with no matching package, without
modifying host materials or permitting network fallback.

The same network-disabled container then passed `cargo build --frozen --release --package auth-service
--jobs 2` and an `ldd` check with no unresolved libraries. The 18,359,960-byte linux/amd64 binary has
SHA256 `366cdaba3e92cd7ede2b2f03b53aac0a2ff6949a2790350bcd8cd4224caa45cd`. Compilation used the existing
pinned Rust 1.97.0 image's compiler, headers and native libraries; it installed no apt packages or
rustup components. This proves the auth binary path only, not all seven service builds or a deployed
auth runtime. In particular, cmake is absent from that base and remains relevant to other components.

A matching acceptance command uses only read-only staged sources and a fresh output directory:

```bash
mkdir /fresh/cargo-acceptance
docker run --rm --network none --pull never --platform linux/amd64 \
  --mount type=bind,src=/fresh/cargo-stage/workspace,dst=/app,readonly \
  --mount type=bind,src=/fresh/cargo-stage/vendor,dst=/vendor,readonly \
  --mount type=bind,src=/fresh/cargo-acceptance,dst=/output \
  --tmpfs /tmp:exec --workdir /app --entrypoint /usr/bin/env \
  lukemathwalker/cargo-chef:0.1.77-rust-1.97.0-bookworm@sha256:974d873357408b6ececac8729285204664e23f5aab21a34e353ae5bf9bb899a2 \
  -i PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
  RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/tmp/cargo-home \
  CARGO_TARGET_DIR=/tmp/target HOME=/tmp/cargo-home /bin/sh -ec '
    mkdir -p /tmp/cargo-home
    cargo metadata --frozen --format-version 1 > /output/metadata.json
    cargo build --frozen --release --package auth-service --jobs 2
    cp /tmp/target/release/auth-service /output/auth-service
    ldd /output/auth-service > /output/linkage.txt
    ! grep -qi "not found" /output/linkage.txt
  '
```

Use read-only workspace/vendor mounts and a fresh private target directory for later Cargo execution;
never use an arbitrary modified vendor tree as release evidence. Other deployable compilation paths,
verified toolchain/base provenance, apt/cmake/rustup inputs, runtime image/cluster acceptance,
authenticated release history and publication race protection remain separate gates.
`identity_verified` and `publication_authorized` remain false. Normal dependencies, lockfiles,
Dockerfiles and release workflows are unchanged.


## Locked APT materials and offline installation

Resolve the existing Rust builder/runtime package families in the approved digest-pinned base,
using the requested platform and fresh native APT lists/cache:

```bash
python scripts/prepare_component_apt.py resolve runtime --platform linux/amd64 --ref APPROVED_COMMIT --output /fresh/apt-resolution
```

Repeat for `builder` and `linux/arm64`. Resolution produces `proposal.json` and a verified `bundle/`.
Review the four proposals and adopt them as the `sets` in
[`apt-materials.lock.json`](apt-materials.lock.json), with algorithm `apt-material-lock-v1`, in Git.
Resolve is an online preparation step; its proposal alone is not an approved release input.
The current lock contains 18 runtime archives and 16 builder archives per architecture, resolved
from source `0a63612d`. No production Dockerfile, language dependency lock or image pin is changed.

`apt-materials-v1` selects only the existing chef/runtime image references from the committed auth
recipe. Its fixed Debian Bookworm/main, updates and security sources use the pinned base's Debian
archive keyring. HTTP bootstrap supports the runtime base before CA installation; native APT
signature and archive-hash enforcement remain enabled. The resolver ignores inherited APT source
configuration, uses fresh package indexes and download-only/reinstall resolution, and checks that
base dpkg state did not change. It matches the complete printed archive set, downloaded SHA256/size,
control identity and fresh index metadata before recording package name/version/architecture/origin,
index digests, keyring digest and base-status digest. The native signature chain is described in
[Debian apt-secure](https://manpages.debian.org/bookworm/apt/apt-secure.8.en.html).
It authenticates repository metadata, not a separate publisher signature for every package.

After the lock is committed, fetch or stage against that approved Git revision:

```bash
python scripts/prepare_component_apt.py fetch runtime --platform linux/amd64 --ref APPROVED_COMMIT --output /fresh/apt-bundle
python scripts/prepare_component_apt.py stage runtime --platform linux/amd64 --ref APPROVED_COMMIT --bundle /fresh/apt-bundle --output /fresh/apt-stage
```

Fetch accepts only fixed HTTPS `deb.debian.org` pool URLs, without redirects, credentials or host
proxy discovery. Four downloads run at most; archives are limited to 64 MiB each and 1 GiB total.
Stage rederives the exact lock/base/recipe from Git, ignores dirty lock edits and rechecks all bytes
before writing fresh content-addressed archives, checksums and `install.sh`. The resolver's bundle
can be reused after approval when it exactly matches the committed lock. Failures may leave partial
output; all operations require fresh destinations.

Execute `sh /materials/install.sh` in the exact recorded base/platform with the stage mounted
read-only at `/materials`, a cleared environment and Docker `--network none`. The installer checks
architecture, exact initial dpkg status and archive checksums before native APT installation. It uses
an empty private lists/cache directory populated only with locked bytes under canonical package names,
no configured package sources, `--no-download`, and explicit
locked archive arguments; extra archives are never selected. Native dpkg metadata, dependencies and
maintainer scripts remain authoritative. Network isolation is the container caller's responsibility;
staging itself does not execute installation. Do not reuse a modified stage as release evidence.

Local acceptance installed all four closures in fresh pinned-base containers with `--network none`,
read-only stages and cleared environments. Every locked package version matched native dpkg state;
curl and its dynamic linkage, the CA bundle, and builder cc/cmake/OpenSSL/libcurl pkg-config checks
passed on amd64 and emulated arm64. An extra invalid unlisted archive was ignored on all four paths.
A missing locked runtime archive failed checksum verification before APT ran. Initial acceptance
exposed APT's canonical-cache requirement; the installer now populates that private cache from the
verified content-addressed archives while retaining `--no-download`. Native arm64 hardware remains
unverified.

This is native package-material preparation, not a completed service OCI build. Auth runtime behavior,
remaining Rust compilation paths, toolchain provenance, rustup materials, authenticated release
history and publication race protection remain open. Repository/index fingerprints and informational
`resolved_from` do not establish reproducibility or release authority. `identity_verified` and
`publication_authorized` stay false. Normal release workflows are unchanged.


## Offline Rust OCI image builds

Build a component from committed source and previously verified Cargo/APT bundles:

```bash
python scripts/build_component_rust_image.py auth --ref APPROVED_COMMIT \
  --cargo-bundle /fresh/cargo-bundle --builder-bundle /fresh/builder-apt-bundle \
  --runtime-bundle /fresh/runtime-apt-bundle --platform linux/amd64 --output /fresh/auth-image
```

Use the [Cargo preparer](#verified-cargo-materials-and-frozen-resolution) and
[APT preparer](#locked-apt-materials-and-offline-installation) first. Both APT selections must be
present in the caller's approved Git lock for the requested platform. The helper accepts the seven
existing Rust component names; execution evidence is recorded separately for each component/platform.
It creates a local image only and never pushes, tags releases or changes production recipes/workflows.

`offline-rust-image-v1` reuses committed source/declared-input inspection and material staging, then
constructs immutable Docker stdin bytes directly from Git blobs and freshly reverified original
archives. Mutations to intermediate staging trees cannot enter that context. Vendor executable bits
are preserved and Cargo checksum files are independently generated. All committed workspace members
are included for native Cargo resolution. The tar has fixed ownership/timestamps and an empty
`.dockerignore`; host configuration, credentials, caches and untracked/dirty files are excluded.

The generated recipe is distinct from the original Dockerfile and its declared-input fingerprint.
It uses the approved digest-pinned frontend, chef and Debian bases. Both native
APT installs and frozen release compilation run with BuildKit networking disabled and restricted
environments. Cargo has a private empty home/target and directory-source replacement; APT receives
only the locked archives. The build uses `--no-cache`, `--network none`, and `--load`, without shared
cache mounts, secrets or caller recipe arguments. Base/frontend transport can still contact the
registry; RUN network denial does not authenticate those image publishers or prove layer availability.

The final runtime preserves `SERVICE`, the original shell exec entrypoint, `/proto/otlp`, and
`65532:65532`; it installs the native runtime closure instead of copying arbitrary libraries from the
builder. Compile-time linkage is checked inside the builder. Runtime linkage and process readiness
require separate acceptance against the finished image. Auth connects to PostgreSQL before opening
its listener, so file/linkage inspection alone does not establish `/health` or `/readyz` behavior.

`request.json`, `context.tar` and successful `result.json` bind the source/workspace, declared inputs,
Cargo material digest, both APT lock digests, generated recipe/context, platform and immutable image.
The helper checks Docker metadata against inspected image/config digests, labels, platform, user,
SERVICE and entrypoint. Classic config-digest and descriptor image-store forms are supported.
Failures produce no success record and return a static diagnostic; partial output may remain.
Native acceptance at source `d89cad5cf7bc19e6ce8c1ddc77d3d193822428eb` built auth on linux/amd64:
image `sha256:de8b304a88fdf3428969f1c34045d3c8336a7c285a18dba4a4bee95ffa930750`, context
`sha256:e466b91bf1cab7dfcc1b67e4b7c30ec5dae3170c0013bc4f1fffc0e3642e22e3`.
The finished image matched all 18 locked runtime package versions and passed auth/curl dynamic
linkage, CA readability and exact committed proto-byte checks as UID/GID `65532:65532` with runtime
networking disabled. Its original entrypoint then returned HTTP 200 for `/health` and `/readyz`
against a disposable PostgreSQL instance on an internal Docker network with no published ports.
That database used the existing cached Compose `postgres:18` image, resolved locally to
`sha256:06cad38a5d9f5d24b4d83d86def30795d5e4b757fedbf5281172b576dedcd941`; this is recorded test
infrastructure, not a new immutable production PostgreSQL pin. Development defaults avoided OIDC
credentials. The schema-free database intentionally exercised connection/readiness only; it does
not verify migrations, credential validation, OIDC or full cluster behavior. Owned containers and
network were removed after acceptance.

For process native Kafka/CMake build and broker recovery evidence, see the
[process acceptance record](../docs/component-process-offline-verification.md).

These are local construction checks, not signed builder provenance or complete release identity.
`identity_verified` and `publication_authorized` remain false. Reproducibility, native arm64 hardware,
other Rust build/runtime paths, full cluster acceptance, history and publication protection remain
separate gates. Full local CI remains required before any future code push.
