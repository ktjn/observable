# Deployment and Operations

## 19. Deployment Specification

### 19.1 Recommended Runtime

Kubernetes-first.

Reasons:
- operability
- autoscaling
- workload segregation
- progressive delivery
- standard service mesh/policy integration

### 19.2 Deployment Model

- control plane on k8s
- data plane on k8s + stateful services
- managed storage where acceptable
- object storage externalized
- edge POP/collector option for RUM

### 19.3 Release Strategy

Use progressive delivery. Argo Rollouts supports blue-green and canary patterns natively.

Deployment sequence:
1. build signed artifact
2. run unit/integration/contract suites
3. ephemeral env smoke
4. canary in internal tenant
5. progressive rollout
6. automated analysis gates
7. rollback on SLO regression

### 19.3.1 Build and Artifact Strategy

Builds are produced by CI and promoted by GitOps. CI owns artifact creation; deployment controllers own reconciliation into Kubernetes environments.

**Build stages**
1. resolve pinned dependencies from lockfiles
2. validate generated code, protobuf/OpenAPI contracts, and schema migrations
3. compile Rust services and frontend assets
4. run unit, contract, and changed-service integration tests
5. build container images from minimal runtime images
6. generate SBOMs and provenance attestations
7. sign images and attach checksums
8. render Helm/Kustomize manifests for target environment profiles
9. publish artifacts to registries using immutable commit-based tags

**Artifact inventory**
- service container images
- frontend static asset bundle or frontend container image
- migration bundles
- generated API/SDK packages
- rendered Kubernetes manifests
- SBOM, provenance, signature, and checksum metadata

**Promotion model**
- PR builds are disposable and never promoted to production.
- Main builds may deploy only to shared integration.
- Release-candidate builds may promote to perf/staging after integration gates pass.
- Production builds must be release-candidate artifacts that passed staging, canary, automated analysis, and rollback checks.
- Rebuilding the same commit for promotion is disallowed; promote the same signed artifact across environments.

### 19.4 Environment Topology

- local dev
- CI ephemeral
- shared integration
- perf/staging
- production
- regulated/single-tenant variant

### 19.5 Disaster Recovery

- multi-AZ mandatory
- multi-region optional at first release
- restore drills quarterly
- RPO/RTO defined per retention tier

### 19.6 Local Development

Local development uses Docker Compose for external dependencies, Rust services, and the React frontend. The frontend is served from an nginx container built from `apps/frontend/Dockerfile`.

**Quick start**

```bash
make dev                       # start Docker Compose stack, services, migrations, and frontend
```

For advanced control, individual steps are available:
- `bash scripts/migrate.sh` — explicitly run ClickHouse/Postgres setup and migrations.
- `bash scripts/start-services.sh` — explicitly start Rust services.

For full detail on the frontend local development workflow, directory layout, environment
variables, Vite proxy config, mock strategy, production build, nginx hosting, and Playwright
E2E setup see `spec/15-frontend-local-dev.md`.

**Dependency stack**

| Service    | Image                        | Ports      | Purpose                      |
|------------|------------------------------|------------|------------------------------|
| clickhouse | clickhouse/clickhouse-server | 8123, 9000 | Telemetry store              |
| redpanda   | redpandadata/redpanda        | 9092, 9644 | Durable queue / stream       |
| postgres   | postgres:16                  | 5432       | Control plane metadata store |
| openfga    | openfga/openfga              | 8080       | Fine-grained auth store      |

**Application services**

The Rust service containers are built from the repo-root `Dockerfile`. The frontend container is built from `apps/frontend/Dockerfile` and serves static Vite assets through nginx.

| Service          | Host port | Internal dependencies                    |
|------------------|-----------|------------------------------------------|
| auth-service     | 4318      | postgres                                 |
| storage-writer   | 4320      | clickhouse                               |
| stream-processor | none      | redpanda, storage-writer                 |
| ingest-gateway   | 4317      | auth-service, redpanda                   |
| query-api        | 8090      | clickhouse                               |
| frontend         | 5173      | query-api                                |

**Configuration**

- Copy `.env.local.example` (committed) to `.env.local` (gitignored) at the repo root.
- Each Rust service reads config from environment variables supplied by `docker-compose.yml`. `.env.local` supplies local defaults for dependency credentials and host-facing ports.
- No production secrets are required for local development.

**Schema migrations**

- In local mode, migrations run automatically via `docker-compose.yml` setup containers when the stack starts.
- For manual execution, run `bash scripts/migrate.sh` after starting the Compose dependency stack.
- In CI and production, migrations are explicit pipeline steps and do not run automatically on service startup.

**Rules**

- `docker compose up` must start cleanly from scratch with no manual seed steps. Setup and migrations are automated via `clickhouse-setup`, `postgres-setup`, and `redpanda-setup` containers.
- Do not bake credentials into `docker-compose.yml`; read all values from environment variables or `.env.local`.
- Local ports must not conflict across services: ClickHouse 8123/9000, Redpanda 9092/9644, Postgres 5432, OpenFGA 8080, ingest-gateway 4317, auth-service 4318, storage-writer 4320, query-api 8090, frontend 5173.
- `make dev` must be documented in the repo root README as the single starting point for new contributors.

### 19.7 Helm Chart Layout and Rollback

**Helm** (v3) is the chosen deployment tool (ADR-020). Charts live under `charts/` in the
monorepo root:

```
charts/
  observable-common/    # Library chart — shared Deployment, Service, and label templates
  observable/           # Application chart — all six services + migration Job
```

**Common library chart (`observable-common`):**  
Provides three named Go templates: `observable-common.deployment`, `observable-common.service`,
and label/selector helpers. The library chart is a Helm `type: library` dependency of the
application chart. Adding a new service requires one new template file in `charts/observable/templates/`
that calls the common templates; no scaffolding code is duplicated.

**Application chart (`observable`):**  
Declares `observable-common` as a local file dependency. One template file per service. Values
(`charts/observable/values.yaml`) are keyed to mirror `docker-compose.yml` environment variable
names so an operator can cross-reference both without separate documentation.

**Install sequence:**

```bash
# 1. Create migration ConfigMaps from the repo-root migration SQL files
kubectl create configmap observable-migrations-postgres \
  --from-file=migrations/postgres/ --namespace observable
kubectl create configmap observable-migrations-clickhouse \
  --from-file=migrations/clickhouse/ --namespace observable

# 2. Resolve chart dependencies
helm dependency update charts/observable

# 3. Install
helm install observable charts/observable --namespace observable --wait
```

**Rollback path:**

Schema migrations are forward-only (ADR-013). Rolling back a bad application release is safe
when the deployed image version is backward-compatible with the current schema. Any migration
that would break a previous service version is a release blocker.

```bash
# List revisions
helm history observable --namespace observable

# Roll back to a specific revision (does not re-run migration Jobs)
helm rollback observable <revision> --namespace observable --wait
```

Runtime rollback constraints:
- `helm rollback` redeploys the previous Deployment specs; it does **not** re-execute hook Jobs.
- Migrations already applied remain in place. This is intentional: schema backward compatibility
  is a release gate, not a rollback responsibility.
- If a migration must be reversed, write a compensating migration file (ADR-013).

**Local Kubernetes testing (kind):**

```bash
# Lint charts (fast, no cluster needed)
bash scripts/helm-lint.sh

# Full cluster test: create cluster, deploy, smoke check, rollback, teardown
bash scripts/kind-test.sh

# Keep cluster alive for debugging
bash scripts/kind-test.sh --keep-cluster
```

`scripts/kind-test.sh` deploys the same images and env var names as `docker-compose.yml` into
a kind cluster, exercising the complete ingest-to-query path and verifying `helm rollback`
reverts the application tier.

### 19.8 Canary Promotion Pattern

The canary pattern deploys a second instance of one service (`ingest-gateway-canary`) running
a candidate image tag alongside the stable release. The stable Kubernetes Service does not
select canary pods — they are isolated behind a dedicated `ingest-gateway-canary` Service and
are never reached by production traffic.

**Motivation:** provides an automated analysis gate between a CI-built artifact and a full
stable promotion, satisfying the deployment sequence in §19.3 (steps 4–6) without requiring
Argo Rollouts at this stage of the project.

**Canary lifecycle:**

```
helm upgrade (canary.enabled=true) → analysis gates → promote or revert
```

1. The canary Deployment is created alongside stable when `services.ingestGateway.canary.enabled=true`
   and `services.ingestGateway.canary.tag=<new-tag>` are set via `helm upgrade --reuse-values`.
2. `scripts/canary-promote.sh` runs three gates against the canary Service:
   - **Gate 1 (health):** `GET /health` must return HTTP 200 with body containing `ok`.
   - **Gate 2 (smoke ingest):** `POST /v1/traces` must return HTTP 200.
   - **Gate 3 (error rate):** zero HTTP 5xx responses in canary pod logs after the soak period.
3. **Promote (all gates pass):** stable is upgraded to the new tag and the canary Deployment
   and Service are removed in the same `helm upgrade` call.
4. **Revert (any gate fails):** canary is removed via `helm upgrade --set canary.enabled=false`.
   The stable release is untouched. The script exits with status 1.

**Usage:**

```bash
# Run canary analysis against a new image tag (soak 60 s before gate 3)
bash scripts/canary-promote.sh --tag sha-abc1234 --soak-seconds 60

# Additional options
bash scripts/canary-promote.sh --tag sha-abc1234 \
  --namespace observable \
  --release  observable \
  --soak-seconds 60 \
  --dev-key  dev-api-key-0000
```

**Rollback contract:**

| Event | Effect on stable | Effect on schema |
|-------|-----------------|-----------------|
| Gate failure | Unchanged | Unchanged |
| Manual abort (`Ctrl-C`) | Unchanged | Unchanged |
| Successful promotion | Upgraded to new tag | Migration Jobs are not re-run; schema is forward-only (ADR-013) |

The canary Deployment never shares the stable `ingest-gateway` Service selector, so no
production tenant traffic is diverted to the canary pod at any point.

**Relationship to Argo Rollouts:**

`scripts/canary-promote.sh` is a Helm-native skeleton that satisfies the Phase 2 requirement.
Production deployments will use Argo Rollouts for traffic-weighted canary and automated
analysis (spec §19.3), replacing this script when that tooling is provisioned.

---

## 20. Tooling and Framework Recommendations

**Backend**
- Rust
- tonic / gRPC
- protobuf
- Arrow
- DataFusion
- Kafka/Redpanda class broker
- ClickHouse for logs, traces, and metrics
- object storage
- OTel Collector distribution
- OpenFGA for fine-grained auth

**Frontend**
- React 19
- TypeScript
- Vite 8
- TanStack Query
- typed routing
- charting abstraction layer
- Playwright for E2E

**Platform**
- Kubernetes
- Argo CD + Argo Rollouts
- OpenTelemetry everywhere
- Prometheus-compatible internal scraping
- cert-manager
- External Secrets or equivalent
- policy engine
- service mesh only if justified

**DevEx**
- monorepo tooling
- codegen for clients
- protobuf/OpenAPI linting
- ADR docs
- dashboard/alert-as-code packages

---

## 21. Build-vs-Buy Boundaries

**Build:**
- ingest orchestration
- correlation engine
- observability UX
- unified query facade
- tenant model
- topology and SLO domain

**Buy or adopt OSS:**
- auth provider
- billing
- object storage
- CI/CD control
- notification connectors
- feature flags if needed
- incident workflow if not core differentiator

**Anti-pattern:** building commodity IAM before observability value exists.
