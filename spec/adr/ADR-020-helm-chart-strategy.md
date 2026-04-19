# ADR-020: Helm Chart Strategy (Library + Umbrella, kind for Testing)

**Date:** 2026-04-19
**Status:** Accepted
**Authors:** Claude Code
**Deciders:** Project Stakeholders
**Review date:** 2026-04-19

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

Two charts live under `charts/` in the monorepo:

```
charts/
  observable-common/    # Helm library chart (type: library)
  observable/           # Helm application chart (type: application)
```

**`observable-common` (library chart):** Defines reusable named templates for the three
resources all six services need: `Deployment`, `Service`, and the common label/selector set.
A library chart produces no manifests on its own; it is only a provider of shared Go template
blocks. Consuming charts declare it as a dependency.

**`observable` (application chart):** The deployable chart for the full platform. It declares
`observable-common` as a local file dependency, defines one template file per service (six total)
that calls the shared library templates, and adds a migration `Job` hook.

### Sharing With Docker Compose

The Helm chart and Docker Compose share:
- The same container image (`observable-services`, built from the repo-root `Dockerfile`).
- The same environment variable names (`DATABASE_URL`, `CLICKHOUSE_URL`, `REDPANDA_BROKERS`, etc.).
- The same infrastructure images (`clickhouse/clickhouse-server:24.3`, `postgres:16`,
  `redpandadata/redpanda:v23.3.1`, `openfga/openfga:v1.5`).
- The same `/health` endpoint contract for liveness and readiness probes.

The Helm `values.yaml` keys (`clickhouse.url`, `postgres.url`, `redpanda.brokers`) mirror the
Compose environment variable sections so an operator can cross-reference the two definitions
without consulting separate documentation.

### Infrastructure in kind Tests

Infrastructure services (ClickHouse, PostgreSQL, Redpanda, OpenFGA) are deployed in the kind
cluster using raw Kubernetes manifests stored in `deploy/kind/infra/`. These manifests use
the same Docker images and environment-variable names as `docker-compose.yml` so there is one
source of truth for image versions and configuration shape.

Operators are **not** used in the kind test environment. The ClickHouse Operator, Strimzi, and
Redpanda Operator are deferred to production-scale environments where their HA guarantees are
needed. The kind environment validates application behaviour, not stateful-service HA.

### Database Migrations in Kubernetes

Migrations run as a Kubernetes `Job` with Helm `pre-install,pre-upgrade` hook annotations.
The Job uses two init containers — one for PostgreSQL (`postgres:16`) and one for ClickHouse
(`clickhouse/clickhouse-server:24.3`) — that mount migration SQL from two ConfigMaps:
`observable-migrations-postgres` and `observable-migrations-clickhouse`.

These ConfigMaps are created by the deployment pipeline **before** `helm install` using
`kubectl create configmap --from-file=migrations/<db>/`. This keeps migration SQL authoritative
in the `migrations/` directory at the repo root (ADR-013) without duplicating files inside the
chart. The kind test script (`scripts/kind-test.sh`) and any future CI release pipeline must
create these ConfigMaps before invoking Helm.

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
- Compose and Helm share image tags and env var names; configuration drift is visible immediately.

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
- `charts/observable/`: Application chart
- `deploy/kind/infra/`: Infrastructure manifests for kind
- `scripts/kind-test.sh`: Local kind cluster test script
- `scripts/helm-lint.sh`: Helm lint script (runnable locally, used in CI)
