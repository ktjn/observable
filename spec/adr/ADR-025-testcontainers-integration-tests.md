# ADR-025: Testcontainers for Service Integration Tests

**Date:** 2026-04-27
**Status:** Accepted — implemented across all backend services
**Authors:** Tommy Alander
**Deciders:** Tommy Alander
**Review date:** 2027-04-27

## Context

The platform currently has three useful but different verification layers:

1. Unit tests for pure parsing, validation, policy, and query-construction behavior.
2. Docker Compose smoke tests through `scripts/local-ci.sh` and `tests/e2e/smoke_test.sh`.
3. Kubernetes integration tests through kind for Helm, rollout, and rollback behavior.

Those layers leave a gap for service-level integration tests that need real dependencies but should not boot the whole platform. Examples include repository code against PostgreSQL or ClickHouse, Redpanda producer/consumer behavior, migration compatibility, tenant isolation against real persisted rows, and failure handling when one dependency is unavailable.

Reusing the shared Docker Compose stack for these tests couples unrelated tests together, makes parallel execution harder, and can hide fixture leakage between runs. Pure unit tests are too weak for code paths whose correctness depends on database engines, Kafka-compatible brokers, or wire-level protocol behavior.

## Decision

Adopt Testcontainers as the standard harness for service-level integration tests that require real containerized dependencies but do not require the full Docker Compose or Kubernetes topology.

Rust service crates must use the Rust Testcontainers ecosystem for these tests. Integration tests that touch PostgreSQL, ClickHouse, Redpanda/Kafka-compatible brokers, OpenFGA, or object storage must prefer per-test or per-suite Testcontainers fixtures unless the slice explicitly requires full-stack Docker Compose, kind, browser, or external-provider behavior.

Testcontainers does not replace:

- unit tests for pure logic
- Docker Compose smoke tests for end-to-end local platform verification
- `scripts/local-ci.sh` as the mandatory pre-push code gate
- kind tests for Kubernetes packaging, rollout, and rollback
- frontend RTL/MSW integration tests that do not need real backend containers

## Consequences

**Easier:**
- Service integration tests can create isolated PostgreSQL, ClickHouse, Redpanda, or object-store fixtures without depending on long-lived local state.
- Tests can run closer to the code under test and can assert migrations, repository queries, tenant filtering, and producer/consumer behavior before the full smoke gate runs.
- Agent iterations get a narrower verification option when the changed surface is too broad for a unit test but too small to justify the full Compose stack.

**Harder:**
- Developers and CI runners must have Docker or a compatible container runtime available for container-backed integration tests.
- Test authors must manage startup readiness, deterministic fixtures, network ports, and cleanup explicitly.
- Testcontainers dependencies must stay current and be reviewed like other runtime-adjacent tooling.

**Constrained:**
- Integration tests that use real platform dependencies cannot use ad hoc long-lived local databases or brokers as their default verification path.
- A new service dependency should include either a Testcontainers fixture or a documented reason it must be covered only by Compose/kind.
- Agents must not skip applicable Testcontainers coverage simply because unit tests pass.

## Alternatives Considered

### Option A: Continue using Docker Compose for all real-dependency tests

Rejected. Compose remains the right end-to-end local platform gate, but it is too coarse for fast, isolated service integration tests. It also increases fixture coupling and makes it harder to identify which service boundary failed.

### Option B: Use only mocks and in-memory adapters

Rejected. Mocks are useful for unit tests, but they do not prove SQL compatibility, migration behavior, ClickHouse query semantics, Kafka protocol behavior, or container startup assumptions.

### Option C: Add shared long-lived local test databases

Rejected. Shared local services create ordering assumptions, stale state, and environment-specific failures. They also make test results harder for agents and reviewers to reproduce.

## Related

- [spec/11-testing.md](../11-testing.md) - testing layers, CI gates, and Testcontainers policy
- [spec/10-process.md](../10-process.md) - AI agent guidance and tiny iteration workflow
- [ADR-019: CI Workflows Must Delegate to Locally-Runnable Scripts](ADR-019-ci-scripts-runnable-locally.md)
- [ADR-020: Helm Chart Strategy](ADR-020-helm-chart-strategy.md)
