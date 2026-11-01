# Observable Brownfield Onboarding & Remediation Feedback

Date: 2026-10-31.
Author: System Specification Pair Implementation.
Target System: [`c:\git\observable`](https://github.com/ktjn/observable) (`79b50478`).
Upstream Metamodel: [`c:\git\system-specification`](https://github.com/ktjn/system-specification) (`a801ec4`).

---

## 1. Executive Summary

`observable` is a distributed multi-tenant telemetry platform (Rust, TypeScript/React, Redpanda/Kafka, ClickHouse, PostgreSQL) onboarded into machine-checkable System Specification compliance across 6 sequential milestones (PRs #790 through #796).

All 6 milestones achieved 100% compliance:
- Level 0 (`inventory`): **PASS**
- Level 1 (`discovery`): **PASS**
- Level 2 (`architecture`): **PASS** (zero component boundary allowlist violations, strict 4-layer DAG)
- Level 2.5 (`verification`): **PASS** (100% verification category coverage, acyclic requirements)
- Level 3 (`release`): **PASS** (15 evidence records passing, valid provenance files and ISO 8601 timestamps)

This document captures feedback, observed facts, model gaps, and recommended evolutions discovered during the full lifecycle of onboarding and remediating `observable`.

---

## 2. Findings and Proposed Evolutions

| ID | Topic & Observed Fact | Metamodel Gap / Risk | Proposed Metamodel Evolution |
|---|---|---|---|
| **OBS-FB-01** | **Database Schema Isolation & Multi-Schema RDBMS Ownership**<br>Brownfield monoliths/distributed backends frequently share a single relational engine (e.g., PostgreSQL) partitioned into logical schemas (`auth.*`, `control.*`, `alerting.*`). Prior to remediation, foreign keys crossed component boundaries (`auth.* -> control.tenants`) and tables lacked explicit schema ownership (`public.query_audit_log`). | `DataModel.owner` models ownership at the logical entity level, but does not represent database-level storage isolation, namespace prefixes, or database user roles. Without runtime role-separation, components silently access each other's tables in shared databases. | Extend `DataModel` (or add guidance to `BROWNFIELD-INTAKE.md`) to capture `schema_prefix`/`namespace` and recommended `storage_role` isolation. Recommend that multi-component relational architectures forbid cross-schema foreign keys and enforce schema-scoped database roles (`observable_auth_role`, etc.) in CI migrations. |
| **OBS-FB-02** | **Direct Storage Consumption vs. Application Layer DAG Inversion (`COMP-05` / `COMP-06`)**<br>In `observable`, alerting rules are evaluated by `observable-alerting` (`domain`), which queries ClickHouse (`observable-store-clickhouse`, `infrastructure`). Meanwhile, `observable-query` (`application`) calls `observable-alerting` to enrich search results with alert statuses. If `observable-alerting` called `observable-query`, it would create a bidirectional cycle (`COMP-06`) and invert layer order (`COMP-05`, domain -> application). | The metamodel strictly forbids cycles and lower-to-higher layer calls. However, when a domain component queries shared storage directly, it needs a declared schema-compatibility range rather than an open dependency, to avoid hidden coupling to the storage writer's internal migrations. | Introduce support for declaring **schema compatibility matrices** or table-range dependencies on `Capability` / `Interface` (e.g. `schema_support_range: ">=030 <=042"`), matching the pattern established in `services/alert-evaluator/clickhouse-schema-support.yaml`. |
| **OBS-FB-03** | **At-Least-Once Delivery vs. Storage Idempotency Modeling**<br>In streaming telemetry ingest (`observable-store-clickhouse`), offset commits occur after batch inserts. Crash windows cause redelivery. Simply declaring `interface.idempotent: true` in the spec without protocol mechanisms led to duplicate visible rows in `MergeTree` tables. | A boolean `idempotent: true` flag on an interface does not specify *how* idempotency is achieved (deduplication tokens vs. natural keys vs. compaction vs. transactional outbox). | Add guidance or schema attributes to specify the idempotency mechanism (e.g., `idempotency_strategy: token | natural_key | outbox | replacing_merge_tree`) and require a concurrency/formal model (e.g., TLA+ `NoDuplicateVisibleRows`) for streaming write capabilities. |
| **OBS-FB-04** | **Evidence Provenance Resolution and Artifact Staleness**<br>During Milestone 6, initial evidence records in `spec/system.yaml` referenced non-existent provenance paths (`tests/performance/query-latency.js`, `apps/frontend/e2e/auth-tenant-isolation.spec.ts`) that had moved or been renamed. | Metamodel schema validation previously only checked string types for `provenance`. A spec could pass Level 3 release validation even if all evidence files were missing or renamed. | **Validated & Commended**: The recent addition of `--check-artifacts --root <path>` and `MOD-FB-06` directly addresses this gap. In `observable`, we implemented `scripts/update-spec-evidence.py --check` and unit tests in `test_system_spec.py` that verify file existence and ISO 8601 timestamps on every local CI run. Recommend standardizing `--check-artifacts` as mandatory for release gates. |
| **OBS-FB-05** | **Component Boundary Allowlist Zeroing Trajectory**<br>`observable` had an allowlist (`scripts/component-boundary-allowlist.txt`) that permitted undeclared internal Rust crate dependencies between `query-api` and `auth-service`. Remediating this required decoupling the integration tests into `auth-service` and exposing public wire contracts. | The metamodel provides an architectural DAG check, but doesn't prescribe a migration path for legacy allowlists or waivers. Teams often create perpetual allowlists that never shrink. | Add a recommendation in `CONSTITUTION.md` and `BROWNFIELD-INTAKE.md` that boundary allowlists must carry explicit expiration dates, zero-target milestones, or ratcheting mechanisms (e.g. CI fails if allowlist entry count increases). |

---

## 3. Realized Architecture & Verification Evidence

The resulting architecture achieved in `observable` cleanly demonstrates the feasibility and rigor of the System Specification standard:

1. **Strict 4-Layer Hierarchy**:
   - `experience`: `observable-web`
   - `application`: `observable-ingest`, `observable-query`
   - `domain`: `observable-control`, `observable-alerting`, `observable-process`
   - `infrastructure`: `observable-auth`, `observable-store-clickhouse`
2. **Zero Boundary Allowlist Violations**:
   - `scripts/component-boundary-allowlist.txt`: **0 entries**.
3. **Formal Verification Backing**:
   - `formal/tla/TelemetryDelivery.tla`: Verifies bounded concurrency and idempotent delivery without duplicates (`NoDuplicateVisibleRows`).
   - `formal/tla/ComponentCompatibility.tla`: Verifies schema version range compatibility across all 8 components.
4. **Continuous Spec-Driven Gate**:
   - Parity across `spec/system.yaml`, `VERSION`, `distribution/manifest.yaml`, and `models/*.mdl` is enforced on every commit via `scripts/tests/test_system_spec.py` and `scripts/local-ci.sh`.
