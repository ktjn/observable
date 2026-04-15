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

### 16.6 AI Agent Guidance

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
