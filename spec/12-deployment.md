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

---

## 20. Tooling and Framework Recommendations

**Backend**
- Rust
- tonic / gRPC
- protobuf
- Arrow
- DataFusion
- Kafka/Redpanda class broker
- ClickHouse for logs/traces
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
