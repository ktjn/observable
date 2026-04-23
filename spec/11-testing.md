# Test Strategy

## 18. Test Strategy

### 18.1 Testing Layers

**Unit**
- parsers
- query builders
- auth policies
- retention logic
- alert evaluators
- UI state reducers/hooks

**Contract**
- OTLP compatibility
- API schema compatibility
- SDK compatibility
- auth provider compatibility

**Integration**
- ingest → queue → storage
- query → storage pushdown
- alert → notification
- tenant isolation
- collector/agent interoperability

**End-to-end**
- service emits telemetry
- telemetry visible in UI
- alert fires
- incident created
- deep links resolve
- dashboard renders
- RBAC enforced

**Performance**
- ingest throughput
- query latency
- high-cardinality stress
- compaction impact
- backpressure behavior
- cold-tier restore

**Chaos/resilience**
- broker outage
- storage node loss
- region loss
- partial auth outage
- queue lag surge
- clock skew
- duplicate ingest

**Security**
- auth bypass
- tenant escape
- injection on query DSL
- SSRF in integrations
- PII leak tests
- secret exposure
- supply chain gates

### 18.2 Test Data Strategy

- synthetic telemetry generators
- replay corpus from known workloads
- golden traces/log bundles
- high-cardinality torture datasets
- malformed payload corpus

### 18.3 Non-Functional Acceptance Targets

| Metric | Target |
|--------|--------|
| ingest availability | 99.95% |
| query API availability | 99.9% |
| P50 query latency (hot path) | < 1s |
| P95 common dashboard load | < 3s |
| alert detection latency | < 60s |
| tenant cross-read | zero in all policy tests |
| data loss | zero for committed buffered ingest under single-node failure |

### 18.4 CI Test Gates

CI gates map the test strategy to merge and release decisions.

| Gate | Trigger | Required scope |
|------|---------|----------------|
| PR fast path | pull request | formatting, linting, unit tests, contract linting, changed-package builds, docs checks |
| PR integration smoke | pull request | changed service integration tests, tenant isolation policy tests, migration dry-runs |
| Main integration | merge to `main` | full unit suite, contract suite, integration suite, generated client drift checks |
| Nightly extended | scheduled | E2E workflows, performance smoke baseline (`scripts/perf-smoke.sh`), high-cardinality datasets, malformed payload corpus, chaos/resilience, dependency scans |
| Release candidate | release tag or promotion branch | full E2E, performance, security, backup/restore, rollback, upgrade, and migration tests |
| Production canary | progressive rollout | SLO analysis, alert latency, queue lag, ingest error rate, query latency, tenant isolation smoke |

**Performance smoke baseline** (`scripts/perf-smoke.sh`): runs 20 samples against every ingest and query endpoint, reports P50 and P95 per path, and exits non-zero if any path exceeds its threshold (ingest P50 < 500 ms / P95 < 1000 ms; query P50 < 1000 ms / P95 < 3000 ms per §18.3). Run locally with `docker compose up perf-smoke --abort-on-container-exit`. Thresholds are overridable via env vars (`P50_INGEST_MAX_MS`, `P95_INGEST_MAX_MS`, `P50_QUERY_MAX_MS`, `P95_QUERY_MAX_MS`).

Run `docker compose up perf-smoke --abort-on-container-exit` for changes that can
affect ingest latency, query latency, dashboard load, ClickHouse query shape,
storage writes, stream processing, Docker resource limits, or service startup
ordering. If skipped, the PR must explain why performance is not in scope.

Tests that guard tenant isolation, auth bypass, schema compatibility, migrations, and data loss are release blockers. Performance and chaos failures are promotion blockers unless explicitly waived by the responsible domain owner with a documented expiry.

### 18.5 Agent Iteration Verification

Tiny agent iterations must include enough verification for the slice they change without forcing unrelated full-suite work into every PR.

The standard is **no new errors introduced**. A PR may document pre-existing failures in the target branch, but it must not add, mask, downgrade, or normalize any new failure. If a baseline check already fails before the change, the agent must capture that baseline and prove the iteration does not worsen it.

| Slice type | Minimum verification |
|------------|----------------------|
| Spec or ADR only | diff review, Markdown/link or diagram check when available, ADR/spec synchronization note |
| API or schema contract | contract test, generated-code drift check, backward-compatibility check |
| Backend behavior | focused unit test, service contract test, relevant lint/format checks |
| Ingest or storage path | parser/normalization test, persistence or queue integration smoke, tenant partition assertion |
| Auth or tenancy | positive and negative policy tests, tenant-cross-read denial test |
| Frontend behavior | component/unit test or browser smoke for the changed route/state, typecheck |
| Deployment or CI | render/dry-run check, policy validation, rollback note |

### 18.6 Per-Iteration No-Regression Rules

Every iteration must use a change-scoped test plan before implementation and report the actual result in the PR.

**Current mandatory smoke coverage**

`scripts/local-ci.sh` runs `docker compose up smoke-test --abort-on-container-exit`
for code changes unless Docker or smoke tests are explicitly skipped. The smoke
test must continue to prove at least one successful path for trace ingest, trace
detail query, trace search, log ingest, metric ingest, OTLP gRPC ingest, and
service discovery.

**Required sequence**
1. Identify touched surfaces: docs, API/schema, backend, frontend, ingest/storage, auth/tenancy, deployment, CI, data migration, or security.
2. Select the minimum checks from `18.5` plus any existing repo-wide checks required for that surface.
3. Run a baseline check first when the target branch is suspected to be red or flaky.
4. Implement the change.
5. Re-run the selected checks and any tests that previously failed in the same touched area.
6. Fix all new failures before opening the PR.
7. Record commands, results, baseline status, skipped checks, and the statement `New errors introduced: none`.

**New errors include**
- test failures, snapshot drift, type errors, lint findings, formatting drift, generated-code drift, dependency audit findings, policy violations, secret scan findings, broken links, invalid diagrams, failed migration dry-runs, runtime panics, browser console errors, accessibility regressions, and newly flaky tests
- reduced coverage on touched behavior unless the PR explains why coverage is intentionally moved or replaced
- skipped, weakened, deleted, or quarantined tests without an issue, owner, expiry date, and reviewer approval

**Baseline handling**
- If the baseline is green, the iteration must leave the selected checks green.
- If the baseline is red for unrelated known failures, the PR must list the failing checks and show the same checks do not regress after the change.
- If the change touches an area with a red baseline, fixing the baseline is preferred. If that is too large, the PR must include a narrow passing test that covers the new behavior.
- Agents must not mark a PR complete when the only verification is "tests already failed" or "checks could not run" without narrower evidence for the changed behavior.

**PR verification block**

```markdown
Checks run:
Baseline:
Result after change:
New errors introduced: none
Skipped checks:
Known pre-existing failures:
Follow-up test debt:
```

Agent PRs must report:
- commands run and their result
- baseline status when relevant
- checks intentionally skipped and why
- confirmation that no new errors were introduced
- evidence that specs and ADRs are synchronized
- the next smallest slice needed to continue toward the phase exit gate

---

### 18.7 Kubernetes Test Strategy

Kubernetes-specific testing follows the same layered model as other test categories, with
additional gates at the manifest and cluster-level.

**Layer 1 — Manifest validation (every PR)**

- `helm lint` validates YAML syntax, required fields, and Helm template correctness for
  both `charts/observable-common` (library) and `charts/observable` (application).
- `helm template --dry-run` renders the full manifest set and confirms all library template
  calls resolve without errors.
- Run locally: `bash scripts/helm-lint.sh`
- Run in CI: `.github/workflows/pr.yml` → `helm` job.

**Layer 2 — kind cluster integration (every push to main + nightly)**

- `scripts/kind-test.sh` creates a fresh kind cluster, loads the local Docker image, deploys
  the infrastructure Helm chart (`charts/observable-infra`), creates migration ConfigMaps,
  installs the application chart, and verifies each service health endpoint responds.
- The smoke path sends an OTLP trace through ingest-gateway and queries it back via query-api,
  proving the ingest→Redpanda→stream-processor→ClickHouse→query-api pipeline round-trips in k8s.
- Rollback verification: the test upgrades to revision 2 and executes `helm rollback 1`,
  then asserts Deployment replica counts match revision 1 values.
- Run locally: `bash scripts/kind-test.sh`
- Run in CI: `.github/workflows/kind-test.yml` (triggered on push to main and nightly).

**Layer 3 — Shared integration cluster (release candidates)**

- Release candidate artifacts (immutable commit-tagged images) are deployed to a shared
  integration cluster using the same chart.
- Smoke, tenant-isolation, RBAC, and audit-log checks run against the shared cluster.
- This environment is not created by this PR; it is a future Phase 4 gate.

**What kind tests do not cover**

| Gap | Mitigation |
|-----|-----------|
| Persistent volume behaviour | Docker Compose smoke test covers storage round-trip |
| Multi-replica HA | ClickHouse/Redpanda Operator work deferred to Phase 4 |
| Network policy enforcement | Deferred; unit tests verify tenant isolation at code level |
| Operator-managed stateful services | In-cluster single-pod infra used for kind tests |
| Production resource sizing | Values tuned per environment via overrides file |

**Rollback test acceptance criteria**

A Kubernetes slice is considered verified when:
1. `helm lint` exits 0 with no warnings.
2. `helm template --dry-run` renders without errors.
3. `scripts/kind-test.sh` exits 0, meaning all service Deployments became `Available`
   and `helm rollback` returned the cluster to the previous revision.
4. No new errors are introduced in the Rust unit/integration suite.

**Schema rollback policy (referenced in 18.4 and ADR-013)**

Schema migrations are forward-only. Every migration file must be backward-compatible with the
immediately preceding application image version. CI gates for release candidates include a
matrix test that deploys the new schema against the previous image to verify compatibility.
If compatibility cannot be maintained, the migration is split into a zero-downtime multi-step
deployment (expand–migrate–contract pattern).
