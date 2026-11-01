# ADR-035: Independent Component Boundaries and Build-Time Isolation

**Date:** 2026-09-25
**Status:** Accepted (Amended 2026-11-01)
**Authors:** OpenAI, ktjn
**Deciders:** Project Stakeholders
**Review date:** 2026-11-01

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

Observable establishes independently buildable, logically decoupled components with strict
build-time and runtime isolation, governed by the machine-checkable System Specification
([spec/system.yaml](../system.yaml)).

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

### 2026-11-01 Amendment: Build-Time Component Isolation & System-Spec Alignment

Following the implementation of machine-checkable specifications ([spec/system.yaml](../system.yaml)
and [spec/22-system-specification.md](../22-system-specification.md)), the project refines its release
and repository architecture:

1. **Build-Time Component Isolation Replaces Repository Extraction**:
   Components are defined as logical, bounded subsystems with strict build-time and state isolation.
   Physical repository extraction (Phase 8) is superseded; components reside within the unified
   monorepo with machine-verified boundaries, unambiguous source ownership (`source_paths`), and
   isolated build targets (`SERVICE=<component>`).
2. **Unified Distribution Release Replaces Per-Component Release Publishing**:
   Publishing separate per-component OCI images to external registries on independent release cadences
   (Phase 7 sub-step 5) is superseded. The full platform is verified, packaged, and released as a
   cohesive distribution (`Observable`), while preserving per-component build caching, scoped CI
   compilation, and local container verification.
3. **Specification-Governed Architecture**:
   Component boundaries, capability dependencies, data ownership, and build-time isolation are
   governed and validated continuously against `spec/system.yaml` via the system specification
   metamodel.
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
another component's owned PostgreSQL schema. Cross-component authorization and metadata reads
(such as cross-tenant session revalidation against `auth.user_tenant_roles` or query metadata lookups
against `control.schema_entries`) route over authenticated internal service APIs (`auth-service`'s
`POST /internal/validate-session`, `admin-service`'s `/internal/schemas/*`).

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

Deployable components are built using service-scoped Docker targets (`component-runtime` with
`SERVICE=<component>`) and scoped CI compilation, ensuring changes to one component do not trigger
unnecessary compilation of others.

As established in the 2026-11-01 Amendment, the project releases as a unified, tested distribution
(`Observable`) rather than publishing separate per-component images with independent release cadences
to external registries. Per-component versioning in `distribution/manifest.yaml` and Helm values
governs component evolution while the overall release is verified and published as a cohesive unit.

### Repository rule

The monorepo is the permanent development and release environment. Build-time component isolation
and architectural boundaries are enforced within the repository by tooling, Cargo workspace structure,
and the machine-checkable specification (`spec/system.yaml`).

Physical multi-repository extraction is superseded. Full-platform Compose, Helm, upgrade, and E2E
verification remain orchestrated under `distribution/` and `charts/observable/`.

### Control-plane ownership

Deployment markers, generic change events, platform configuration, and schema registry entries
and annotations are control-plane metadata and belong to `observable-control`.

Cross-owner SQL against `control` tables is prohibited; other components (`observable-query`)
access control-plane metadata via internal APIs (`/internal/deployment-*`, `/internal/schemas/*`,
`/internal/config/llm`).

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
