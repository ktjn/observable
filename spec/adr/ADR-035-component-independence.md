# ADR-035: Independent Component and Repository Boundaries

**Date:** 2026-09-25
**Status:** Accepted
**Authors:** OpenAI, ktjn
**Deciders:** Project Stakeholders
**Review date:** 2026-09-25

## Context

Observable already runs as multiple processes, but several boundaries are still coupled at source,
artifact, persistence, and release level:

- all Rust services share one Cargo workspace and are shipped in one `observable-services` image
- the repository has one product version that forces lockstep release metadata
- `query-api`, `admin-service`, `ingest-gateway`, `auth-service`, and `alert-evaluator`
  share PostgreSQL tables and credentials
- `stream-processor` forwards processed telemetry to `storage-writer` over synchronous HTTP
  rather than using the durable stream as the component boundary
- shared Rust crates contain both wire contracts and implementation concerns
- the full Helm/Compose distribution and service implementation are released from the same source
  repository

Moving the existing directories directly into separate repositories would preserve these runtime
and schema dependencies and create a distributed monolith.

The immediate roadmap goal is therefore to establish real independent component boundaries before
the existing operational-maturity roadmap continues.

## Decision

Observable will move to independently buildable, versioned, deployable components with a separate
distribution repository.

The target component set is:

- `observable-contracts`
- `observable-auth`
- `observable-control`
- `observable-ingest`
- `observable-process`
- `observable-store-clickhouse`
- `observable-query`
- `observable-alerting`
- `observable-web`
- `observable-distribution`

Detailed ownership and migration sequencing are defined in
[docs/component-decomposition.md](../../docs/component-decomposition.md).

### Contract rule

Components share versioned contracts, not application implementation.

Cross-component HTTP APIs use OpenAPI. Durable telemetry and domain events use explicit versioned
event schemas. A shared implementation crate must not become the implicit compatibility contract
between independently released components.

### Persistence rule

Each PostgreSQL table/schema has exactly one component owner. Components do not execute SQL against
another component's owned PostgreSQL schema.

ClickHouse is intentionally different because the query path requires storage-engine-level
performance: the storage component owns writes and migrations; the query component may read the
schema directly within an explicit tested schema-version compatibility range.

### Telemetry pipeline rule

ADR-009's durable queue boundary is applied between both ingest/processing and processing/storage:

```text
ingest -> telemetry.raw.v1 -> process -> telemetry.normalized.v1 -> store
```

The current processor-to-storage HTTP transport is transitional and will be removed.

### Release rule

Each deployable component receives its own container image and SemVer lifecycle. The
`observable-distribution` repository owns the product version and pins a tested set of component
versions.

The current single root `VERSION` and `observable-services` image remain valid only during the
migration.

#### Immutable component release identity

Per-component OCI version tags are immutable: an unchanged version reuses the original verified
digest across product releases without a rebuild or push. Changed release inputs require a new
component version. Release identity covers transitive build inputs as well as service source;
the previous product tag or a changed-path selector alone is not a sufficient baseline.

Publishing must fail closed on unverifiable identity or registry errors and prevent concurrent
writers from replacing a component/version tag. Partial retries reuse published digests and
finish their release evidence; rollback never deletes or replaces them. The detailed policy and
implementation acceptance cases are in
[VERSIONING.md](../../VERSIONING.md#immutable-component-releases) and
[the deployment specification](../12-deployment.md#1932-independent-component-releases).
This is a target requirement; per-component registry publishing remains unimplemented.

The monorepo recipes pin every base and Dockerfile-frontend image by digest, and the
[declared build-input identity](../../distribution/README.md#declared-build-input-identity)
fingerprints the source closure, the recipe, the reachable pinned images, target platforms and
effective supported build arguments as `component-input-v2`. Rust SERVICE selection stays fixed;
web accepts only the two reviewed recipe arguments. Snapshot execution passes exactly the recorded
values and verifies their input-digest image label. Version 1 omitted argument values and is not
interchangeable with v2. This does not authenticate image digests or pin the materials apt, rustup
and the language package managers resolve at build time,
so it supplies a declared-input fingerprint rather than verified release identity.

The read-only [recipe-image verifier](../../distribution/README.md#pinned-recipe-image-verification)
checks those approved pins against fixed Docker Hub token/registry endpoints, using anonymous
repository-scoped pull tokens held only in memory. Raw index digests and direct requested-platform
manifest digests/sizes/media types must match their committed declarations. Origins/repositories
are allowlisted; redirects, credential discovery and registry writes are prohibited. This supplies
registry-bound manifest evidence, not publisher signatures, independently measured architecture,
actual builder/frontend selection, layer availability or complete build-material identity.
It never promotes declared inputs into trusted release-planner records.

The initial monorepo attestation verifier trusts GitHub Actions OIDC certificates
for `ktjn/observable` and its direct `.github/workflows/release-artifacts.yml`
workflow, with both source and signer commits equal to the caller's approved full
commit. It rejects self-hosted runners and requires separate SLSA v1 provenance
and SPDX statements about the exact component repository and immutable image
digest. Verification uses GitHub CLI's cryptographic policy enforcement, not
caller-supplied verification JSON. This authenticates signed image evidence only;
it does not establish complete external-material identity, source main/tag
eligibility, per-platform SBOM completeness, release history or publication safety.
Future extracted repositories/reusable builders require an explicit trust-policy
revision before acceptance. See the [verifier contract](../../distribution/README.md#authenticated-image-attestations).

The [npm material preparer](../../distribution/README.md#verified-npm-materials-and-offline-installation) verifies locked web archive bytes and revalidates them against
approved Git metadata before an isolated `npm ci --offline --ignore-scripts` installation. A native
Windows/amd64 full-lock install passed for 421 archives and 434 external package records. This is
partial consumption evidence: offline OCI builds, complete external materials, authenticated history
and publication safety remain open; it grants no release identity or publication authorization.
The [native web build](../../distribution/README.md#native-web-compilation-from-verified-materials) additionally compiles committed production source
from those materials and records a static-output digest inventory. Its compiler environment is
restricted, but native execution has no network isolation or verified toolchain provenance; this
compilation evidence does not satisfy the offline OCI build or publication gates.

The [offline web image builder](../../distribution/README.md#offline-web-oci-image-builds) now executes the generated npm install and production
compile recipe with BuildKit RUN networking disabled and cache reuse disabled. Web amd64 and emulated arm64 image
binding and isolated nginx static serving passed. Its generated recipe/context identity is separate
from the original Dockerfile declaration; pinned image resolution still uses registry transport.
Trusted builder/material provenance, native arm64 hardware, release history and publication protection
remain required. The local result grants no publication authorization.

The [Cargo material preparer](../../distribution/README.md#verified-cargo-materials-and-frozen-resolution) verifies exact committed crates.io archive SHA256 values,
then reconstructs a directory source with independently generated per-file checksums. All 526
registry archives verified, and frozen resolution passed for 543 packages/17 workspace members in a
network-disabled pinned Rust container with private Cargo state. The auth-service release binary
also compiled frozen/offline and passed dynamic linkage without additional apt/rustup downloads.
This verifies registry source consumption and that binary path, not all native toolchain materials,
deployable image behavior or release authority.

The [APT material preparer](../../distribution/README.md#locked-apt-materials-and-offline-installation) records native signed
Debian resolution for the pinned Rust builder/runtime bases on amd64 and arm64: 16 builder and
18 runtime archives per platform. A committed lock governs later HTTPS fetch and byte-revalidated
staging; generated installers guard base dpkg state, select only locked archives and prohibit
downloads. All four closures installed in network-disabled containers with exact dpkg versions,
tool/CA/linkage checks and unlisted archives ignored; a missing archive failed before installation.
Arm64 used emulation. This supplies native package material evidence, not a completed service image,
reproducibility or publication authority.

The [offline Rust image builder](../../distribution/README.md#offline-rust-oci-image-builds) now combines Git-only workspace
source with reverified Cargo archives and committed builder/runtime APT closures. Its generated
recipe disables RUN networking and cache reuse, compiles a frozen release and preserves the non-root
runtime/proto/entrypoint contract. Inspected image digests, platform, labels and runtime configuration
must match the local request. Component/platform runtime acceptance is separate; full release identity
and publication authorization remain false. Auth amd64 built successfully and passed non-root
runtime linkage/CA/proto checks, exact runtime package versions and HTTP 200 `/health`/`/readyz`
against an isolated disposable PostgreSQL instance. Other Rust components/platforms, migrations,
OIDC and full cluster acceptance remain open.

Full identity and publication flags remain false.

### Repository rule

The current monorepo remains the migration workspace until component boundaries are proven.
Repository extraction is performed last, one component at a time, with history preserved.

Full-platform Compose, Helm, upgrade, rollback, and E2E verification move to
`observable-distribution`.

### Control-plane ownership

Deployment markers and generic change events are control-plane metadata and move to
`observable-control`.

This supersedes ADR-024's decision that deployment writes permanently belong in
`ingest-gateway`. The existing route remains a temporary implementation detail until the
control-plane extraction slice moves it.

### Helm strategy

ADR-020 remains valid for Helm, the umbrella distribution chart, shared chart templates, and kind
testing. Its single `observable-services` image assumption is transitional and is replaced by
per-component image/version values as independent artifacts are introduced.

### Admin-service evolution

ADR-033 correctly established the need to isolate privilege-granting administration from query.
ADR-035 further splits that temporary service by state owner:

- users, memberships/roles, API-key lifecycle, and credential audit move to `observable-auth`
- platform configuration and other product control-plane metadata move to `observable-control`
- usage data is obtained through `observable-query` rather than direct ClickHouse access

This keeps credential validation and credential lifecycle on one PostgreSQL ownership boundary.

## Consequences

**Easier:**

- services can release, roll back, scale, and evolve independently
- persistence ownership and security boundaries become explicit
- unrelated services are not rebuilt for local changes
- Redpanda provides the intended durability boundary during storage outages
- component compatibility can be tested rather than inferred from one Git commit
- the full distribution becomes a composition of released artifacts

**Harder:**

- more repositories, releases, dependency updates, and compatibility matrices
- contract evolution requires explicit backward-compatibility discipline
- local development needs a distribution manifest rather than implicit workspace source linkage
- cross-component integration bugs move from compile time to contract/integration tests
- independent database ownership requires staged migration and temporary compatibility bridges

**Constrained:**

- no new cross-component source dependency may be introduced
- no new cross-owner PostgreSQL query may be introduced
- new durable cross-component messages require versioned contracts
- repository extraction cannot precede independent build, contract, and state ownership
- full-system behavior remains a release gate in the distribution repository

## Alternatives Considered

### Option A: Keep the monorepo permanently and only improve module boundaries

Rejected as the target because it preserves lockstep artifact/release ownership and does not meet
the goal of independently versioned components. The monorepo is retained temporarily as the safest
migration workspace.

### Option B: Split repositories immediately

Rejected because current database, shared-crate, image, and runtime coupling would simply move
across repository boundaries and create a distributed monolith.

### Option C: One repository per existing service

Rejected because the existing services are not the final bounded contexts. In particular,
`query-api` contains control-plane and alerting responsibilities, while alerting functionality is
spread across multiple existing services.

### Option D: Require all storage access through an RPC service

Rejected for ClickHouse query reads because it would add an unnecessary hop to the latency-sensitive
query path. Explicit schema compatibility gives storage ownership without hiding the engine from
the dedicated query component.

## Related

- [ROADMAP.md](../../ROADMAP.md)
- [Component decomposition](../../docs/component-decomposition.md)
- [ADR-009](ADR-009-queue-stream-backbone.md)
- [ADR-020](ADR-020-helm-chart-strategy.md)
- [ADR-024](ADR-024-deployment-marker-routing.md)
- [ADR-033](ADR-033-admin-service-extraction.md)
- [spec/02-architecture.md](../02-architecture.md)
- [spec/10-process.md](../10-process.md)
- [spec/12-deployment.md](../12-deployment.md)
- [spec/18-deployment-markers.md](../18-deployment-markers.md)
