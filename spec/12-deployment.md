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

Local development uses Docker Compose for external dependencies and Rust services. The React frontend runs natively on the developer's machine.

**Quick start**

```bash
make dev                       # start Docker Compose dependency stack
bash scripts/migrate.sh        # apply ClickHouse and PostgreSQL migrations
bash scripts/start-services.sh # build and start Rust services in Docker Compose
npm run dev                    # run the React frontend (from apps/frontend)
```

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

The Rust service containers are built from the repo-root `Dockerfile` and run through the Compose `services` profile.

| Service          | Host port | Internal dependencies                    |
|------------------|-----------|------------------------------------------|
| auth-service     | 4318      | postgres                                 |
| storage-writer   | 4320      | clickhouse                               |
| stream-processor | none      | redpanda, storage-writer                 |
| ingest-gateway   | 4317      | auth-service, redpanda                   |
| query-api        | 8090      | clickhouse                               |

**Configuration**

- Copy `.env.local.example` (committed) to `.env.local` (gitignored) at the repo root.
- Each Rust service reads config from environment variables supplied by `docker-compose.yml`. `.env.local` supplies local defaults for dependency credentials and host-facing ports.
- No production secrets are required for local development.

**Schema migrations**

- In local mode, run `bash scripts/migrate.sh` after starting the Compose dependency stack and before starting the Rust service containers.
- In CI and production, migrations are explicit pipeline steps and do not run automatically on service startup.

**Rules**

- `docker compose up` must start cleanly from scratch with no manual seed steps beyond those automated in `make dev`, `scripts/migrate.sh`, and `scripts/start-services.sh`.
- Do not bake credentials into `docker-compose.yml`; read all values from environment variables or `.env.local`.
- Local ports must not conflict across services: ClickHouse 8123/9000, Redpanda 9092/9644, Postgres 5432, OpenFGA 8080, ingest-gateway 4317, auth-service 4318, storage-writer 4320, query-api 8090.
- `make dev` must be documented in the repo root README as the single starting point for new contributors.

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
