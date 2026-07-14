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
- no new errors, warnings, regressions, flaky tests, type errors, lint findings, generated-code drift, policy violations, or documentation check failures are introduced by the change
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

**Pipeline triggers**
- Pull request to `main`: run validation, build, unit, contract, integration smoke, security, and documentation checks.
- Merge to `main`: repeat required checks, publish versioned build artifacts, and update the integration environment through GitOps.
- Release tag: build immutable release artifacts, sign them, generate SBOM/provenance, and promote through staging to production.
- Nightly: run extended integration, E2E, performance, high-cardinality, chaos, dependency, and vulnerability scans.

**Required PR checks**
1. repository hygiene: formatting, linting, dependency policy, secret scan, and generated-code drift
2. contract checks: protobuf/OpenAPI linting, breaking-change detection, and SDK/client generation
3. backend checks: Rust format, clippy, unit tests, service-level contract tests, and applicable Testcontainers integration tests for real dependency boundaries
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

### 16.7 AI Agent Guidance

When utilizing AI agents for development, the following mandates apply:

- **No Unreviewed Merges:** Nothing can be merged or committed to the main branch without a human review.
- **Branch and PR Every Iteration:** Before changing files, the agent must create or switch to a dedicated short-lived branch for the current task. The agent must commit only to that branch, push it to GitHub, and open a pull request for every iteration.
- **Verification & Testing:** Every change must be thoroughly tested and verified before being considered complete. An iteration is not complete if it introduces any new error, warning, regression, flaky test, type error, lint finding, generated-code drift, policy violation, or documentation check failure.
- **Testcontainers for Real Dependencies:** Backend changes that touch PostgreSQL, ClickHouse, Redpanda/Kafka-compatible brokers, object storage, OpenFGA, or another real containerized dependency boundary must add or update the narrowest applicable Testcontainers integration test unless the slice explicitly requires Docker Compose, kind, browser, or external-provider verification instead. If Testcontainers is not applicable, the PR must state why and name the replacement signal.
- **Clarity Above All:** Nothing can be left unclear. If instructions, requirements, or code changes are ambiguous, the agent must seek clarification before proceeding.
- **Specification Alignment:** All changes must align with the core architectural principles and specifications defined in the `spec/` directory.
- **Implementation Plan Adherence:** All tasks must follow the latest implementation plans and iteration documents located in `docs/superpowers/plans/`.
- **Finished Plan Archiving:** When a detailed task plan is completed, remove it from `docs/superpowers/plans/` (or store it privately) in the same iteration and update all active-plan and agent-context links that pointed at it. Do not reintroduce a public `archived/` directory of internal plan history.
- **ADR and Spec Synchronization:** Architecture, technology, deployment, data model, security, and roadmap changes must update both the relevant ADRs and affected specs in the same iteration. If an ADR change is not required, the PR must explain why.

- **NLQ Quality Gate:** Any change that affects the NLQ→IR→SQL pipeline — including the system
  prompt (`build_system_prompt()` in `llm_adapter.rs`), the IR schema (`NlqIr` struct or field
  semantics), SQL templates (`sql_templates.rs`), the metadata injection logic, the IR parser or
  repair loop, or the eval test cases (`tests/nlq/cases.json`) — must:
  1. Include or update cases in `tests/nlq/cases.json` covering the changed or added behavior.
  2. Run `python3 scripts/nlq-eval.py` against a running cluster and record the pass/fail summary
     in the PR description.
  3. Show that the changed behavior now passes and that no previously-passing case has regressed.
  The eval harness is a protected regression gate equivalent to `local-ci.sh`. Do not weaken,
  skip, or remove assertions without a replacement signal and explicit reviewer approval.
  See [spec/08-ai-ml.md §13.4](08-ai-ml.md) for the full operation reference and
  feedback loop.

**MANDATORY: Before Pushing ANY Code**

You **MUST** run the following checks before pushing **ANY** code changes to the repository. No exceptions. Do not push and rely on CI to catch errors.

**Note:** Pure documentation changes (e.g., files under `docs/`, `spec/`, or any `.md` files) are exempt from these mandatory code checks.

1. Run `cargo fmt --all` — fix all formatting issues.
2. Run `cargo clippy --workspace --all-targets -- -D warnings` — fix all warnings in workspace crates and targets.
3. Run `cargo test --workspace --all-targets` — ensure all workspace tests pass.
4. If Docker is available:
   - Run `docker compose up -d` to ensure the stack is running.
   - Run `docker compose up smoke-test --abort-on-container-exit` — all checks MUST pass.

If any check fails, you **MUST** fix it before pushing.

For any Rust code change, `cargo fmt --all` must be run explicitly before pushing, even when `bash scripts/local-ci.sh` is also required and also runs formatting.

**Regression gate stewardship**

Agents must treat `scripts/local-ci.sh`, `tests/e2e/smoke_test.sh`, `scripts/perf-smoke.sh`, and Docker Compose verification services as protected regression gates.

- Before changing a regression gate, state the current coverage it provides and the exact coverage that will exist after the change.
- Never delete, weaken, skip, or quarantine a regression assertion unless the PR includes a replacement signal, linked issue, owner, expiry date, and explicit reviewer approval.
- Regression-gate changes must preserve existing build and product functionality. Run the narrowest affected check first, then the required local gate for the touched surface.
- Testcontainers tests are protected regression signals once introduced. Do not replace them with mocks, shared local databases, or broad smoke tests unless the PR explains the lost coverage and includes a reviewer-approved replacement.
- Performance-sensitive changes must run `docker compose up perf-smoke --abort-on-container-exit` or explain why the performance gate is not relevant.

### 16.8 Tiny Agent Iteration Workflow

Agents must move from specification to final product through small, reviewable vertical slices. An iteration should be small enough that a reviewer can understand the intent, diff, tests, and remaining risk in one sitting.

**Iteration size limits**
- Prefer one behavior, one API endpoint, one schema change, one UI state, one deployment check, or one documentation correction per iteration.
- Keep pull requests below roughly 300 changed lines unless generated files or mechanical migrations make that impossible.
- Avoid mixing concerns. Do not combine unrelated backend, frontend, infrastructure, and documentation work unless the slice cannot function without all of them.
- Do not start the next slice until the current branch has a PR with verification notes and a clear merge or follow-up path.

**Spec-to-slice loop**
1. **Select source spec:** identify the exact spec, ADR, phase item, and acceptance target driving the work.
2. **Define the smallest user-visible or operator-visible outcome:** state what will work after this slice that did not work before.
3. **Write the slice contract:** list inputs, outputs, touched boundaries, test evidence, telemetry impact, rollback path, and any ADR/spec sync requirement.
4. **Create a branch:** use a short-lived branch named for the slice before editing files.
5. **Implement the smallest coherent change:** prefer a thin end-to-end path over a broad partial subsystem.
6. **Verify locally:** run the narrowest useful checks first, then the required local/CI-equivalent checks for the touched area. Compare results with the known baseline and fix any new failure before opening the PR.
7. **Update docs/specs/ADRs:** keep implementation, specs, and architectural decisions aligned in the same branch.
8. **Retire completed detailed plans:** when the slice completes a detailed task plan, remove the finished plan from `docs/superpowers/plans/` (store it privately if you want to retain the history) and update active roadmap and `docs/agent-context.md` links in the same PR.
9. **Open a PR:** include source spec links, acceptance criteria, verification output, known gaps, rollback notes, and the next suggested tiny slice.
10. **Wait for review or CI signal:** only stack follow-up work when the dependency is explicit and the new branch targets the previous PR branch.

**Slice contract template**

```markdown
Source spec:
Acceptance target:
User/operator outcome:
Files or modules expected to change:
Out of scope:
Verification:
Baseline:
New errors introduced:
Telemetry impact:
Auth/tenancy impact:
Data retention or migration impact:
Rollback path:
ADR/spec sync:
Next smallest slice:
```

**Preferred product-building sequence**
1. Start with a failing or pending contract test that names the expected behavior.
2. Add the minimum domain/API shape needed to satisfy the contract.
3. Add the smallest persistence or integration path needed for the behavior.
4. Add a minimal UI or CLI path only when the backend contract exists.
5. Add telemetry, docs, and runbook notes in the same slice when the behavior is operationally meaningful.
6. Harden with follow-up slices: edge cases, performance, resilience, security, and UX polish.

**Agent PR requirements**
- PR titles must describe the tiny outcome, not the broad phase.
- PR bodies must include the slice contract or a concise equivalent.
- PRs must state whether ADRs and specs are in sync. If no ADR update is required, explain why.
- PRs must state whether any new errors were introduced. The expected answer is `none`; any exception requires an explicit owner, issue, expiry, and reviewer approval.
- PRs must identify any skipped checks and why they were not relevant or could not run.
- PRs must name the next smallest useful slice so the project can continue without re-planning from scratch.

### 16.9 Documentation and Spec Review

Any agent PR that touches files under `spec/` or `docs/` must run the `doc-review` skill and pass all four phases before opening a PR or claiming the change complete.

**Trigger rule:** Mandatory whenever the agent modifies any file under `spec/` or `docs/`. Not optional. Must complete before `superpowers:verification-before-completion` or PR creation.

**Phases — all must pass:**

A **WARN** indicates a finding that is noted but does not block the PR (e.g., a cross-reference that is stale but not incorrect, or a missing bidirectional link to an external resource outside the repo). A **FAIL** indicates a finding that makes the change incorrect or incomplete and blocks the PR.

#### Phase 1: Structural Validation
- Valid Markdown: no unclosed fences, broken headings
- No bare `TODO` or `TBD` placeholders remaining in the document
- ADR files must contain: Status, Context, Decision, Consequences sections
- Spec files must have a numbered heading consistent with their filename
- Diagrams (Mermaid or similar) must have valid syntax

#### Phase 2: Cross-Reference Consistency
For every ADR or spec referenced in a changed file:
- The linked file exists at the stated path
- The reference is bidirectional (if spec A links to ADR-007, ADR-007 must be consistent with spec A)
- The description of the linked decision matches what the linked file actually says

#### Phase 3: Coverage Completeness
- If a spec change touches architecture, technology choices, deployment model, data model, security model, or roadmap scope → an ADR must also be touched, or the PR must explicitly state why no ADR change is needed
- If an ADR is touched → all specs that reference that ADR must be checked for staleness
- `spec/README.md` table must accurately reflect any added, renamed, or removed spec files

#### Phase 4: Quality Gates
- No contradictions between changed files and the rest of the corpus
- No sections removed without a replacement or an explicit note explaining the removal
- Changed files maintain accurate cross-links to related specs

**Report format:**

```
## Doc/Spec Review Report

### Phase 1: Structural Validation — PASS | WARN | FAIL
- [finding] → [file:line]

### Phase 2: Cross-Reference Consistency — PASS | WARN | FAIL
- [finding] → [file:line] ↔ [linked file:line]

### Phase 3: Coverage Completeness — PASS | WARN | FAIL
- [finding] → [spec file] requires ADR update OR [ADR file] requires spec sync

### Phase 4: Quality Gates — PASS | WARN | FAIL
- [finding] → [contradiction or gap location]

### Summary
Overall: PASS | FAIL
Warnings requiring PR acknowledgement: N
Blockers requiring fix before PR: N
```

**Failure handling:**
- `FAIL` in any phase: agent fixes the issue and re-runs from Phase 1. Cannot open a PR until all phases pass.
- `WARN`: agent lists all warnings in the PR body under "Acknowledged doc/spec review warnings."
- `PASS` (all phases): agent notes "Doc/spec review: all phases passed" in the PR body.

---

### 16.10 Dependency Maintenance Policy

#### Pinning rules

| Ecosystem | Version specifier | Lockfile | Notes |
|---|---|---|---|
| Rust crates | `^major.minor` in `Cargo.toml` | `Cargo.lock` committed | Lockfile is the exact pin; use `cargo` commands for dependency updates |
| npm packages | `^major` in `package.json` | `package-lock.json` committed | Lockfile is the exact pin; use npm only, never yarn/pnpm/bun |
| Python packages | `pyproject.toml` | `uv.lock` committed | Use uv; if not yet uv-managed, plan the uv migration before changing Python dependencies |
| Docker Compose (local/dev) | `image:major.minor` minimum | n/a | Keep versions aligned with matching Testcontainers fixtures |
| Production Dockerfiles / base images | `image:major.minor.patch` | SHA digest strongly preferred | |
| GitHub Actions | `action@vN` (latest major tag) | n/a | |

Lockfiles are always committed. Range specifiers without committed lockfiles are not permitted.

Dependency and image changes must use the ecosystem-native tool:

- Rust crates: use `cargo add`, `cargo update`, or the narrowest applicable cargo command. Do not hand-edit `Cargo.lock` entries.
- npm packages: use npm commands only (`npm install`, `npm update`, `npm audit`, `npm ci`). Do not use yarn, pnpm, bun, or another package manager for npm dependency work.
- Python packages: use uv. If the relevant Python tooling still uses `requirements.txt`, `pip`, Poetry, or Pipenv, the PR must include or link a migration plan to `pyproject.toml` plus `uv.lock` before changing dependencies.
- Docker images: prefer the latest stable release. Search and update every matching reference in Docker Compose files, Dockerfiles, Helm values/templates, scripts, and Testcontainers fixtures. When the same dependency is started by Docker Compose and Testcontainers, the image version must be identical unless the PR documents a deliberate compatibility exception.

#### Update cadence

- **Routine:** monthly sweep — bump all dependencies to the latest stable version within the declared range, run `bash scripts/local-ci.sh`, open a dedicated PR.
- **Security (critical or high CVE):** 7-day SLA from public disclosure. Patch-only bumps bypass the monthly cycle.
- **Security (medium CVE):** 30-day SLA.
- **Breaking upgrades** (major version bumps, image EOL): treated as a feature slice — the PR must include a source spec reference, acceptance target, and rollback note. Do not bundle breaking upgrades with routine dependency updates.

#### Automation

Dependabot or Renovate is the preferred tool for surfacing routine update PRs. Configuration lives in `.github/dependabot.yml` or `renovate.json`. Automation is not required immediately but is the target state. Configure it once the core CI pipeline described in §16.6 is operational.
For CI-level dependency audit, SBOM generation, and license policy enforcement, see §16.6 PR check #6.

#### Ownership

- The PR author is responsible for verifying the update does not break `bash scripts/local-ci.sh` before pushing. No exceptions.
- Routine dependency PRs must state: what changed, whether local-ci passed, and whether any lockfile drift was introduced.

## 17. Project Plan: Small Steps to Production

### 17.1 Planning Rules

The roadmap is staged to prove the risky foundations before broadening the product surface.

1. Every phase has an explicit exit gate.
2. Tenant isolation, ingest durability, cardinality controls, and internal telemetry are not optional hardening items; they are part of the first runnable platform.
3. MVP means internally dogfoodable. v1 means externally supportable for selected production customers.
4. Do not start advanced telemetry, incident workflows, or AI features until the ingest, query, retention, and authorization foundations are measured under load.
5. Any new phase work must identify contract, data-retention, auth, test, rollback, and telemetry impacts before implementation starts.
6. Every numbered phase item must be decomposed into tiny agent iterations before implementation. A phase item is not ready for execution until its first slice has a source spec, acceptance target, verification plan, and rollback note.
7. Agents should prefer vertical walking skeletons over horizontal platform layers: prove ingest-to-query before broadening signal coverage; prove one tenant-safe path before adding more roles or environments.

### 17.2 Agent Backlog Decomposition

Each phase item should be split into three levels:

| Level | Purpose | Example |
|---|---|---|
| Phase item | Product capability or risk reduction target | Build OTLP HTTP/gRPC ingest for traces and logs |
| Slice | One reviewable behavior with tests | Accept one valid OTLP trace payload and return a durable acknowledgement |
| Task | Local implementation step inside the slice | Add route, parser test, tenant lookup stub, and ingest metric |

Agents must not turn phase items directly into large implementation PRs. The first slice for any capability should establish the contract and a runnable path; later slices can expand coverage, performance, and edge cases.

**Tiny slice examples for Phase 1**

| Phase item | First tiny slice | Follow-up slices |
|---|---|---|
| Monorepo and CI scaffold | Create workspace skeleton with one Rust service crate, one frontend package placeholder, and required format/lint tasks | Add protobuf lint; add container build; add docs link checks |
| OTLP ingest | Accept a minimal trace request through one protocol with tenant-scoped validation and a contract test | Add logs; add gRPC; add malformed payload corpus; add backpressure behavior |
| Tenant authentication | Validate one API key against an in-memory/test fixture store | Add hashed key storage; add workload identity; add audit logs; add rotation |
| Durable buffering | Write one normalized envelope to the queue with idempotency key and retry test | Add partition strategy; add dead-letter handling; add queue lag telemetry |
| ClickHouse writes | Persist one span table shape behind a repository interface and migration | Add logs table; add batch writer; add dedupe; add retention policy |
| Query APIs | Return one trace by `tenant_id + trace_id` from a repository contract | Add log search; add metric query; add pagination; add auth scopes |
| React UI | Render trace search form against a mocked API contract | Connect live API; add loading/error states; add deep link; add smoke test |

**Definition of a complete tiny slice**
- The slice maps to a spec or ADR.
- The changed behavior can be demonstrated by a test, local command, screenshot, API example, or rendered artifact.
- The PR can be reverted without leaving partially adopted architecture behind.
- Follow-up work is explicit and smaller than the parent phase item.

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
7. Add basic metrics ingestion and storage using ClickHouse (DONE).
8. Expose initial trace, log, metric, and configuration query APIs (DONE).
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
