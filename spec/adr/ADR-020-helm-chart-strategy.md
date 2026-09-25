# ADR-020: Helm Chart Strategy (Library + Umbrella, kind for Testing)

**Date:** 2026-04-19
**Status:** Accepted
**Authors:** Claude Code
**Deciders:** Project Stakeholders
**Review date:** 2026-04-19

> **Amended by ADR-035 (2026-09-25):** Helm, the umbrella distribution chart, the shared library
> chart, and kind testing remain the deployment strategy. References below to a monorepo-local
> chart, a single `observable-services` image, and repo-root migration bundles describe the current
> implementation during decomposition. The target distribution consumes independently versioned
> component images and component-owned migration bundles.

## Context

ADR-010 established Kubernetes as the target deployment platform and noted that "Helm or Kustomize
will be used for configuration and deployment management." Phase 2 slice P2-S8a requires a concrete
decision so that:

1. Service manifests can be rendered, reviewed, and version-controlled.
2. A rollback path exists and is documented.
3. Local Kubernetes testing is possible without a shared cluster.
4. Maximum configuration is shared with the existing Docker Compose local-dev stack.

The platform currently has six Rust services plus four infrastructure components (ClickHouse,
PostgreSQL, Redpanda, OpenFGA). All six services are built from a single Dockerfile with
per-service command entrypoints, share the same environment-variable naming conventions, and expose
a common `/health` endpoint. This uniformity makes a shared chart template viable.

## Decision

### Tooling

Use **Helm v3** (not Kustomize). Helm provides native rollback (`helm rollback`), templated
configuration, hook lifecycle (pre-install Jobs for migrations), and a large ecosystem.
Kustomize is deferred; it may be used as a post-renderer if environment-specific patches are
needed in a future phase.

Use **kind (Kubernetes in Docker)** for local Kubernetes testing. kind is lightweight, reproducible,
does not require cloud credentials, and is well-supported in GitHub Actions.

### Chart Layout

The current implementation keeps two charts under `charts/` in the monorepo:

```
charts/
  observable-common/    # Helm library chart (type: library)
  observable/           # Helm application chart (type: application)
```

Under ADR-035 these charts move to `observable-distribution` once component artifacts are
independent. The chart structure remains the same; only source ownership and image inputs change.

**`observable-common` (library chart):** Defines reusable named templates for the three
resources all six services need: `Deployment`, `Service`, and the common label/selector set.
A library chart produces no manifests on its own; it is only a provider of shared Go template
blocks. Consuming charts declare it as a dependency.

**`observable` (application chart):** The deployable chart for the full platform. It declares
`observable-common` as a local file dependency, defines one template file per service (six total)
that calls the shared library templates, and adds a migration `Job` hook.

### Sharing With Docker Compose

The Helm chart and Docker Compose share:
- During migration, the current `observable-services` image. The ADR-035 target replaces this
  with one independently versioned image per deployable component, pinned by the distribution.
- The same environment variable names (`DATABASE_URL`, `CLICKHOUSE_URL`, `REDPANDA_BROKERS`, etc.).
- The same infrastructure images (`clickhouse/clickhouse-server:24.3`, `postgres:16`,
  `redpandadata/redpanda:v23.3.1`, `openfga/openfga:v1.5`).
- The same `/health` endpoint contract for liveness and readiness probes.

The Helm `values.yaml` keys (`clickhouse.url`, `postgres.url`, `redpanda.brokers`) mirror the
Compose environment variable sections so an operator can cross-reference the two definitions
without consulting separate documentation.

### Infrastructure in kind Tests

Infrastructure services are deployed in the kind cluster using a dedicated **infrastructure Helm
chart** (`charts/observable-infra`). This chart prioritizes official, community-maintained
alternatives over third-party repackagers like Bitnami (which moved to a restricted model in 2025):
- `redpanda-data/redpanda` (Official)
- `openfga/openfga` (Official)
- `CloudNativePG` (CNCF Project) for PostgreSQL
- Official `clickhouse/clickhouse-server` images for ClickHouse

Using official community resources ensures that our integration environment remains sustainable
and aligned with production best practices. The `kind-test.sh` script automates the installation
of the `CloudNativePG` operator before deploying the infrastructure chart.

A custom `redpanda-setup` Job is included in the `observable-infra` chart to automate topic
creation on startup.

The kind integration chart runs OpenFGA with its in-memory datastore. Authorization persistence
is not exercised by the current application smoke path, and using in-memory OpenFGA avoids a
Helm hook race where the OpenFGA migration job can run before the CloudNativePG cluster has
finished bootstrapping. PostgreSQL remains deployed through CloudNativePG for application
database migrations and runtime connectivity.

### Database Migrations in Kubernetes

Migrations run as a Kubernetes `Job` with Helm `pre-install,pre-upgrade` hook annotations.
The Job uses two init containers — one for PostgreSQL (`postgres:16`) and one for ClickHouse
(`clickhouse/clickhouse-server:24.3`) — that mount migration SQL from two ConfigMaps:
`observable-migrations-postgres` and `observable-migrations-clickhouse`.

These ConfigMaps are currently created by the deployment pipeline **before** `helm install` using
`kubectl create configmap --from-file=migrations/<db>/`. During ADR-035 decomposition, migration
ownership moves with the owning component: ClickHouse migrations are released by
`observable-store-clickhouse`, while PostgreSQL migrations are released by their owning control,
auth, or alerting component. `observable-distribution` materializes the pinned migration bundles
before invoking Helm.

### Rollback Path

`helm rollback <release> <revision>` redeploys the previous Deployment specs. The rollback
path is documented in `spec/12-deployment.md §19.7`.

Schema migrations are forward-only (ADR-013). Rolling back the application to a previous image
version is safe as long as:
1. The previous image version can run against the current schema (backward-compatible migration
   policy — any migration that breaks a previous service version is a release blocker).
2. The migration Job for the reverted release is **not** re-run after rollback (Helm rollback
   does not re-execute hook Jobs; migrations already applied stay applied).

### Kubernetes Test Strategy

See `spec/11-testing.md §18.7` for the full Kubernetes test strategy. In summary:

- **Helm lint** runs on every PR via `.github/workflows/pr.yml`.
- **kind integration test** runs on every push to `main` and nightly via
  `.github/workflows/kind-test.yml`. It creates a fresh kind cluster, deploys infra, runs
  migrations, installs the chart, verifies all service health endpoints respond, exercises the
  ingest-to-query smoke path, validates `helm rollback`, and tears down.
- **Release candidate** environments use the same chart against a shared integration cluster.

## Consequences

**Easier:**
- Single rollback command (`helm rollback`) for application tier.
- Library chart eliminates per-service copy-paste for Deployment/Service scaffolding.
- kind allows any developer or CI runner to exercise Kubernetes behaviour without cloud access.
- Compose and Helm share component image/version pins and env var names; configuration drift is
  visible immediately.

**Harder:**
- Helm dependency management (`helm dependency update`) must run before `helm install`/`lint`.
- Migration ConfigMaps must be created by the pipeline before chart install; forgetting this step
  is a deployment failure mode.
- kind clusters are ephemeral and do not test persistent-volume behaviour; storage HA tests
  require a different environment.

**Constrained:**
- Kustomize is not used; any environment-specific patches go through Helm values overrides.
- Operators for stateful services are not part of the kind test; they are introduced in a
  later phase when production HA is required.
- The chart targets Helm v3.x; Helm v2 (Tiller) is not supported.

## Alternatives Considered

### Option A: Kustomize Only
Rejected. Kustomize lacks native rollback semantics, hook lifecycle, and release tracking.
Rollback would require manual `kubectl apply` of a previous commit, which is error-prone and
harder to document.

### Option B: Raw kubectl Manifests
Rejected. Manifests would require manual environment-specific substitution (sed/envsubst), no
release history, and no atomic rollback.

### Option C: Embed Migration SQL in the Chart
Rejected for this phase. Embedding SQL in `charts/observable/files/migrations/` creates a
duplicate that diverges from the authoritative `migrations/` directory. The ConfigMap injection
approach keeps ADR-013 as the single source.

### Option D: Minikube Instead of kind
Rejected. kind is faster to start, more reproducible in CI, and requires no VM hypervisor.
Minikube adds a hypervisor dependency that is not available in all CI runners.

## Related

- `ADR-010`: Deployment Model (k8s-first) — now resolved to Helm + kind
- `ADR-013`: Schema Governance (SQL migrations) — authoritative migration source
- `ADR-019`: CI Scripts Runnable Locally
- `spec/12-deployment.md §19.7`: Rollback documentation
- `spec/11-testing.md §18.7`: Kubernetes test strategy
- `charts/observable-common/`: Library chart
- `charts/observable-infra/`: Infrastructure chart
- `charts/observable/`: Application chart
- `scripts/kind-test.sh`: Local kind cluster test script
- `scripts/helm-lint.sh`: Helm lint script (runnable locally, used in CI)
