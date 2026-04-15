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
| Nightly extended | scheduled | E2E workflows, high-cardinality datasets, malformed payload corpus, chaos/resilience, dependency scans |
| Release candidate | release tag or promotion branch | full E2E, performance, security, backup/restore, rollback, upgrade, and migration tests |
| Production canary | progressive rollout | SLO analysis, alert latency, queue lag, ingest error rate, query latency, tenant isolation smoke |

Tests that guard tenant isolation, auth bypass, schema compatibility, migrations, and data loss are release blockers. Performance and chaos failures are promotion blockers unless explicitly waived by the responsible domain owner with a documented expiry.

### 18.5 Local Test Scope

Local tests optimize for fast feedback and confidence before PR submission. They do not replace CI gates.

| Scope | Local command | Purpose |
|-------|---------------|---------|
| Formatting | `just fmt` | keep generated and handwritten code consistent |
| Static checks | `just lint` | catch obvious Rust, TypeScript, docs, and config issues |
| Unit tests | `just test` | validate changed packages without external services |
| Contract checks | `just contract` | verify protobuf/OpenAPI compatibility and generated-code drift |
| Service smoke | `just smoke` | validate one service against local dependencies |
| CI approximation | `just ci-local` | run the closest practical local equivalent of required PR checks |

Local integration fixtures must include at least one tenant-isolation case, one malformed telemetry payload, one golden trace/log correlation bundle, and one high-cardinality sample slice for any feature that touches ingest, query, storage, auth, or schema logic.
