# Development Process

## 15. Architecture Decision Records

Maintain ADRs from day 1.

### Required ADR Set

1. OTel as external contract
2. polyglot storage vs single engine
3. ClickHouse adoption boundary
4. Rust for data plane services
5. Arrow/DataFusion query layer
6. React/Vite frontend
7. multi-tenant isolation strategy
8. authorization model
9. queue/stream backbone
10. deployment model: k8s-first
11. sampling strategy
12. retention and tiering
13. schema governance
14. AI feature boundaries
15. build vs buy decisions for incidenting/auth/billing

---

## 16. Development Process Specification

### 16.1 Repo Strategy

Monorepo preferred when:
- shared protobuf/schema packages
- shared UI packages
- shared infra modules
- many internal APIs evolving together

Polyrepo acceptable if org scale requires it.

### 16.2 Branching

- trunk-based
- short-lived feature branches
- feature flags for incomplete work
- protected main
- release branches only when necessary

### 16.3 Definition of Ready

- requirement with acceptance criteria
- domain owner assigned
- telemetry impact identified
- data retention impact identified
- auth impact identified
- runbook/docs impact identified
- ADR/spec synchronization impact identified

### 16.4 Definition of Done

- tests pass
- telemetry emitted
- dashboards/alerts updated if relevant
- security review complete if boundary changed
- load test impact assessed if hot path changed
- ADRs and specs updated together if architecture, technology choices, deployment model, data model, security model, or roadmap scope changed
- migration path documented
- rollback path documented

### 16.5 Engineering Standards

- API-first
- protobuf or OpenAPI contracts required
- backward compatibility policy enforced
- no breaking query semantics without versioning
- every service exports health, metrics, traces, and logs
- every persistent schema change uses migrations

### 16.6 CI and Build Process

CI must make every change reproducible, reviewable, and releasable before merge. The default target is a monorepo with Rust services, TypeScript frontend packages, protobuf/OpenAPI contracts, containerized services, and Kubernetes deployment artifacts.

**Recommended toolchain**
- CI orchestrator: GitHub Actions
- CD/GitOps: Argo CD
- progressive delivery: Argo Rollouts
- local task wrapper: `just`
- Rust checks: `cargo fmt`, `cargo clippy`, `cargo test`, and `cargo nextest` when the workspace is large enough to benefit
- frontend checks: `pnpm`, TypeScript, Vite production build, and Playwright for E2E
- contract checks: `buf` for protobuf and OpenAPI lint/diff tooling when OpenAPI contracts exist
- container builds: Docker Buildx or an equivalent OCI image builder
- supply chain: GitHub OIDC, GitHub artifact attestations, SBOM generation, provenance attestations, image signing, dependency scanning, and secret scanning

**Deferred tools**
- Do not introduce Bazel, Pants, Nx, or a custom build system until native Rust/TypeScript tooling is too slow or inconsistent to operate.
- Do not introduce Buildkite or self-hosted runners until GitHub-hosted runners become a demonstrated bottleneck for cost, queue time, hardware access, or isolation.
- Do not let CI deploy directly to production with imperative cluster commands. CI publishes signed artifacts; GitOps controllers reconcile deployment state.

**Pipeline triggers**
- Pull request to `main`: run validation, build, unit, contract, integration smoke, security, and documentation checks.
- Merge to `main`: repeat required checks, publish versioned build artifacts, and update the integration environment through GitOps.
- Release tag: build immutable release artifacts, sign them, generate SBOM/provenance, and promote through staging to production.
- Nightly: run extended integration, E2E, performance, high-cardinality, chaos, dependency, and vulnerability scans.

**Required PR checks**
1. repository hygiene: formatting, linting, dependency policy, secret scan, and generated-code drift
2. contract checks: protobuf/OpenAPI linting, breaking-change detection, and SDK/client generation
3. backend checks: Rust format, clippy, unit tests, and service-level contract tests
4. frontend checks: TypeScript typecheck, lint, unit tests, production build, and component/smoke tests
5. infrastructure checks: Kubernetes manifest render, policy validation, IaC linting, and migration dry-run where relevant
6. security checks: dependency audit, container scan for changed images, SBOM generation, and license policy
7. documentation checks: ADR/spec synchronization review, Markdown link checks, and diagram syntax checks where supported

**Build outputs**
- Rust service binaries compiled in release mode
- frontend static assets
- protobuf/OpenAPI generated clients and schema artifacts
- container images for deployable services
- Helm/Kustomize render output for each environment profile
- database migration bundles
- SBOMs, provenance attestations, image signatures, and checksums

**Artifact rules**
- Every artifact must be traceable to a git commit, CI run, source branch, and dependency lockfile.
- Container images must use immutable tags based on commit SHA and release version; mutable tags are aliases only.
- Generated code must be committed only when required by the repo layout and must be checked for drift in CI.
- Build scripts must be deterministic and runnable locally without requiring production credentials.
- Secrets must never be baked into artifacts, images, generated config, or test fixtures.

**Promotion gates**
1. PR validation passes and human review approves.
2. Merge to `main` publishes artifacts and deploys to shared integration.
3. Integration smoke and contract suites pass.
4. Release candidate deploys to perf/staging.
5. Performance, security, migration, backup/restore, and rollback checks pass for release scope.
6. Canary deploys to an internal tenant.
7. Automated analysis validates platform SLOs, error budgets, logs, traces, queue lag, query latency, and alert latency.
8. Production rollout proceeds progressively and rolls back automatically on SLO regression.

**Failure handling**
- Required PR checks block merge.
- Failed artifact signing, SBOM generation, provenance generation, or secret scanning blocks release.
- Failed integration or staging deployment blocks promotion, not unrelated PR validation.
- Flaky checks must be quarantined only with an owner, expiry date, linked issue, and replacement signal.
- CI failures that affect hot-path correctness, tenant isolation, auth, schema compatibility, or migrations require root-cause notes before retrying.

### 16.7 Local Development Workflow

Local development must use the same source-controlled tasks and contract checks as CI, while keeping the feedback loop fast enough for daily work.

**Required local tools**
- Rust toolchain pinned by repo configuration
- Node.js and `pnpm` pinned by repo configuration
- `just` for task entry points
- Docker or an equivalent OCI-compatible local runtime
- `buf` once protobuf contracts exist
- Kubernetes local runtime only for integration work, preferably `kind`

**Local task contract**
- `just setup`: install/check local tool prerequisites and hooks
- `just fmt`: run repository formatting
- `just lint`: run static checks that do not require external services
- `just test`: run unit tests for changed packages by default, with an option for the full suite
- `just contract`: run protobuf/OpenAPI lint, generation, and breaking-change checks
- `just dev`: start the minimal local service graph for the current feature
- `just smoke`: run local smoke checks against the running service graph
- `just ci-local`: run the closest practical local equivalent of required PR checks

**Runtime modes**
- **Unit mode:** no external services; default for fast backend/frontend tests.
- **Service mode:** run one service plus mocked dependencies or lightweight local dependencies.
- **Compose mode:** run dependencies such as ClickHouse, Kafka/Redpanda-compatible broker, object storage emulator, and OpenFGA-compatible store for integration smoke.
- **Kind mode:** run Kubernetes manifests in a local `kind` cluster when validating deployment, service discovery, ingress, policy, or operator behavior.

**Local data and credentials**
- Use synthetic telemetry, golden traces/log bundles, malformed payload samples, and high-cardinality fixture slices from the test corpus.
- Local tenants, users, API keys, and OIDC identities must be deterministic fixtures.
- Production, staging, and shared integration credentials must never be required for local development.
- Secrets must be loaded from local-only files or developer secret stores that are ignored by git.

**Developer loop**
1. create a task branch
2. run `just setup` when tool versions or hooks change
3. run targeted `just fmt`, `just lint`, `just test`, and `just contract` while editing
4. use `just dev` and `just smoke` for service-level validation
5. run `just ci-local` before opening a PR when the change affects code, contracts, migrations, deployment, auth, tenancy, or hot-path behavior

**Local boundaries**
- Local runs may use reduced fidelity, smaller datasets, and mock external providers.
- Local runs must preserve tenant context, auth boundaries, schema validation, and migration behavior.
- Local tooling must not depend on cloud-only services unless the feature explicitly integrates with that provider.
- CI remains authoritative for release artifacts, SBOMs, provenance, signing, container scanning, and promotion.

### 16.8 AI Agent Guidance

When utilizing AI agents for development, the following mandates apply:

- **No Unreviewed Merges:** Nothing can be merged or committed to the main branch without a human review.
- **Branch and PR Every Iteration:** Before changing files, the agent must create or switch to a dedicated short-lived branch for the current task. The agent must commit only to that branch, push it to GitHub, and open a pull request for every iteration.
- **Verification & Testing:** Every change must be thoroughly tested and verified before being considered complete.
- **Clarity Above All:** Nothing can be left unclear. If instructions, requirements, or code changes are ambiguous, the agent must seek clarification before proceeding.
- **Specification Alignment:** All changes must align with the core architectural principles and specifications defined in the `spec/` directory.
- **ADR and Spec Synchronization:** Architecture, technology, deployment, data model, security, and roadmap changes must update both the relevant ADRs and affected specs in the same iteration. If an ADR change is not required, the PR must explain why.

---

## 17. Project Plan: Small Steps to Production

### 17.1 Planning Rules

The roadmap is staged to prove the risky foundations before broadening the product surface.

1. Every phase has an explicit exit gate.
2. Tenant isolation, ingest durability, cardinality controls, and internal telemetry are not optional hardening items; they are part of the first runnable platform.
3. MVP means internally dogfoodable. v1 means externally supportable for selected production customers.
4. Do not start advanced telemetry, incident workflows, or AI features until the ingest, query, retention, and authorization foundations are measured under load.
5. Any new phase work must identify contract, data-retention, auth, test, rollback, and telemetry impacts before implementation starts.

### Phase 0 — Foundations

1. Finalize product scope, target users, and success metrics.
2. Write system context and container diagrams.
3. Define the canonical telemetry domain model and OTel attribute mapping.
4. Define shared multi-tenant, isolated-storage, and single-tenant deployment models.
5. Create ADR template and ratify the required ADR set.
6. Define repo layout, release conventions, CI gates, and codegen strategy.
7. Define security baseline: OIDC, workload identity, secret handling, SBOM, artifact signing, and audit requirements.
8. Define local, CI ephemeral, integration, perf/staging, production, and regulated/single-tenant environments.
9. Define platform SLOs and first non-functional acceptance targets.
10. Create the initial staffing and ownership map.

**Exit gate:** all foundational specs exist, the initial ADR set is Accepted, acceptance targets are documented, and the implementation backlog can be traced back to specs.

### Phase 1 — Internal MVP: Ingest to Query

1. Scaffold the monorepo, CI, protobuf/OpenAPI linting, migrations, and service templates.
2. Build OTLP HTTP/gRPC ingest for traces and logs first.
3. Add tenant authentication with API keys and workload identity.
4. Add durable buffering before expensive transforms.
5. Implement validation, normalization, tenant routing, and idempotent write design.
6. Store traces and logs in ClickHouse using the canonical domain model.
7. Add basic metrics ingestion and storage, using ClickHouse initially unless a TSDB decision is ratified.
8. Expose initial trace, log, metric, and configuration query APIs.
9. Ship a simple React UI for trace search, log search, metric exploration, and one dashboard view.
10. Emit platform telemetry from every service and publish internal health dashboards.
11. Run internal dogfooding with synthetic and real service telemetry.

**Exit gate:** a tenant can ingest telemetry, query it through API and UI, and survive a single queue or storage node failure without committed buffered data loss.

### Phase 2 — Governed MVP: Isolation, Cost, and Release Safety

1. Enforce tenant isolation in API, query, storage, and policy tests.
2. Add tenant-aware rate limits, ingest quotas, and basic cardinality budgets.
3. Add PII masking/redaction hooks in the ingest pipeline.
4. Add hot retention policies and deletion workflows for traces, logs, and metrics.
5. Add audit logging for ingest credentials, queries, config changes, and admin actions.
6. Add RBAC for tenant admin, project admin, member, and viewer roles.
7. Add basic alert definitions and threshold evaluation.
8. Add Kubernetes deployment manifests, GitOps delivery, canary rollout, and rollback gates.
9. Add perf smoke tests for ingest throughput, common query latency, and dashboard load.

**Exit gate:** internal tenants can run the platform continuously with measured cost controls, enforced RBAC, audit trails, and progressive deployment rollback.

### Phase 3 — Correlation and Service Operations

1. Implement trace-log correlation.
2. Build the service catalog from OTel resource attributes and deployment metadata.
3. Add service maps and topology rollups.
4. Add deployment/change events.
5. Generate RED metrics from spans.
6. Add Kubernetes metadata enrichment.
7. Improve search, filtering, and deep links across signals.
8. Add dashboard-as-code and alert-as-code import/export.

**Exit gate:** users can move from service to trace, log, metric, deployment, and dashboard context without manual ID copying.

### Phase 4 — v1 Production Readiness

1. Add warm retention tiers, compaction, and restore procedures.
2. Add backup/restore drills and RPO/RTO documentation per retention tier.
3. Add SSO/OIDC integration and SCIM if required by target v1 customers.
4. Add OpenFGA-style ReBAC for dashboards, projects, environments, incidents, and data scopes.
5. Add SLO definitions, burn-rate calculations, and burn-rate alerts.
6. Add production incident runbooks for ingest, query, storage, auth, and deployment failures.
7. Add cost reporting, cardinality diagnostics, and tenant usage reports.
8. Run load, chaos, tenant-escape, and upgrade/rollback test suites.
9. Complete security review for auth, tenancy, query, and ingestion boundaries.

**Exit gate:** selected production customers can use v1 with documented support boundaries, measured SLOs, rollback paths, and customer-facing runbooks.

### Phase 5 — Reliability Product

1. Add incident timelines.
2. Add notification routing and on-call integrations.
3. Add runbook workflows.
4. Add topology-aware impact analysis.
5. Add composite alerts and alert suppression.
6. Add reliability reporting by service, team, environment, and tenant.

**Exit gate:** the platform supports the full detect, triage, notify, and review loop for service reliability.

### Phase 6 — Advanced Telemetry

1. Add continuous profiling with separate storage and symbol handling.
2. Add browser RUM.
3. Add mobile observability.
4. Add synthetics.
5. Add eBPF-assisted enrichment where operationally justified.
6. Add session replay only after privacy, storage, and cost controls are proven.

**Exit gate:** each advanced signal is modular, governed by retention and privacy policy, and correlated through the shared identity model.

### Phase 7 — Enterprise Readiness

1. Add regional residency controls.
2. Add BYOK.
3. Add tenant-isolated deployment packaging.
4. Add compliance reporting.
5. Add billing/metering integration.
6. Add marketplace/private deployment packaging.
7. Define multi-region active-active only after single-region/multi-AZ operations are stable.

**Exit gate:** enterprise deployment variants are supportable without forking the product architecture.

### Phase 8 — Intelligence

1. Add anomaly models after historical retention and labeling are reliable.
2. Add query recommendations.
3. Add incident summarization.
4. Add capacity forecasting.
5. Add remediation hooks with explicit approval controls.

**Exit gate:** AI features are explainable, auditable, reversible, and never required for core platform correctness.
